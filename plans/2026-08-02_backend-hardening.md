# Backend hardening

**Date:** 2026-08-02
**Status:** implemented

Five independent items, one review.

## 1. Argon2 off the async runtime

`auth/backend.rs` called `verify_password` synchronously inside
`AuthnBackend::authenticate`, pinning a tokio worker thread for the ~50–100ms Argon2
deliberately costs — starving every other request that worker was multiplexing. Now wrapped
in `tokio::task::spawn_blocking`.

## 2. User-enumeration timing oracle closed

`authenticate` returned early when the email was unknown, without hashing, so a
wrong-email response was measurably faster than a wrong-password one — a remote oracle for
"does this account exist". The unknown-email path now verifies against a fixed dummy hash,
so both branches do exactly one Argon2 verification.

The dummy is generated once via `LazyLock` from the same `hash_password` used for real
passwords, rather than being a hardcoded literal: a literal would silently stop matching if
the Argon2 cost parameters were ever tuned, quietly reopening the gap. It uses `expect`
rather than a fallback literal, because a fallback that turned out to be unparseable would
make the unknown-email path return 500 — a *louder* oracle than the one being closed.

## 3. Login rate limit

Per-IP token bucket in a `DashMap` on `AppState` (`auth/rate_limit.rs`). `dashmap` was
already a dependency and the state already held nine of them, so no new crate. Applied to
`/api/auth/login` only, and checked *before* authenticating — the point is to cap how often
a client can make the server spend Argon2 CPU.

- 5 attempts burst, then 1 per 12s (5/min). A token bucket rather than a fixed window
  because the useful shape is "a few attempts immediately, then slow": someone fumbling
  their own password never notices, someone running a wordlist hits it in seconds. A fixed
  window treats both alike and lets an attacker burst at every boundary.
- A *rejected* attempt consumes nothing, so hammering while throttled can't extend the
  throttle — otherwise a retrying client could never recover.
- Buckets idle >10min are pruned once the map exceeds 1024 entries; unbounded growth would
  be its own denial of service.
- New `AppError::TooManyRequests` → HTTP 429.

Two limits are documented in the module rather than hidden:

- **Per process, not per cluster.** With more than one API instance behind a load
  balancer the effective rate multiplies by instance count. Fine for the single-process
  deployment this ships as; the point where it stops being fine is the point where the
  buckets belong in the shared database.
- **Keys on `X-Forwarded-For`, which is attacker-controlled** unless a reverse proxy
  overwrites it — sound for this deployment (see the `APP_ENV=production` branch in
  `lib.rs`), unsound if the API is ever exposed directly, where a client could rotate the
  header for a fresh bucket per attempt. It still keys on the header rather than the socket
  address because behind a proxy every request shares one socket address, and keying on
  that would throttle all users together — a worse failure that hurts legitimate traffic.

The refill curve is pure arithmetic (`TokenBucket::refilled` takes `elapsed_secs`) so it is
unit-tested without sleeping: 15 co-located tests cover the refill rate, the capacity cap,
a backwards clock, burst-then-throttle, non-deepening rejection, recovery, key isolation,
pruning, and the header-parsing cases.

## 4. CORS production fallback is now a startup error

`lib.rs` fell through to `CorsLayer::permissive()` when `APP_ENV=production` and
`cors_origin` was unset, on the reasoning that production sits behind a reverse proxy on
the same origin. That assumption was invisible at runtime: with the proxy absent or
misconfigured, the server came up happily with a wildcard policy and nothing said so. That
combination is now a hard `anyhow::bail!` at startup.

A permissive layer cannot carry credentials, so cookie auth would have broken rather than
leaked — the old failure mode was confusing, not catastrophic. Failing at startup still
beats failing mysteriously on the first cross-origin request.

## 5. `is_admin` removed

Both creation sites hardcoded `true` (the bootstrap seeder in `lib.rs`, and
`cli/main.rs`), there is no signup endpoint that could produce a non-admin, and no handler
or frontend component ever read it — it survived only in the ts-rs-generated `User.ts`.

It could not gate anything, and enforcing it would mean inventing a role system that does
not exist. A privilege flag that grants no privilege is worse than none, because it reads
as protection.

- `migrations/005_drop_is_admin.sql` — `ALTER TABLE users DROP COLUMN is_admin`
  (needs SQLite ≥3.35; local is 3.45).
- Field dropped from `User` (`auth/models.rs`) and `UserResponse` (`auth/handlers.rs`).
- Parameter dropped from `create_user` (`auth/db.rs`); both callers updated.
- `src/types/generated/User.ts` regenerated — `isAdmin` gone.

## Verification

- `cargo test -p brightflow-api` — 19 auth tests pass (15 new in `rate_limit`, 2 new in
  `backend`)
- `cargo build --release`
- End-to-end against a running server: login works; repeated bad-password attempts from one
  IP start returning 429; the server refuses to start with `APP_ENV=production` and no
  `cors_origin`
