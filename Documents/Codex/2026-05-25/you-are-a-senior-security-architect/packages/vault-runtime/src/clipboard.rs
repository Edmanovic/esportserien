//! Clipboard exposure TTL guard.
//!
//! When ESPASS copies a secret to the clipboard it must clear it after a short
//! TTL. Create a `ClipboardGuard` immediately after writing to the clipboard,
//! then poll `is_expired()` in the UI loop to trigger clearing.

use std::time::{Duration, Instant};

/// Tracks elapsed time since a secret was placed on the clipboard.
pub struct ClipboardGuard {
    set_at: Instant,
    ttl: Duration,
}

impl ClipboardGuard {
    /// Creates a guard with `ttl_seconds` TTL, starting now.
    #[must_use]
    pub fn new(ttl_seconds: u64) -> Self {
        Self {
            set_at: Instant::now(),
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Creates a guard with a custom elapsed offset, used only in tests.
    #[cfg(test)]
    pub(crate) fn with_elapsed(elapsed: Duration, ttl_seconds: u64) -> Self {
        Self {
            set_at: Instant::now() - elapsed,
            ttl: Duration::from_secs(ttl_seconds),
        }
    }

    /// Returns true when the TTL has elapsed and the clipboard should be cleared.
    #[must_use]
    pub fn is_expired(&self) -> bool {
        self.set_at.elapsed() >= self.ttl
    }

    /// Returns whole seconds remaining before expiry, clamped to 0.
    #[must_use]
    pub fn seconds_remaining(&self) -> u64 {
        let elapsed = self.set_at.elapsed();
        if elapsed >= self.ttl {
            0
        } else {
            (self.ttl - elapsed).as_secs()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn not_expired_immediately() {
        let guard = ClipboardGuard::new(30);
        assert!(!guard.is_expired());
        assert!(guard.seconds_remaining() > 0);
    }

    #[test]
    fn expired_after_ttl_elapses() {
        let guard = ClipboardGuard::with_elapsed(Duration::from_secs(31), 30);
        assert!(guard.is_expired());
        assert_eq!(guard.seconds_remaining(), 0);
    }

    #[test]
    fn seconds_remaining_decreases_over_time() {
        let guard = ClipboardGuard::with_elapsed(Duration::from_secs(10), 30);
        assert!(guard.seconds_remaining() <= 20);
    }
}
