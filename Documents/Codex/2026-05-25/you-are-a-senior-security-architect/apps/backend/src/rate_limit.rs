//! Token-bucket rate limiter for global backend request throttling.
//!
//! Complements the per-IP fixed-window limiter in `security.rs`.
//! The bucket refills at a constant rate; requests consume one token each.

use std::net::IpAddr;
use std::collections::BTreeMap;
use time::OffsetDateTime;

/// Per-IP token-bucket rate limiter.
///
/// Each IP gets its own bucket. Tokens refill at `refill_rate` tokens per
/// second up to `capacity`. Each request consumes one token.
pub struct TokenBucket {
    capacity: f64,
    refill_rate: f64,
    buckets: BTreeMap<IpAddr, BucketState>,
}

struct BucketState {
    tokens: f64,
    last_refill: OffsetDateTime,
}

impl TokenBucket {
    /// Creates a token bucket with the given capacity and refill rate.
    #[must_use]
    pub fn new(capacity: u32, refill_rate_per_second: f64) -> Self {
        Self {
            capacity: f64::from(capacity),
            refill_rate: refill_rate_per_second,
            buckets: BTreeMap::new(),
        }
    }

    /// Attempts to consume one token for the given IP.
    ///
    /// Returns `Ok(())` if a token was available, or `Err(())` if the bucket
    /// is empty (request should be rejected with 429).
    pub fn consume(&mut self, ip: IpAddr, now: OffsetDateTime) -> Result<(), ()> {
        let capacity = self.capacity;
        let refill_rate = self.refill_rate;

        let state = self.buckets.entry(ip).or_insert(BucketState {
            tokens: capacity,
            last_refill: now,
        });

        // Refill tokens based on elapsed time.
        let elapsed = (now - state.last_refill).as_seconds_f64().max(0.0);
        state.tokens = (state.tokens + elapsed * refill_rate).min(capacity);
        state.last_refill = now;

        if state.tokens >= 1.0 {
            state.tokens -= 1.0;
            Ok(())
        } else {
            Err(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};
    use time::{Duration, OffsetDateTime};

    fn ip(a: u8) -> IpAddr {
        IpAddr::V4(Ipv4Addr::new(a, 0, 0, 1))
    }

    #[test]
    fn allows_requests_within_capacity() {
        let mut bucket = TokenBucket::new(5, 1.0);
        let now = OffsetDateTime::now_utc();
        for _ in 0..5 {
            assert!(bucket.consume(ip(1), now).is_ok());
        }
    }

    #[test]
    fn rejects_when_empty() {
        let mut bucket = TokenBucket::new(2, 0.0); // no refill
        let now = OffsetDateTime::now_utc();
        assert!(bucket.consume(ip(1), now).is_ok());
        assert!(bucket.consume(ip(1), now).is_ok());
        assert!(bucket.consume(ip(1), now).is_err());
    }

    #[test]
    fn refills_over_time() {
        let mut bucket = TokenBucket::new(2, 1.0);
        let now = OffsetDateTime::now_utc();
        // Drain the bucket.
        assert!(bucket.consume(ip(1), now).is_ok());
        assert!(bucket.consume(ip(1), now).is_ok());
        assert!(bucket.consume(ip(1), now).is_err());
        // 3 seconds later, should have 3 tokens but capped at 2.
        let later = now + Duration::seconds(3);
        assert!(bucket.consume(ip(1), later).is_ok());
        assert!(bucket.consume(ip(1), later).is_ok());
        assert!(bucket.consume(ip(1), later).is_err());
    }
}
