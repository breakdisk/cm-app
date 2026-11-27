//! Rate limiting for the unauthenticated table-scan endpoint.
//!
//! ## Why this is not the platform's rate limiter
//!
//! `api-gateway`'s `check_rate_limit` keys on `ratelimit:tenant:{tenant_id}`
//! and sizes its window from the caller's subscription tier. A scan carries
//! neither: it is unauthenticated, and the tenant is an *output* of the token
//! lookup rather than an input to it. The scan endpoint therefore sits outside
//! the platform's rate-limiting model entirely — it is not merely
//! under-configured — and needs its own.
//!
//! ## What this bounds, and what it does not
//!
//! Two keys, because they bound two different abuses:
//!
//! - **By token** — one photographed sticker cannot open sessions forever.
//! - **By client IP** — one scanner cannot sweep many tokens from one place.
//!
//! **LIMITATION, stated rather than discovered: this is per-process.** Counters
//! live in memory, so N replicas allow N times the configured rate, and a
//! restart forgets everything. That is a real weakening and it is the honest
//! trade for not adding Redis to this service. It still turns "unbounded" into
//! "bounded by a number times the replica count", which is the difference that
//! matters. Move it to Redis when the scan endpoint is genuinely public
//! traffic rather than a handful of venues.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

/// A fixed window. Chosen over a sliding log because the memory cost of a
/// sliding window is proportional to traffic, and this thing exists precisely
/// to survive someone generating traffic.
const WINDOW_SECONDS: i64 = 60;

/// Scans allowed per token per window. Generous: a table of friends each
/// scanning, plus retries on bad restaurant wifi, must not trip it.
const PER_TOKEN_PER_WINDOW: u32 = 20;

/// Scans allowed per client IP per window. Higher than the per-token limit
/// because a whole venue can legitimately share one NAT address.
const PER_IP_PER_WINDOW: u32 = 120;

/// How many distinct keys to track before refusing to grow.
///
/// Without this, an attacker sweeping random tokens turns the limiter itself
/// into the memory leak — the classic way a naive limiter becomes the attack.
/// At the cap, unseen keys are simply allowed: failing open on an over-full
/// table is better than failing the whole endpoint, and the per-IP key (far
/// fewer of them) still bounds the sweeper.
const MAX_TRACKED_KEYS: usize = 50_000;

#[derive(Debug, Clone, Copy)]
struct Bucket {
    window_start: DateTime<Utc>,
    count: u32,
}

/// Fixed-window counters for the scan endpoint.
#[derive(Default)]
pub struct ScanLimiter {
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl ScanLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this scan may proceed, counting it if so.
    ///
    /// `now` is a parameter rather than an internal `Utc::now()` so window
    /// boundaries can be tested exactly — the same reason `leg_recovery::decide`
    /// and `Venue::is_open_at` take one.
    ///
    /// Checks the IP budget first and the token budget second, but **counts
    /// both only when both pass**: a request refused on one key must not
    /// consume the other's budget, or a sweeper hitting its IP limit would also
    /// exhaust every token it touched and lock out real diners.
    pub fn check(&self, token: &str, client_ip: Option<&str>, now: DateTime<Utc>) -> bool {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());

        let mut keys: Vec<(String, u32)> = Vec::with_capacity(2);
        if let Some(ip) = client_ip {
            keys.push((format!("ip:{ip}"), PER_IP_PER_WINDOW));
        }
        keys.push((format!("tok:{token}"), PER_TOKEN_PER_WINDOW));

        // Would any key exceed? Decided before anything is incremented.
        for (key, limit) in &keys {
            // Absent, or its window has rolled: nothing to exceed yet.
            let over = matches!(
                buckets.get(key),
                Some(b)
                    if now - b.window_start < Duration::seconds(WINDOW_SECONDS)
                        && b.count >= *limit
            );
            if over {
                return false;
            }
        }

        for (key, _) in keys {
            match buckets.get_mut(&key) {
                Some(b) if now - b.window_start < Duration::seconds(WINDOW_SECONDS) => {
                    b.count += 1;
                }
                Some(b) => *b = Bucket { window_start: now, count: 1 },
                None => {
                    // See MAX_TRACKED_KEYS: refuse to grow rather than let the
                    // limiter become the memory exhaustion it exists to prevent.
                    if buckets.len() < MAX_TRACKED_KEYS {
                        buckets.insert(key, Bucket { window_start: now, count: 1 });
                    }
                }
            }
        }
        true
    }

    /// Drops buckets whose window has long passed.
    ///
    /// Called on a timer rather than on every request: sweeping inline would
    /// make one request's cost proportional to how many keys exist, which is
    /// exactly the property an attacker would drive up.
    pub fn evict_stale(&self, now: DateTime<Utc>) -> usize {
        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        let before = buckets.len();
        buckets.retain(|_, b| now - b.window_start < Duration::seconds(WINDOW_SECONDS * 2));
        before - buckets.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> DateTime<Utc> {
        DateTime::from_timestamp(1_788_000_000, 0).unwrap()
    }

    #[test]
    fn a_token_is_allowed_up_to_its_limit_and_then_refused() {
        let l = ScanLimiter::new();
        for i in 0..PER_TOKEN_PER_WINDOW {
            assert!(l.check("tok-a", None, t0()), "scan {i} should be allowed");
        }
        assert!(!l.check("tok-a", None, t0()), "one past the limit must be refused");
    }

    #[test]
    fn the_window_rolls() {
        let l = ScanLimiter::new();
        for _ in 0..PER_TOKEN_PER_WINDOW {
            assert!(l.check("tok-a", None, t0()));
        }
        assert!(!l.check("tok-a", None, t0()));
        let later = t0() + Duration::seconds(WINDOW_SECONDS);
        assert!(l.check("tok-a", None, later), "a new window starts fresh");
    }

    #[test]
    fn one_tokens_exhaustion_does_not_affect_another() {
        let l = ScanLimiter::new();
        for _ in 0..PER_TOKEN_PER_WINDOW {
            l.check("tok-a", None, t0());
        }
        assert!(!l.check("tok-a", None, t0()));
        assert!(l.check("tok-b", None, t0()), "a different table is unaffected");
    }

    #[test]
    fn a_sweeper_is_bounded_by_ip_even_across_many_tokens() {
        // The abuse this key exists for: one machine trying token after token.
        let l = ScanLimiter::new();
        let mut allowed = 0;
        for i in 0..(PER_IP_PER_WINDOW + 50) {
            if l.check(&format!("tok-{i}"), Some("10.0.0.9"), t0()) {
                allowed += 1;
            }
        }
        assert_eq!(
            allowed, PER_IP_PER_WINDOW,
            "the IP budget must bound a sweep across distinct tokens",
        );
    }

    #[test]
    fn a_refusal_on_one_key_does_not_consume_the_others_budget() {
        // The subtle one. A sweeper that has burned its IP budget must not also
        // burn every token's budget, or it locks out the real diners at those
        // tables for the rest of the window.
        let l = ScanLimiter::new();
        for i in 0..PER_IP_PER_WINDOW {
            l.check(&format!("sweep-{i}"), Some("10.0.0.9"), t0());
        }
        assert!(!l.check("victim-table", Some("10.0.0.9"), t0()), "IP is exhausted");

        // The victim table, scanned from a phone on a different network, is fine.
        assert!(
            l.check("victim-table", Some("192.168.1.5"), t0()),
            "the refused scan must not have consumed the token's own budget",
        );
    }

    #[test]
    fn stale_buckets_are_evicted() {
        let l = ScanLimiter::new();
        l.check("tok-a", Some("10.0.0.1"), t0());
        let much_later = t0() + Duration::seconds(WINDOW_SECONDS * 3);
        assert_eq!(l.evict_stale(much_later), 2, "both the token and ip buckets go");
        // And the table is usable again afterwards.
        assert!(l.check("tok-a", Some("10.0.0.1"), much_later));
    }
}
