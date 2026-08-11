//! A token bucket on `/mcp`.
//!
//! The MCP specification lists rate limiting under what a server should do, and
//! there was none. The threat is not abuse — this daemon binds to localhost and
//! serves one person — it is **an agent in a loop**: a model that retries a
//! failing call as fast as the transport allows will hold the global write lock
//! and make the store unusable for the human sitting in front of it.
//!
//! # Why this is hand-written
//!
//! `governor` is the obvious dependency and it is not worth it here. This is
//! one process serving one user over loopback, so there is no distributed
//! coordination, no per-tenant accounting and no clock skew to handle — the
//! whole problem is "how many calls in the last second", which is a counter and
//! an instant. Adding a crate for that buys machinery the scale does not
//! justify, and the standing rule is to leave it out until a measurement asks
//! for it.
//!
//! # Why one bucket and not one per client
//!
//! Per-IP buckets on a loopback socket all key to `127.0.0.1`, so they would be
//! one bucket wearing a disguise. The thing being protected is the store's
//! single write lock, which is global, so the limit that protects it is global
//! too.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Calls allowed in a burst before the bucket is empty.
///
/// Set far above anything legitimate, and the first number was not. Sixty
/// looked generous until a test creating sixty questions in a loop tripped it —
/// which is not a test artefact, it is what planning a phase looks like: this
/// project's own Phase 8 went in as roughly fifty creates and links back to
/// back. A limiter that interrupts real work is a worse bug than the one it
/// prevents.
///
/// So this is a backstop against a *pathological* loop, not a fairness
/// mechanism. A model retrying a failing call does thousands per second; real
/// bulk work does hundreds at most, and slowly.
const BURST: f64 = 300.0;

/// Calls per second sustained, once the burst is spent.
///
/// At roughly 1–10ms per write, fifty a second leaves the write lock mostly
/// free — enough that the human in front of the app still gets served while a
/// runaway session is being throttled, which is the whole point.
const PER_SECOND: f64 = 50.0;

/// A token bucket.
///
/// Tokens rather than a fixed window, because a fixed window lets a caller
/// spend the whole allowance in the last millisecond of one window and the
/// whole of the next in the first — twice the intended rate, at the moment the
/// limiter is supposed to be working hardest.
#[derive(Debug)]
pub struct RateLimit {
    inner: Mutex<Bucket>,
    burst: f64,
    per_second: f64,
}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Default for RateLimit {
    fn default() -> Self {
        Self::new(BURST, PER_SECOND)
    }
}

impl RateLimit {
    /// A bucket with an explicit size and refill rate, so tests can use a small
    /// one instead of making sixty calls to prove a limit exists.
    pub fn new(burst: f64, per_second: f64) -> Self {
        RateLimit {
            inner: Mutex::new(Bucket {
                tokens: burst,
                last: Instant::now(),
            }),
            burst,
            per_second,
        }
    }

    /// Take a token. `Err` carries how long to wait before trying again.
    ///
    /// A poisoned lock allows the call through rather than refusing it. This is
    /// a guard rail, not an authorisation check: failing open costs one
    /// unmetered request, and failing closed would turn a panic somewhere else
    /// into a daemon that answers nothing.
    pub fn check(&self) -> Result<(), Duration> {
        let Ok(mut bucket) = self.inner.lock() else {
            return Ok(());
        };

        let now = Instant::now();
        let elapsed = now.duration_since(bucket.last).as_secs_f64();
        bucket.last = now;
        bucket.tokens = (bucket.tokens + elapsed * self.per_second).min(self.burst);

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            return Ok(());
        }

        // How long until one whole token exists. Rounded up to the next second
        // because `Retry-After` is expressed in whole seconds, and rounding
        // down would invite an immediate retry that fails again.
        let deficit = 1.0 - bucket.tokens;
        Err(Duration::from_secs_f64(deficit / self.per_second).max(Duration::from_secs(1)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn lets_a_burst_through() {
        let limit = RateLimit::new(3.0, 1.0);
        assert!(limit.check().is_ok());
        assert!(limit.check().is_ok());
        assert!(limit.check().is_ok());
    }

    // Failure case: the loop this exists to stop.
    #[test]
    fn refuses_once_the_burst_is_spent() {
        let limit = RateLimit::new(2.0, 1.0);
        assert!(limit.check().is_ok());
        assert!(limit.check().is_ok());
        let wait = limit.check().expect_err("the third call must be refused");
        assert!(wait >= Duration::from_secs(1), "{wait:?}");
    }

    #[test]
    fn refills_over_time_rather_than_locking_out() {
        // A limiter that never recovers is a broken daemon, not a safe one.
        let limit = RateLimit::new(1.0, 1000.0);
        assert!(limit.check().is_ok());
        std::thread::sleep(Duration::from_millis(20));
        assert!(
            limit.check().is_ok(),
            "twenty milliseconds at 1000/s is twenty tokens"
        );
    }

    #[test]
    fn never_banks_more_than_the_burst() {
        // Otherwise an idle session accumulates an unbounded allowance and the
        // limit stops meaning anything on the one call that matters.
        let limit = RateLimit::new(2.0, 1000.0);
        std::thread::sleep(Duration::from_millis(50));
        assert!(limit.check().is_ok());
        assert!(limit.check().is_ok());
        assert!(limit.check().is_err(), "the cap is the burst, not the wait");
    }
}
