//! Request anomaly detection for ESPASS backend.
//!
//! Tracks per-IP request volume over a sliding window. IPs that exceed the
//! burst threshold are flagged; the caller decides whether to log or block.

use std::collections::BTreeMap;
use std::net::IpAddr;
use time::{Duration, OffsetDateTime};

/// Tracks per-IP request burst anomalies.
pub struct AnomalyDetector {
    window_seconds: i64,
    burst_threshold: u32,
    counts: BTreeMap<IpAddr, WindowCount>,
}

struct WindowCount {
    count: u32,
    window_start: OffsetDateTime,
}

impl AnomalyDetector {
    /// Creates a detector with the given window and burst threshold.
    #[must_use]
    pub fn new(window_seconds: i64, burst_threshold: u32) -> Self {
        Self {
            window_seconds,
            burst_threshold,
            counts: BTreeMap::new(),
        }
    }

    /// Records a request and returns true if the IP is anomalous (burst detected).
    pub fn record(&mut self, ip: IpAddr, now: OffsetDateTime) -> bool {
        let window_seconds = self.window_seconds;
        let burst_threshold = self.burst_threshold;

        let entry = self.counts.entry(ip).or_insert(WindowCount {
            count: 0,
            window_start: now,
        });

        if now - entry.window_start >= Duration::seconds(window_seconds) {
            entry.count = 0;
            entry.window_start = now;
        }

        entry.count = entry.count.saturating_add(1);
        entry.count > burst_threshold
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
    fn normal_traffic_is_not_anomalous() {
        let mut detector = AnomalyDetector::new(60, 5);
        let now = OffsetDateTime::now_utc();
        for _ in 0..5 {
            assert!(!detector.record(ip(1), now));
        }
    }

    #[test]
    fn burst_traffic_is_flagged() {
        let mut detector = AnomalyDetector::new(60, 5);
        let now = OffsetDateTime::now_utc();
        for _ in 0..5 {
            detector.record(ip(1), now);
        }
        assert!(detector.record(ip(1), now));
    }

    #[test]
    fn window_resets_after_expiry() {
        let mut detector = AnomalyDetector::new(60, 3);
        let now = OffsetDateTime::now_utc();
        for _ in 0..3 {
            detector.record(ip(1), now);
        }
        // After window expires, count resets.
        let later = now + Duration::seconds(61);
        assert!(!detector.record(ip(1), later));
    }
}
