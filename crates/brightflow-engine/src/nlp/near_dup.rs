//! Near-duplicate detection over dense embeddings.
//!
//! # Scope: near-duplicates, *not* "related issues"
//!
//! This finds rows whose text is **substantially the same string**: copy-paste
//! reports, re-filed tickets, automated submissions from a template, the same
//! crash pasted twice with a different timestamp. It does **not** find two
//! differently-worded reports of the same underlying bug — "login button does
//! nothing" and "cannot authenticate on the sign-in page" describe one issue
//! but share almost no surface form, and they will not clear the threshold.
//! Do not market this as semantic issue-linking; it is a de-duplicator.
//!
//! That limit is deliberate. Static token-averaged embeddings (Model2Vec) have
//! a dominant axis of variance that tracks **surface format and vocabulary**
//! rather than intent — which is a liability for unsupervised clustering (see
//! [`crate::nlp::linear`] for why supervision is the fix there), but is exactly
//! the right bias *here*. Near-duplicate detection wants a similarity that
//! rises when the wording matches and stays low otherwise. The format bias is a
//! feature in this module and a bug in the others.
//!
//! # Algorithm
//!
//! 1. **Exact pre-pass.** Rows are fingerprinted with
//!    [`crate::nlp::fingerprint::fingerprint`] over their cleaned text.
//!    Identical fingerprints are unioned immediately — this catches verbatim
//!    copy-paste in O(n) hashing, before any float math.
//! 2. **Blocked O(n²) dot product.** Survivors are compared pairwise in cache-
//!    friendly chunks. Vectors are L2-normalized, so cosine == dot product
//!    ([`crate::nlp::dense_cosine`]). No n×n matrix is ever materialized; only
//!    pairs at or above the threshold are recorded.
//! 3. **Union-find grouping** (path compression + union by rank) so transitive
//!    duplicates land in a single group: if a≈b and b≈c, all three group even
//!    when a and c fall just short of the threshold on their own.
//!
//! # Bounds — read this before raising the cap or pointing it at a big table
//!
//! The pairwise pass is quadratic and exact — there is no ANN index and no new
//! dependency. Above [`MAX_NEAR_DUP_ROWS`] rows the work is refused with
//! [`NearDupError::TooManyRows`] rather than crawling silently.
//!
//! **The cap does not bound wall-clock to anything comfortable.** Measured on a
//! real GitHub `issues` corpus (release build, 512-dim `potion-base-32M`):
//!
//! ```text
//!    2,326 rows  ->  ~6 seconds      (measured)
//!   25,000 rows  ->  ~6 minutes      (extrapolated, n^2)
//!   71,000 rows  ->  ~45 minutes     (extrapolated)
//!  100,000 rows  ->  ~85 minutes     (extrapolated — i.e. AT the cap)
//! ```
//!
//! So a table of ~71k issues sits *under* the cap and still runs for the better
//! part of an hour with no progress output. Treat [`MAX_NEAR_DUP_ROWS`] as a
//! backstop against the absurd, not as a promise that anything below it is
//! quick. Run this on a filtered slice, or expect to wait.

use std::collections::HashMap;

use crate::nlp::dense_cosine;
use crate::nlp::fingerprint::fingerprint;

/// Cosine similarity at or above which two rows are considered near-duplicates.
///
/// 0.9 is tuned for *surface* similarity on L2-normalized static embeddings:
/// reworded text lands well below it, while boilerplate edits (a changed id, a
/// different timestamp) stay above.
pub const DEFAULT_NEAR_DUP_THRESHOLD: f32 = 0.9;

/// Hard cap on input rows.
///
/// The pairwise pass is O(n²); at this size it is already ~5×10⁹ dot products
/// (~85 minutes — see the module docs for measured scaling), and beyond it the
/// honest answer is "use an ANN index", not "wait longer".
///
/// This is a backstop against the absurd, NOT a latency guarantee: real corpora
/// well under it still take tens of minutes.
pub const MAX_NEAR_DUP_ROWS: usize = 100_000;

/// Rows per block in the blocked similarity pass. Sized so one block's vectors
/// stay resident in L2 while the inner block streams past.
const BLOCK_ROWS: usize = 256;

/// Why near-duplicate detection could not run.
#[derive(Debug, thiserror::Error)]
pub enum NearDupError {
    /// Input exceeds [`MAX_NEAR_DUP_ROWS`].
    #[error("too many rows for exact near-duplicate detection: {rows} > {max}")]
    TooManyRows { rows: usize, max: usize },

    /// `embeddings` and `texts` describe different row counts.
    #[error("length mismatch: {embeddings} embeddings vs {texts} texts")]
    LengthMismatch { embeddings: usize, texts: usize },

    /// Threshold is outside the meaningful cosine range.
    #[error("threshold must be finite and within [-1.0, 1.0], got {0}")]
    InvalidThreshold(f32),

    /// Embedding vectors disagree on dimensionality.
    #[error("inconsistent embedding dimension: row {row} has {found}, expected {expected}")]
    InconsistentDimension {
        row: usize,
        found: usize,
        expected: usize,
    },
}

/// A set of rows that are near-duplicates of one another.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NearDupGroup {
    /// Row indices into the original input, ascending. Always >= 2 entries.
    pub rows: Vec<usize>,
    /// True when every member shares one fingerprint — a verbatim duplicate
    /// group found by the exact pre-pass, with no embedding math involved.
    pub exact: bool,
}

/// Group rows into near-duplicate sets.
///
/// `embeddings` are expected L2-normalized and index-aligned with `texts`. A
/// row with either a `None` embedding or `None` text is skipped: it can be
/// neither fingerprinted nor compared, so it never joins a group.
///
/// Returns only groups of size >= 2, in deterministic order: members ascending,
/// groups by their lowest member.
///
/// # Errors
///
/// See [`NearDupError`] — oversized input, mismatched lengths, an out-of-range
/// threshold, or ragged embedding dimensions.
pub fn find_near_duplicates(
    embeddings: &[Option<Vec<f32>>],
    texts: &[Option<String>],
    threshold: f32,
) -> Result<Vec<NearDupGroup>, NearDupError> {
    if embeddings.len() != texts.len() {
        return Err(NearDupError::LengthMismatch {
            embeddings: embeddings.len(),
            texts: texts.len(),
        });
    }
    if !threshold.is_finite() || !(-1.0..=1.0).contains(&threshold) {
        return Err(NearDupError::InvalidThreshold(threshold));
    }
    if embeddings.len() > MAX_NEAR_DUP_ROWS {
        return Err(NearDupError::TooManyRows {
            rows: embeddings.len(),
            max: MAX_NEAR_DUP_ROWS,
        });
    }

    // Eligible rows: both an embedding and a text.
    let eligible: Vec<usize> = (0..embeddings.len())
        .filter(|&i| {
            embeddings.get(i).is_some_and(Option::is_some)
                && texts.get(i).is_some_and(Option::is_some)
        })
        .collect();
    if eligible.len() < 2 {
        return Ok(Vec::new());
    }

    let dim = eligible
        .first()
        .and_then(|&i| embeddings.get(i))
        .and_then(Option::as_ref)
        .map_or(0, Vec::len);
    for &i in &eligible {
        let found = embeddings
            .get(i)
            .and_then(Option::as_ref)
            .map_or(0, Vec::len);
        if found != dim {
            return Err(NearDupError::InconsistentDimension {
                row: i,
                found,
                expected: dim,
            });
        }
    }

    let mut uf = UnionFind::new(embeddings.len());

    // Pass 1: exact fingerprint pre-pass over cleaned text.
    //
    // `fp_group[r]` records WHICH fingerprint group row r belongs to (by its
    // first-seen representative), not merely that it has one. That distinction
    // decides the `exact` flag below: a plain "this row had a twin" boolean
    // would call a group exact even when pass 2 fused two *different*
    // fingerprint groups by cosine, which is precisely a near-match, not an
    // exact one.
    let mut by_fp: HashMap<String, usize> = HashMap::new();
    let mut fp_group: Vec<Option<usize>> = vec![None; embeddings.len()];
    for &i in &eligible {
        let Some(Some(text)) = texts.get(i) else {
            continue;
        };
        let fp = fingerprint(&[text.as_str()]);
        // `first` is copied out, so the immutable borrow of `by_fp` ends before
        // the else branch inserts.
        if let Some(&first) = by_fp.get(&fp) {
            uf.union(first, i);
            if let Some(slot) = fp_group.get_mut(i) {
                *slot = Some(first);
            }
        } else {
            let _ = by_fp.insert(fp, i);
            if let Some(slot) = fp_group.get_mut(i) {
                *slot = Some(i);
            }
        }
    }

    // Pass 2: blocked pairwise cosine over the survivors — one representative
    // per exact group, since exact dupes are already unioned and share a vector
    // neighbourhood.
    let survivors: Vec<usize> = eligible
        .iter()
        .copied()
        .filter(|&i| uf.find(i) == i)
        .collect();

    let vectors: Vec<&[f32]> = survivors
        .iter()
        .filter_map(|&i| {
            embeddings
                .get(i)
                .and_then(Option::as_ref)
                .map(Vec::as_slice)
        })
        .collect();

    let n = survivors.len();
    for outer in (0..n).step_by(BLOCK_ROWS) {
        let outer_end = (outer + BLOCK_ROWS).min(n);
        for inner in (outer..n).step_by(BLOCK_ROWS) {
            let inner_end = (inner + BLOCK_ROWS).min(n);
            for a in outer..outer_end {
                // Within the diagonal block, only look forward.
                let start = if inner <= a { a + 1 } else { inner };
                for b in start..inner_end {
                    let (Some(va), Some(vb)) = (vectors.get(a), vectors.get(b)) else {
                        continue;
                    };
                    if dense_cosine(va, vb) >= threshold {
                        let (Some(&ra), Some(&rb)) = (survivors.get(a), survivors.get(b)) else {
                            continue;
                        };
                        uf.union(ra, rb);
                    }
                }
            }
        }
    }

    // Collect groups, deterministically.
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
    for &i in &eligible {
        members.entry(uf.find(i)).or_default().push(i);
    }

    let mut groups: Vec<NearDupGroup> = members
        .into_values()
        .filter(|rows| rows.len() >= 2)
        .map(|mut rows| {
            rows.sort_unstable();
            // Exact only when every member shares ONE fingerprint — i.e. the
            // group came purely from the pre-pass and no cosine merge widened it.
            let first_fp = rows
                .first()
                .and_then(|&r| fp_group.get(r).copied().flatten());
            let exact = first_fp.is_some()
                && rows
                    .iter()
                    .all(|&r| fp_group.get(r).copied().flatten() == first_fp);
            NearDupGroup { rows, exact }
        })
        .collect();
    groups.sort_by(|a, b| a.rows.first().cmp(&b.rows.first()));

    Ok(groups)
}

/// Disjoint-set forest with path compression and union by rank.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<u32>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
        }
    }

    /// Representative of `x`'s set, compressing the path on the way up.
    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent.get(root).copied().unwrap_or(root) != root {
            root = self.parent.get(root).copied().unwrap_or(root);
        }
        let mut cur = x;
        while cur != root {
            let next = self.parent.get(cur).copied().unwrap_or(cur);
            if let Some(slot) = self.parent.get_mut(cur) {
                *slot = root;
            }
            cur = next;
        }
        root
    }

    /// Merge the sets containing `a` and `b`; smaller rank hangs off larger.
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra == rb {
            return;
        }
        let (rank_a, rank_b) = (
            self.rank.get(ra).copied().unwrap_or(0),
            self.rank.get(rb).copied().unwrap_or(0),
        );
        let (child, root) = if rank_a < rank_b { (ra, rb) } else { (rb, ra) };
        if let Some(slot) = self.parent.get_mut(child) {
            *slot = root;
        }
        if rank_a == rank_b {
            if let Some(slot) = self.rank.get_mut(root) {
                *slot += 1;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unnecessary_wraps)]
mod tests {
    use super::*;

    /// L2-normalize in place.
    fn norm(mut v: Vec<f32>) -> Vec<f32> {
        let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if n > 0.0 {
            for x in &mut v {
                *x /= n;
            }
        }
        v
    }

    fn text(s: &str) -> Option<String> {
        Some(s.to_string())
    }

    #[test]
    fn union_find_path_compression_and_rank() {
        let mut uf = UnionFind::new(6);
        uf.union(0, 1);
        uf.union(1, 2);
        uf.union(4, 5);
        assert_eq!(uf.find(0), uf.find(2));
        assert_eq!(uf.find(4), uf.find(5));
        assert_ne!(uf.find(0), uf.find(4));
        assert_ne!(uf.find(0), uf.find(3));
        uf.union(2, 5);
        assert_eq!(uf.find(0), uf.find(5));
        // Path compression: every member points straight at the root.
        let root = uf.find(0);
        for i in [0, 1, 2, 4, 5] {
            assert_eq!(uf.find(i), root);
            assert_eq!(uf.parent[i], root);
        }
    }

    #[test]
    fn exact_prepass_groups_identical_text() {
        // Deliberately orthogonal vectors: only the fingerprint pass can group
        // these, proving the pre-pass runs independently of the cosine pass.
        let embeddings = vec![
            Some(norm(vec![1.0, 0.0, 0.0])),
            Some(norm(vec![0.0, 1.0, 0.0])),
            Some(norm(vec![0.0, 0.0, 1.0])),
        ];
        let texts = vec![
            text("crash on save"),
            text("crash on save"),
            text("totally other"),
        ];
        let groups = find_near_duplicates(&embeddings, &texts, DEFAULT_NEAR_DUP_THRESHOLD).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].rows, vec![0, 1]);
        assert!(groups[0].exact, "identical text must be flagged exact");
    }

    #[test]
    fn near_dupes_group_transitively() {
        // a≈b (>=0.9) and b≈c (>=0.9) but a·c < 0.9 — union-find must still
        // put all three in one group.
        let a = norm(vec![1.0, 0.0]);
        let b = norm(vec![1.0, 0.36]);
        let c = norm(vec![1.0, 0.75]);
        assert!(dense_cosine(&a, &b) >= 0.9);
        assert!(dense_cosine(&b, &c) >= 0.9);
        assert!(
            dense_cosine(&a, &c) < 0.9,
            "precondition: a and c are not direct dupes"
        );

        let embeddings = vec![Some(a), Some(b), Some(c)];
        let texts = vec![text("alpha one"), text("alpha two"), text("alpha three")];
        let groups = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].rows, vec![0, 1, 2]);
        assert!(!groups[0].exact, "distinct text is not an exact group");
    }

    #[test]
    fn below_threshold_does_not_group() {
        let embeddings = vec![
            Some(norm(vec![1.0, 0.0])),
            Some(norm(vec![0.0, 1.0])),
            Some(norm(vec![1.0, 1.0])),
        ];
        let texts = vec![text("one"), text("two"), text("three")];
        // 1·1 rows sit at cos=0.707 to the axes — all below 0.9.
        let groups = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert!(groups.is_empty(), "nothing should group: {groups:?}");
    }

    #[test]
    fn none_embeddings_and_texts_are_skipped() {
        let v = norm(vec![1.0, 0.0]);
        let embeddings = vec![
            Some(v.clone()),
            None,            // no embedding
            Some(v.clone()), // groups with row 0
            Some(v),         // has a vector but no text
        ];
        let texts = vec![text("dup"), text("dup"), text("dup"), None];
        let groups = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].rows,
            vec![0, 2],
            "None rows must not join a group"
        );
    }

    #[test]
    fn over_max_rows_errors() {
        let embeddings = vec![None; MAX_NEAR_DUP_ROWS + 1];
        let texts = vec![None; MAX_NEAR_DUP_ROWS + 1];
        let err = find_near_duplicates(&embeddings, &texts, 0.9).unwrap_err();
        assert!(
            matches!(
                err,
                NearDupError::TooManyRows { rows, max }
                    if rows == MAX_NEAR_DUP_ROWS + 1 && max == MAX_NEAR_DUP_ROWS
            ),
            "expected TooManyRows, got {err:?}"
        );

        // Exactly at the cap is allowed.
        let capped_embeddings = vec![None; MAX_NEAR_DUP_ROWS];
        let capped_texts = vec![None; MAX_NEAR_DUP_ROWS];
        assert!(find_near_duplicates(&capped_embeddings, &capped_texts, 0.9).is_ok());
    }

    #[test]
    fn length_mismatch_and_bad_threshold_error() {
        let embeddings = vec![Some(norm(vec![1.0, 0.0]))];
        assert!(matches!(
            find_near_duplicates(&embeddings, &[], 0.9).unwrap_err(),
            NearDupError::LengthMismatch { .. }
        ));
        let texts = vec![text("x")];
        assert!(matches!(
            find_near_duplicates(&embeddings, &texts, 1.5).unwrap_err(),
            NearDupError::InvalidThreshold(_)
        ));
        assert!(matches!(
            find_near_duplicates(&embeddings, &texts, f32::NAN).unwrap_err(),
            NearDupError::InvalidThreshold(_)
        ));
    }

    #[test]
    fn ragged_dimensions_error() {
        let embeddings = vec![Some(norm(vec![1.0, 0.0])), Some(norm(vec![1.0, 0.0, 0.0]))];
        let texts = vec![text("a"), text("b")];
        assert!(matches!(
            find_near_duplicates(&embeddings, &texts, 0.9).unwrap_err(),
            NearDupError::InconsistentDimension { row: 1, .. }
        ));
    }

    #[test]
    fn deterministic_and_sorted() {
        // Enough rows to cross several blocks, with planted duplicate pairs.
        let n = BLOCK_ROWS * 2 + 37;
        let mut embeddings = Vec::with_capacity(n);
        let mut texts = Vec::with_capacity(n);
        for i in 0..n {
            let angle = (i / 2) as f32 * 0.7;
            embeddings.push(Some(norm(vec![angle.cos(), angle.sin()])));
            texts.push(text(&format!("row {i}")));
        }
        let first = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        let second = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert_eq!(first, second, "repeat runs must agree");
        assert!(!first.is_empty(), "planted pairs should group");

        // Groups sorted by lowest member; members ascending; size >= 2.
        let mut prev = None;
        for g in &first {
            assert!(g.rows.len() >= 2);
            assert!(g.rows.windows(2).all(|w| w[0] < w[1]), "members not sorted");
            let lead = g.rows[0];
            if let Some(p) = prev {
                assert!(p < lead, "groups not sorted by lowest member");
            }
            prev = Some(lead);
        }
    }

    #[test]
    fn exact_and_near_merge_into_one_group() {
        let v = norm(vec![1.0, 0.0]);
        let near = norm(vec![1.0, 0.3]);
        assert!(dense_cosine(&v, &near) >= 0.9);
        let embeddings = vec![Some(v.clone()), Some(v), Some(near)];
        // Rows 0/1 are verbatim; row 2 is a near-dupe of both.
        let texts = vec![text("same text"), text("same text"), text("similar text")];
        let groups = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].rows, vec![0, 1, 2]);
        assert!(
            !groups[0].exact,
            "a group containing a non-verbatim member is not exact"
        );
    }

    /// Two DISTINCT verbatim groups fused by cosine are near-dupes, not exact.
    ///
    /// Regression: a per-row "this row had a twin" boolean is true for every
    /// member here (each row does have a verbatim twin), so it reported the
    /// fused group as `exact` — claiming four verbatim copies where there are
    /// two pairs of two. The CLI prints that flag verbatim, so the report lied.
    #[test]
    fn two_exact_groups_fused_by_cosine_are_not_exact() {
        let a = norm(vec![1.0, 0.0]);
        let b = norm(vec![1.0, 0.3]);
        assert!(dense_cosine(&a, &b) >= 0.9, "the two groups must fuse");

        let embeddings = vec![Some(a.clone()), Some(a), Some(b.clone()), Some(b)];
        let texts = vec![
            text("alpha text"),
            text("alpha text"),
            text("beta text"),
            text("beta text"),
        ];

        let groups = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert_eq!(
            groups.len(),
            1,
            "cosine must fuse the two fingerprint groups"
        );
        assert_eq!(groups[0].rows, vec![0, 1, 2, 3]);
        assert!(
            !groups[0].exact,
            "two different texts cannot form an exact group, even though every \
             member has a verbatim twin"
        );
    }

    /// The complementary case: a group built purely from one fingerprint stays
    /// exact even after pass 2 runs over its representative.
    #[test]
    fn single_fingerprint_group_stays_exact() {
        let v = norm(vec![1.0, 0.0]);
        let far = norm(vec![0.0, 1.0]);
        let embeddings = vec![Some(v.clone()), Some(v.clone()), Some(v), Some(far)];
        let texts = vec![text("same"), text("same"), text("same"), text("unrelated")];

        let groups = find_near_duplicates(&embeddings, &texts, 0.9).unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].rows, vec![0, 1, 2]);
        assert!(groups[0].exact, "one fingerprint, three rows => exact");
    }

    #[test]
    fn empty_and_single_row_inputs() {
        assert!(find_near_duplicates(&[], &[], 0.9).unwrap().is_empty());
        let embeddings = vec![Some(norm(vec![1.0, 0.0]))];
        let texts = vec![text("solo")];
        assert!(find_near_duplicates(&embeddings, &texts, 0.9)
            .unwrap()
            .is_empty());
    }
}
