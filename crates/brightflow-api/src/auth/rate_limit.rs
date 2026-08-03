//! Per-IP token bucket for the login endpoint.
//!
//! Argon2 verification is deliberately expensive, which makes `/api/auth/login`
//! both a credential-stuffing target and a cheap way to burn server CPU. This
//! caps how fast one client can spend that CPU.
//!
//! A token bucket rather than a fixed window because the useful shape is "a few
//! attempts immediately, then slow": someone fumbling their own password should
//! never notice this, and someone iterating a wordlist should hit it within
//! seconds. A fixed window gives both the same treatment and lets an attacker
//! burst at every boundary.
//!
//! State lives in a `DashMap` on `AppState` — `dashmap` is already a dependency
//! and the state already holds nine of them, so this adds no crate. That also
//! means the limit is **per process**, not shared across replicas; with more than
//! one API instance behind a load balancer the effective rate is multiplied by
//! the instance count. Sufficient for the single-process deployment this ships
//! as, and the point where it stops being sufficient is the point where the
//! bucket store should move to the shared database.

use std::time::{Duration, Instant};

use dashmap::DashMap;

/// Attempts a client may make back-to-back before throttling starts.
pub const LOGIN_BURST: f64 = 5.0;
/// Sustained rate once the burst is spent — 1 attempt per 12s (5/min).
pub const LOGIN_REFILL_PER_SEC: f64 = 1.0 / 12.0;
/// Buckets are dropped once idle this long; a full bucket is indistinguishable
/// from a client that never appeared, so nothing is lost by forgetting it.
const IDLE_EVICT: Duration = Duration::from_mins(10);
/// Prune only once the map is big enough to be worth walking.
const PRUNE_THRESHOLD: usize = 1024;

/// A single client's bucket.
#[derive(Debug, Clone, Copy)]
pub struct TokenBucket {
    tokens: f64,
    last_seen: Instant,
}

impl TokenBucket {
    /// A fresh bucket starts full, so a first-time client is never throttled.
    pub fn new(now: Instant) -> Self {
        Self {
            tokens: LOGIN_BURST,
            last_seen: now,
        }
    }

    /// Tokens available after `elapsed_secs` of refill, capped at the burst size.
    ///
    /// Split out as pure arithmetic so the refill curve can be tested without
    /// sleeping: everything time-dependent is the caller's `elapsed_secs`.
    pub fn refilled(tokens: f64, elapsed_secs: f64, capacity: f64, rate_per_sec: f64) -> f64 {
        // A backwards clock (elapsed < 0) must not mint tokens.
        let gained = elapsed_secs.max(0.0) * rate_per_sec;
        (tokens + gained).min(capacity)
    }

    /// Refill for elapsed time, then spend one token if any remain.
    ///
    /// Returns `true` when the attempt is allowed. A rejected attempt does not
    /// consume anything — being throttled must not extend the throttle, or a
    /// client that keeps retrying could never recover.
    pub fn try_consume(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last_seen).as_secs_f64();
        self.tokens = Self::refilled(self.tokens, elapsed, LOGIN_BURST, LOGIN_REFILL_PER_SEC);
        self.last_seen = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Whether this bucket carries no information worth keeping.
    fn is_idle(self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_seen) > IDLE_EVICT
    }
}

/// Shared map of client key → bucket.
pub type LoginLimiter = DashMap<String, TokenBucket>;

/// Record a login attempt from `key`, returning `true` if it is allowed.
///
/// `key` is the client identity — see `client_key` for why that is a header and
/// what it assumes.
pub fn check_login(limiter: &LoginLimiter, key: &str, now: Instant) -> bool {
    // Unbounded growth here would be its own denial of service: one request per
    // forged key would otherwise pin a map entry forever.
    if limiter.len() > PRUNE_THRESHOLD {
        limiter.retain(|_, bucket| !bucket.is_idle(now));
    }

    let mut bucket = limiter
        .entry(key.to_string())
        .or_insert_with(|| TokenBucket::new(now));
    bucket.try_consume(now)
}

/// Derive the rate-limit key for a request.
///
/// **Contract:** `X-Forwarded-For` is attacker-controlled unless a reverse
/// proxy overwrites it, so this keying is only sound behind a proxy that does.
/// Deploying it exposed directly to the internet re-opens the limit: a client
/// could rotate the header to get a fresh bucket per attempt. It still keys on the
/// header rather than the socket address because behind a proxy every request
/// shares one socket address, and keying on that would throttle all users
/// together — a worse failure, and one that hurts legitimate traffic.
pub fn client_key(headers: &axum::http::HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
        })
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refill_accumulates_at_the_configured_rate() {
        // 12s at 1/12 per second = exactly one token.
        let t = TokenBucket::refilled(0.0, 12.0, LOGIN_BURST, LOGIN_REFILL_PER_SEC);
        assert!((t - 1.0).abs() < 1e-9, "expected ~1.0, got {t}");
    }

    #[test]
    fn refill_is_capped_at_capacity() {
        let t = TokenBucket::refilled(4.0, 10_000.0, LOGIN_BURST, LOGIN_REFILL_PER_SEC);
        assert!((t - LOGIN_BURST).abs() < 1e-9);
    }

    #[test]
    fn refill_ignores_a_backwards_clock() {
        // Negative elapsed must not remove tokens either.
        let t = TokenBucket::refilled(2.0, -50.0, LOGIN_BURST, LOGIN_REFILL_PER_SEC);
        assert!((t - 2.0).abs() < 1e-9);
    }

    #[test]
    fn burst_is_allowed_then_throttled() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(now);
        for i in 0..5 {
            assert!(bucket.try_consume(now), "attempt {i} should be allowed");
        }
        assert!(!bucket.try_consume(now), "6th attempt must be throttled");
    }

    #[test]
    fn a_throttled_attempt_does_not_deepen_the_throttle() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(now);
        for _ in 0..5 {
            bucket.try_consume(now);
        }
        // Hammer while throttled.
        for _ in 0..100 {
            assert!(!bucket.try_consume(now));
        }
        // One refill period later exactly one attempt is available again — the
        // hammering neither delayed nor advanced recovery.
        assert!(bucket.try_consume(now + Duration::from_secs(12)));
        assert!(!bucket.try_consume(now + Duration::from_secs(12)));
    }

    #[test]
    fn waiting_restores_the_full_burst() {
        let now = Instant::now();
        let mut bucket = TokenBucket::new(now);
        for _ in 0..5 {
            bucket.try_consume(now);
        }
        let later = now + Duration::from_mins(1);
        for i in 0..5 {
            assert!(bucket.try_consume(later), "attempt {i} after wait");
        }
        assert!(!bucket.try_consume(later));
    }

    #[test]
    fn separate_keys_get_separate_buckets() {
        let limiter = LoginLimiter::new();
        let now = Instant::now();
        for _ in 0..5 {
            assert!(check_login(&limiter, "10.0.0.1", now));
        }
        assert!(!check_login(&limiter, "10.0.0.1", now));
        // A different client is unaffected by the first one's exhaustion.
        assert!(check_login(&limiter, "10.0.0.2", now));
    }

    #[test]
    fn idle_buckets_are_pruned_once_the_map_grows() {
        let limiter = LoginLimiter::new();
        let now = Instant::now();
        for i in 0..(PRUNE_THRESHOLD + 2) {
            check_login(&limiter, &format!("10.0.{}.{}", i / 256, i % 256), now);
        }
        assert!(limiter.len() > PRUNE_THRESHOLD);

        // Long after every bucket went idle, the next call sweeps them.
        let much_later = now + IDLE_EVICT + Duration::from_secs(1);
        check_login(&limiter, "10.9.9.9", much_later);
        assert_eq!(
            limiter.len(),
            1,
            "only the caller that triggered the prune should remain"
        );
    }

    #[test]
    fn active_buckets_survive_a_prune() {
        let limiter = LoginLimiter::new();
        let now = Instant::now();
        for i in 0..(PRUNE_THRESHOLD + 2) {
            check_login(&limiter, &format!("10.0.{}.{}", i / 256, i % 256), now);
        }
        // Still within the idle window: nothing should be dropped.
        let soon = now + Duration::from_secs(5);
        check_login(&limiter, "10.9.9.9", soon);
        assert!(limiter.len() > PRUNE_THRESHOLD);
    }

    fn headers_with(pairs: &[(&str, &str)]) -> axum::http::HeaderMap {
        let mut h = axum::http::HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                axum::http::HeaderName::from_bytes(k.as_bytes()).unwrap(),
                axum::http::HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn client_key_prefers_first_forwarded_for_hop() {
        let h = headers_with(&[("x-forwarded-for", "203.0.113.7, 10.0.0.1")]);
        assert_eq!(client_key(&h), "203.0.113.7");
    }

    #[test]
    fn client_key_falls_back_to_real_ip() {
        let h = headers_with(&[("x-real-ip", "198.51.100.4")]);
        assert_eq!(client_key(&h), "198.51.100.4");
    }

    #[test]
    fn client_key_ignores_an_empty_forwarded_for() {
        // An empty header must not become the key for every such client, and must
        // not shadow x-real-ip.
        let h = headers_with(&[("x-forwarded-for", ""), ("x-real-ip", "198.51.100.4")]);
        assert_eq!(client_key(&h), "198.51.100.4");
    }

    #[test]
    fn client_key_without_headers_is_a_single_shared_bucket() {
        // Direct-to-socket deployments all share "unknown". That is the safe
        // direction: it over-throttles rather than under-throttles.
        assert_eq!(client_key(&headers_with(&[])), "unknown");
    }
}
