# Supply-chain audit

**Date:** 2026-08-02
**Status:** implemented

`cargo audit` reported 12 advisories that nobody had seen, because the pre-commit hook
prints them and moves on. Result: 12 → 0 un-ignored, with a gate that actually blocks.

## Decision: notify-only hook, deliberate gate

The pre-commit hook stays **notify-only** and never blocks a commit — a commit hook must
not block on the network, and must not block a commit on an advisory the committer didn't
introduce. The gate moves to `./scripts/audit.sh`, run on demand.

**No severity threshold, deliberately.** `cargo audit --format json` showed `cvss: None`
on 5 of the 12 advisories, *including all four rustls-webpki certificate-validation bugs*
— a Critical-only gate would have passed precisely the ones that mattered. cargo-audit
0.22 has no severity flag either (`--deny` accepts only
`warnings|unmaintained|unsound|yanked`).

## What was done

1. **`cargo update`** — cleared 8 of 12, semver-compatible, no code changes:
   `crossbeam-epoch` 0.9.18→0.9.20, `lz4_flex` 0.11.5→0.11.6 (also clears a yanked
   warning), `quinn-proto` 0.11.13→0.11.16, `rustls-webpki` 0.103.9→0.103.13 (four
   advisories at once). `spin`'s yanked warning cleared too.

2. **`maxminddb` 0.24 → 0.27** in `crates/brightflow-api/Cargo.toml` — the only direct
   dependency in the list (RUSTSEC-2025-0132, `Reader::open_mmap` unsoundness).

   The plan allowed falling back to an ignore entry if the 0.25–0.27 API churn was
   non-trivial, since `ingest/geo.rs` uses `open_readfile` and never touches the
   unsound path. The churn turned out to be real but small and *simplifying*, so the
   upgrade landed instead:
   - `lookup()` now returns a deferred `LookupResult` handle; decoding is a second step
     (`.decode::<T>()` → `Ok(None)` for an uncovered IP, distinct from a lookup error).
   - Record fields are no longer `Option`-wrapped (`city.country`, `city.subdivisions`).
   - `Names` became a struct with typed language fields, so `.names.english` replaces
     `.names.as_ref().and_then(|n| n.get("en")).copied()`.

   Behavior is unchanged: every failure path still degrades to empty geo.

3. **`.cargo/audit.toml`** (sibling of the existing `config.toml`, where cargo-audit
   looks) with an `[advisories] ignore` list. **Every entry carries a reason and a
   recheck trigger** — same principle as the docs convention: the justification lives
   next to the thing it justifies, so it goes stale loudly rather than quietly.
   - `RUSTSEC-2023-0071` (rsa, Marvin Attack, no fix exists) — reached only via
     `sqlx-mysql`. Verified: `cargo tree -i sqlx-mysql` *and* `cargo tree -i rsa`, both
     with `--target all`, print "nothing to print" — neither crate is in the resolved
     build graph. A Cargo.lock ghost from sqlx's feature union. Recheck if a MySQL
     feature is enabled.
   - `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195` (quick-xml, fixed in ≥0.41) — pinned at
     0.38 transitively via `polars → polars-error → object_store`. object_store's XML
     parsing is the S3/GCS/Azure list-bucket response path; Brightflow only scans local
     parquet paths. Recheck on the next polars bump.

4. **`scripts/audit.sh`** — fetches the advisory DB (no `--no-fetch`) and exits non-zero
   on any un-ignored advisory. This is the real gate.

5. **`scripts/pre-commit` audit block** — it runs `--no-fetch`, so a clean report may
   just mean a stale local DB. It no longer prints "✓ Security audit"; it prints "✓ No
   advisories in local DB (not fetched — run ./scripts/audit.sh for a real audit)". The
   unparseable case now reads as *unknown* rather than clean, and the vulnerable case
   says explicitly that it is not blocking. Still `|| true`, still non-blocking.

6. **`CLAUDE.md`** — `./scripts/audit.sh` documented in the `## Commands` block, with the
   hook's weaker guarantee spelled out so the two aren't confused.

## Residual

Four `unmaintained` warnings remain (`bincode`, `instant`, `number_prefix`, `paste`), all
transitive. `cargo audit` treats these as allowed warnings and exits 0; they are not
vulnerabilities and there is nothing to upgrade to. Left visible rather than ignored.

## Verification

- `cargo audit` → 0 un-ignored advisories (was 12), exit 0
- `./scripts/audit.sh` → green against a freshly-fetched DB
- `cargo test --workspace` → 576 passing
- `cargo build --release`
- `bash scripts/pre-commit` → green and non-blocking
