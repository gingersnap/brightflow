//! Curation overlay: durable cluster edits applied at READ time.
//!
//! `fit_topics` regenerates artifacts wholesale, so user edits live in SQLite
//! (`cluster_edits`, `excluded_terms`) and are overlaid onto whatever the
//! current fit produced: name overrides, noise hiding, merge remapping, and
//! excluded naming terms. The parquet `topic_cluster_id` stays RAW — consumers
//! that need effective ids use [`effective_cluster_map`].

use std::collections::{HashMap, HashSet};

use brightflow_store::{ClusterEditRow, ExcludedTermRow};

use crate::topics::types::ClusterSummary;

/// In-memory overlay for one table.
#[derive(Debug, Default, Clone)]
pub struct CurationOverlay {
    /// raw cluster id → custom display name
    pub names: HashMap<i64, String>,
    /// raw cluster id → curated label
    pub labels: HashMap<i64, String>,
    /// raw cluster ids hidden as noise
    pub noise: HashSet<i64>,
    /// raw cluster id → raw cluster id it merges into
    pub merges: HashMap<i64, i64>,
    /// lowercase terms excluded from naming
    pub excluded_terms: HashSet<String>,
    /// edits that lost their cluster after a re-fit and await review
    pub orphaned_edits: usize,
}

impl CurationOverlay {
    pub fn from_rows(edits: &[ClusterEditRow], terms: &[ExcludedTermRow]) -> Self {
        let mut overlay = Self {
            excluded_terms: terms.iter().map(|t| t.term.to_lowercase()).collect(),
            ..Self::default()
        };
        for edit in edits {
            if edit.orphaned {
                overlay.orphaned_edits += 1;
                continue;
            }
            let Some(cid) = edit.cluster_id else {
                overlay.orphaned_edits += 1;
                continue;
            };
            if let Some(name) = &edit.custom_name {
                if !name.is_empty() {
                    overlay.names.insert(cid, name.clone());
                }
            }
            if let Some(label) = &edit.label {
                if !label.is_empty() {
                    overlay.labels.insert(cid, label.clone());
                }
            }
            if edit.is_noise {
                overlay.noise.insert(cid);
            }
            if let Some(target) = edit.merged_into {
                overlay.merges.insert(cid, target);
            }
        }
        overlay
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
            && self.labels.is_empty()
            && self.noise.is_empty()
            && self.merges.is_empty()
            && self.excluded_terms.is_empty()
    }

    /// Follow merge chains to the final target (with a cycle guard).
    pub fn merge_target(&self, cluster_id: i64) -> i64 {
        let mut current = cluster_id;
        let mut hops = 0;
        while let Some(&next) = self.merges.get(&current) {
            if next == current || hops > 32 {
                break;
            }
            current = next;
            hops += 1;
        }
        current
    }

    /// Effective display name for a cluster, if curated: an explicit rename
    /// wins, otherwise the assigned label doubles as the display name.
    pub fn display_name(&self, cluster_id: i64) -> Option<&str> {
        self.names
            .get(&cluster_id)
            .or_else(|| self.labels.get(&cluster_id))
            .map(String::as_str)
    }

    /// Strip excluded terms from a term list.
    pub fn filter_terms(&self, terms: &mut Vec<String>) {
        if self.excluded_terms.is_empty() {
            return;
        }
        terms.retain(|t| !self.excluded_terms.contains(&t.to_lowercase()));
    }
}

/// Raw → effective cluster id map for `k` raw clusters. `None` = hidden
/// (noise). Exposed for consumers that read raw `topic_cluster_id` off
/// parquet (e.g. insights over topics).
pub fn effective_cluster_map(overlay: &CurationOverlay, k: usize) -> Vec<Option<i64>> {
    (0..k)
        .map(|raw| {
            let raw = i64::try_from(raw).unwrap_or(i64::MAX);
            let target = overlay.merge_target(raw);
            if overlay.noise.contains(&target) {
                None
            } else {
                Some(target)
            }
        })
        .collect()
}

/// Apply the overlay to cluster summaries:
/// name override → noise filter → merge remap (sizes summed, terms unioned)
/// → excluded terms. Returns rows hidden as noise (they become unassigned).
pub fn apply_to_summaries(summaries: &mut Vec<ClusterSummary>, overlay: &CurationOverlay) -> usize {
    if overlay.is_empty() {
        return 0;
    }

    // 1. Name overrides
    for s in summaries.iter_mut() {
        if let Some(name) = overlay.display_name(i64::from(s.id)) {
            s.name = name.to_string();
            s.curated = true;
        }
    }

    // 2. Noise filter (rows become unassigned)
    let mut noise_rows = 0;
    summaries.retain(|s| {
        if overlay.noise.contains(&i64::from(s.id)) {
            noise_rows += s.size;
            false
        } else {
            true
        }
    });

    // 3. Merge remap
    let mut by_target: HashMap<i64, ClusterSummary> = HashMap::new();
    let mut order: Vec<i64> = Vec::new();
    for s in summaries.drain(..) {
        let target = overlay.merge_target(i64::from(s.id));
        if overlay.noise.contains(&target) {
            noise_rows += s.size;
            continue;
        }
        match by_target.get_mut(&target) {
            None => {
                order.push(target);
                let mut merged = s;
                merged.id = i32::try_from(target).unwrap_or(merged.id);
                // Target may itself have a name override
                if let Some(name) = overlay.display_name(target) {
                    merged.name = name.to_string();
                    merged.curated = true;
                }
                by_target.insert(target, merged);
            },
            Some(existing) => {
                existing.size += s.size;
                for term in s.top_terms {
                    if !existing.top_terms.contains(&term) {
                        existing.top_terms.push(term);
                    }
                }
                for title in s.sample_titles {
                    if existing.sample_titles.len() < 6 && !existing.sample_titles.contains(&title)
                    {
                        existing.sample_titles.push(title);
                    }
                }
            },
        }
    }
    *summaries = order
        .into_iter()
        .filter_map(|t| by_target.remove(&t))
        .collect();
    summaries.sort_by_key(|s| std::cmp::Reverse(s.size));

    // 4. Excluded terms
    for s in summaries.iter_mut() {
        overlay.filter_terms(&mut s.top_terms);
    }

    noise_rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit(cluster_id: i64) -> ClusterEditRow {
        ClusterEditRow {
            id: cluster_id,
            table_id: "t".to_string(),
            centroid_fingerprint: format!("fp{cluster_id}"),
            centroid_json: "[]".to_string(),
            cluster_id: Some(cluster_id),
            custom_name: None,
            label: None,
            is_noise: false,
            merged_into: None,
            orphaned: false,
            updated_at: 0,
        }
    }

    fn summary(id: i32, size: usize, terms: &[&str]) -> ClusterSummary {
        ClusterSummary {
            id,
            name: format!("cluster {id}"),
            size,
            top_terms: terms.iter().map(|t| (*t).to_string()).collect(),
            sample_titles: Vec::new(),
            top_labels: Vec::new(),
            curated: false,
        }
    }

    #[test]
    fn overlay_applies_in_order() {
        let mut rename = edit(0);
        rename.custom_name = Some("Payments".to_string());
        let mut noisy = edit(1);
        noisy.is_noise = true;
        let mut merged = edit(2);
        merged.merged_into = Some(0);
        let terms = vec![ExcludedTermRow {
            id: 1,
            table_id: "t".to_string(),
            term: "the".to_string(),
            created_at: 0,
        }];
        let overlay = CurationOverlay::from_rows(&[rename, noisy, merged], &terms);

        let mut summaries = vec![
            summary(0, 100, &["billing", "the"]),
            summary(1, 50, &["spam"]),
            summary(2, 30, &["invoices"]),
        ];
        let noise_rows = apply_to_summaries(&mut summaries, &overlay);

        assert_eq!(noise_rows, 50, "noise cluster rows become unassigned");
        assert_eq!(summaries.len(), 1, "merge folded 2 into 0, noise dropped 1");
        let s = &summaries[0];
        assert_eq!(s.name, "Payments");
        assert_eq!(s.size, 130);
        assert!(s.top_terms.contains(&"invoices".to_string()));
        assert!(!s.top_terms.contains(&"the".to_string()), "excluded term");
    }

    #[test]
    fn label_doubles_as_display_name_unless_renamed() {
        let mut labeled = edit(0);
        labeled.label = Some("Payments".to_string());
        let mut both = edit(1);
        both.label = Some("Billing label".to_string());
        both.custom_name = Some("Billing".to_string());
        let overlay = CurationOverlay::from_rows(&[labeled, both], &[]);
        assert_eq!(overlay.display_name(0), Some("Payments"));
        assert_eq!(overlay.display_name(1), Some("Billing"), "rename wins");

        let mut summaries = vec![summary(0, 10, &["a"]), summary(2, 5, &["b"])];
        apply_to_summaries(&mut summaries, &overlay);
        assert_eq!(summaries[0].name, "Payments");
        assert!(summaries[0].curated);
        assert!(!summaries[1].curated, "untouched cluster stays uncurated");
    }

    #[test]
    fn merge_chains_resolve_with_cycle_guard() {
        let mut a = edit(1);
        a.merged_into = Some(2);
        let mut b = edit(2);
        b.merged_into = Some(1); // cycle
        let overlay = CurationOverlay::from_rows(&[a, b], &[]);
        // Must terminate
        let _ = overlay.merge_target(1);
    }

    #[test]
    fn orphaned_edits_counted_not_applied() {
        let mut orphan = edit(3);
        orphan.orphaned = true;
        orphan.custom_name = Some("Lost".to_string());
        let overlay = CurationOverlay::from_rows(&[orphan], &[]);
        assert_eq!(overlay.orphaned_edits, 1);
        assert!(overlay.names.is_empty());
    }

    #[test]
    fn effective_map_hides_noise_and_remaps() {
        let mut noisy = edit(0);
        noisy.is_noise = true;
        let mut merged = edit(2);
        merged.merged_into = Some(1);
        let overlay = CurationOverlay::from_rows(&[noisy, merged], &[]);
        let map = effective_cluster_map(&overlay, 3);
        assert_eq!(map, vec![None, Some(1), Some(1)]);
    }
}
