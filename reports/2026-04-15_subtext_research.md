# v0.1 Spec: Cheap Text Signals for Rust

> **Research snapshot, 2026-04-15.** A record of what was designed and believed on that
> date, moved here from `docs/` unchanged. Not maintained — where this describes shipped
> behavior, the source is authoritative and this may have drifted.

A focused Rust library providing the cheap-text-signal primitives needed for
"always on, signals float up" data platforms. Designed to fit naturally with
the Polars/DataFusion/Arrow/Tokio stack.

## What this is

A small set of well-composed primitives for tokenization, n-grams, TF-IDF, and
similarity — enough to build real text tagging/classification/dedup features
without reaching for Python or pulling in a transformer model.

## What this is not

- Not a port of spaCy or NLTK
- Not a pipeline framework
- Not a transformer/embedding library (that's `fastembed-rs` / `ort`'s job)
- Not a search engine (that's `tantivy`'s job)
- Not a training framework — train models elsewhere, consume artifacts here

## Design philosophy

- **Library, not framework.** Small composable pieces. Users (and other libs) compose.
- **Primitives, not pipelines.** No orchestration layer; that's what Polars expressions are for.
- **Cheap enough to run always.** Microsecond-to-low-millisecond per document.
- **Zero-copy where natural.** Tokenizer returns spans into the input, not allocated strings.
- **Single binary friendly.** Pure Rust, no C++ runtime deps, no Python.
- **Boring API.** Builder patterns, single error type, no `unwrap()` in public paths.

## Acceptance criterion (the README example must compile and run)

```rust
use cheap_text::{Tokenizer, Vocabulary, TfIdf, cosine};
use std::collections::HashMap;

// Fit on a corpus
let tokenizer = Tokenizer::code_aware();
let mut vocab = Vocabulary::new();

let docs: Vec<&str> = load_issues();

let mut tfidf = TfIdf::builder()
    .ngram_range(1, 2)
    .min_df(3)
    .build();
tfidf.fit(&tokenizer, &mut vocab, &docs);

// Build label prototypes (one TF-IDF vector per label)
let label_centroids: HashMap<String, _> = build_centroids(&tfidf, &tokenizer, &vocab);

// Score a new document
let new_issue = "memory leak when processing large files";
let vec = tfidf.transform(&tokenizer, &vocab, new_issue);

let mut scores: Vec<(&String, f32)> = label_centroids
    .iter()
    .map(|(label, centroid)| (label, cosine(&vec, centroid)))
    .collect();
scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
```

If this snippet feels awkward, the API is wrong. Iterate the API until it reads cleanly.

## Scope of v0.1

Build exactly these pieces, in this order:

1. **Tokenizer** — code-aware word tokenizer, configurable
2. **Vocabulary** — token→id mapping with frequency tracking and min/max_df filtering
3. **N-gram generation** — word n-grams over the token stream, configurable range
4. **Sparse vector type** — the substrate; designed once, used by everything
5. **TF-IDF** — fit on a corpus (with incremental IDF updates), transform docs
6. **Cosine similarity** — over sparse vectors

Everything else is v0.2+.

## Component specs

### 1. Tokenizer

A word tokenizer that produces tokens suitable for TF-IDF over code-mixed text
(GitHub issues, support tickets, logs, code-adjacent prose).

**Requirements:**
- Returns an iterator of `TokenSpan { start: usize, end: usize }` (byte offsets into input)
- Iterator-based, no allocation per token, lifetimes tie spans to input
- Configurable preservation of:
  - Identifiers with `_` and CamelCase (`NullPointerException`, `use_after_free`)
  - Paths/namespaces (`tokio::spawn`, `src/main.rs`)
  - Error codes (`E0308`, `ECONNREFUSED`)
  - Version strings (`v1.2.3`)
- Configurable case-folding (default: lowercase)
- Configurable stopword filtering (user supplies the list, or none)
- Optional CamelCase splitting that emits BOTH the compound and its parts
  (so `NullPointerException` → `nullpointerexception`, `null`, `pointer`, `exception`)

**Implementation hints:**
- Build on `unicode-segmentation` for the base word splitting
- Use `regex` for special-token detection (paths, error codes, versions)
- `Tokenizer::code_aware()` and `Tokenizer::plain()` as the two preset constructors
- `Tokenizer::builder()` for full control

**Reference:** spaCy's tokenizer (prefix/suffix/infix rules) for handling URLs,
emails, contractions correctly. Don't port it; understand it.

### 2. Vocabulary

Maps tokens (and n-grams) to stable u32 ids. Tracks document frequency.

**Requirements:**
- `add(token: &str) -> TokenId` (assigns new id or returns existing)
- `get(token: &str) -> Option<TokenId>`
- `len() -> usize`
- Frequency tracking: per-token document-frequency counter (how many distinct
  docs contained this token)
- `filter_extremes(min_df, max_df)` — drops tokens outside the band, returns
  a remapping so existing sparse vectors can be updated if needed
- `serde` derives so users can persist/load via their format of choice

**Reference:** gensim's `Dictionary` class — exactly the right shape, port the
API design. Not the implementation, just the shape.

### 3. N-gram generation

Word n-grams over a token iterator.

**Requirements:**
- `ngrams(tokens: impl Iterator<Item = &str>, range: (usize, usize)) -> impl Iterator<Item = String>`
- For range (1, 2): emits unigrams and bigrams
- Joins multi-token n-grams with a separator (e.g., `"memory leak"` or
  `"memory_leak"` — pick one and document it; space is conventional)
- Allocates one String per n-gram (unavoidable for n>1; that's fine)

**Note:** Trigram support should work but isn't a focus. Most value is in
unigram + bigram.

### 4. Sparse vector type

The substrate everything composes through. Get this right first.

**Requirements:**
- Public type: `SparseVec<F: Float>` (or generic over numeric, but f32/f64 are
  the realistic uses; start with f32)
- Internal layout: `indices: Vec<u32>` and `values: Vec<F>`, sorted by index,
  no duplicate indices
- Operations needed by v0.1:
  - `dot(&self, other: &Self) -> F` (efficient sorted-merge implementation)
  - `l2_norm(&self) -> F`
  - `normalize(&mut self)` (in-place L2 normalization)
  - Iteration over `(index, value)` pairs
  - Construction from sorted pairs (debug-assert sorted; trust release)
  - Construction from unsorted pairs (sorts and deduplicates by summing)
- `serde` derives

**Decision:** Roll your own (≈200 lines), don't depend on `sprs`. The library
is small enough that controlling the core type is worth the duplication, and
you can swap to `sprs` later if needed without breaking the public API
(because the type stays yours).

### 5. TF-IDF

Fit IDF on a corpus, transform documents into sparse vectors.

**Requirements:**
- `TfIdf::builder()` with options:
  - `ngram_range(min, max)` — default (1, 1)
  - `min_df(usize)` — default 1
  - `max_df(f32)` — default 1.0 (fraction of corpus)
  - `sublinear_tf(bool)` — default true (use 1 + log(tf))
  - `norm: Option<Norm::L2>` — default Some(L2)
  - `idf_smoothing(bool)` — default true (add-one smoothing on IDF)
- `fit(&mut self, &Tokenizer, &mut Vocabulary, &[&str])` — fits IDF, populates vocab
- `fit_extend(&mut self, ..., &[&str])` — incremental: update doc counts, recompute IDF
  (this matters for "always on" — daily corpus updates shouldn't require full refit)
- `transform(&self, &Tokenizer, &Vocabulary, &str) -> SparseVec<f32>` — single doc
- `transform_batch(...)` — convenience for many docs

**Reference:** scikit-learn's `TfidfVectorizer` — read the source, port the
test suite, match the math. Sklearn's exact formulas (sublinear_tf, smoothing,
norm interaction) are the de facto standard and worth matching unless there's
a reason not to.

**Pitfall:** the interaction between `min_df` filtering and IDF computation is
subtle. Filter the vocabulary first, then compute IDF on the filtered set. Use
sklearn's behavior as ground truth and write tests that match.

### 6. Cosine similarity

`pub fn cosine(a: &SparseVec<f32>, b: &SparseVec<f32>) -> f32`

If both vectors are pre-normalized (default after TF-IDF), this is just `dot(a, b)`.
Make this a fast path. Document clearly that pre-normalization makes cosine
trivial; users building label centroids should normalize.

Also expose `pub fn cosine_unnormalized(a, b) -> f32` for the case where
vectors aren't pre-normalized.

## Cross-cutting requirements

### Error handling
- One crate-wide `Error` enum with `thiserror::Error` derive
- `pub type Result<T> = std::result::Result<T, Error>`
- No `unwrap()` or `panic!` on the public API path — including weird Unicode input

### API hygiene
- Builder patterns for anything with options (`TfIdf::builder()`, `Tokenizer::builder()`)
- Don't re-export dependency types in your public API. Wrap them so you can
  swap implementations later.
- Iterator-returning APIs where possible (`tokens()` returns `impl Iterator`,
  not `Vec<Token>`)
- `serde` derives on `Vocabulary`, `TfIdf`, `SparseVec` — don't pick a serialization
  format, let users choose

### Testing
- `proptest` property tests on the tokenizer for Unicode edge cases
- Port a chunk of sklearn's `TfidfVectorizer` test cases as ground-truth comparison
- Round-trip tests for serde-derived types
- Unit tests on each public function

### Performance
- `criterion` benchmarks from day one for: tokenization, ngram generation,
  TF-IDF transform, cosine similarity
- Target: tokenize + transform a typical (~500 word) document in < 100µs on
  a modern CPU
- Don't optimize prematurely, but track regressions

### Documentation
- README with the acceptance-criterion example, runnable end-to-end
- Doc comments on every public item
- One end-to-end example in `examples/` showing a real tagging workflow
  (use a small public dataset like 20-newsgroups or NLBSE issues)

## What's explicitly out of v0.1

Resist these. They're real and valuable but belong in v0.2+:

- MinHash / LSH (dedup) — different use case, ship after v0.1 proves itself
- Hashing vectorizer (no-vocab variant) — nice but redundant with v0.1's vocab path
- Linear classifier inference — separate concept (file format, weight loading)
- RAKE / YAKE keyword extraction — composes on top of TF-IDF, ship later
- Stemmer integration — depend on `rust-stemmers`, don't wrap it in v0.1
- Stopword lists for specific languages — users supply their own
- Sentence splitting — not needed for tagging
- Polars expression integration — separate crate, after v0.1 API is stable
- Async APIs — sync is fine, transform is already fast
- Topic modeling, NER, dependency parsing, anything ML — out of scope forever

## Crate structure

Single crate for v0.1 — premature splitting hurts. Suggested module layout:

```
src/
  lib.rs        # re-exports, crate-level docs
  error.rs      # crate-wide Error type
  tokenizer.rs  # Tokenizer, TokenSpan, builder
  vocabulary.rs # Vocabulary, TokenId
  ngrams.rs     # ngram iterator
  sparse.rs     # SparseVec and ops
  tfidf.rs      # TfIdf, builder, fit/transform
  similarity.rs # cosine, dot helpers
  preset.rs     # Tokenizer::code_aware(), ::plain() etc.
```

## Dependencies (keep minimal)

- `unicode-segmentation` — word splitting
- `regex` — special-token detection
- `thiserror` — error derives
- `serde` (with `derive` feature) — opt-in via feature flag
- `proptest` — dev only
- `criterion` — dev only

Avoid: any heavy ML/NLP crate, anything pulling in C++, anything async.

## Suggested build sequence

1. Set up the crate, error type, CI (cargo check + test + clippy + fmt)
2. SparseVec with full test coverage (this is the foundation)
3. Tokenizer with `code_aware()` preset, property tests on Unicode
4. Vocabulary with serde and filter_extremes
5. N-gram iterator
6. TF-IDF (fit, transform, fit_extend) — port sklearn test cases
7. Cosine similarity (trivial once SparseVec is solid)
8. README example end-to-end on a real dataset
9. Benchmarks
10. Polish, document, ship as 0.1.0

## References to study (in priority order)

1. **scikit-learn `feature_extraction.text`** — canonical TfidfVectorizer.
   Read source, port tests. <https://github.com/scikit-learn/scikit-learn>
2. **gensim `corpora.Dictionary`** — Vocabulary API design.
   <https://github.com/piskvorky/gensim>
3. **spaCy tokenizer** — for understanding tokenization edge cases.
   Don't port; understand the rules. <https://github.com/explosion/spaCy>
4. **sprs** — Rust sparse matrices, reference for SparseVec design.
   <https://github.com/sparsemat/sprs>

## Discipline notes for the developer

- **Don't expand scope.** Every "while I'm here, let me also add X" delays v0.1
  and pollutes the API. Write X down in `WANTED.md` and keep moving.
- **Use the library yourself before tagging 0.1.0.** Build one real feature on
  your platform using only this library. Every awkwardness you hit is an API
  bug — fix it before release.
- **Maintain a `WANTED.md`** of things you reached for that weren't there.
  This becomes your prioritized v0.2 backlog.
- **Don't open source on day one.** Use it internally for 6-12 months. The
  libraries that get adopted are the ones shaped by real use, not designed in
  isolation.

## Definition of done for v0.1

- [ ] All six components implemented per spec
- [ ] README example compiles, runs, and produces sensible output on a real dataset
- [ ] Tests pass: unit, property, sklearn-comparison, round-trip
- [ ] Benchmarks exist and the targeted document hits < 100µs
- [ ] No `unwrap()` or `panic!` in the public API path (audit with `clippy::unwrap_used`)
- [ ] Doc comments on every public item; `cargo doc` produces clean output
- [ ] One end-to-end example in `examples/` using a real dataset
- [ ] Used internally for at least one real feature in the host platform