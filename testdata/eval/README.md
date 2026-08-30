# Per-language eval sets for `brightflow enrich eval`

**Dated:** 2026-08-30. Starter sets, author-supplied and hand-labelled; extend
them before trusting the numbers. One CSV per language, same header:

```
id,title,body,expected_category,expected_polarity,expected_mentions
```

`expected_category` names a root category that must exist in the target
table's vocabulary (or `other`). `expected_mentions` is `type:subject` pairs
separated by `|` (subject empty for feedback/service), e.g.
`product:Invoice screen|feedback:`. The eval reports per-field accuracy for
Call A and precision/recall on `(type, subject)` pairs for Call B.
