//! Rates: a token bucket per key (a client address, a space), in memory only. Nothing of it is
//! written anywhere, and a bucket that has refilled is forgotten.

use std::collections::HashMap;
use std::hash::Hash;
use std::net::{IpAddr, Ipv6Addr};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::config::canonical;

/// So many events per `period` for each key, at most `burst` of them at once.
pub(crate) struct Rate<K> {
    burst: f64,
    per_second: f64,
    buckets: Mutex<HashMap<K, Bucket>>,
}

#[derive(Clone, Copy)]
struct Bucket {
    tokens: f64,
    at: Instant,
}

impl<K: Eq + Hash + Clone> Rate<K> {
    /// `events` per `period`, a burst of as many (at least one).
    pub(crate) fn new(events: u32, period: Duration) -> Self {
        let events = f64::from(events.max(1));
        Self { burst: events, per_second: events / period.as_secs_f64().max(f64::MIN_POSITIVE), buckets: Mutex::new(HashMap::new()) }
    }

    /// Whether `key` may have one more now; counts it when so.
    pub(crate) fn allow(&self, key: &K, now: Instant) -> bool {
        let mut buckets = self.buckets.lock();
        let bucket = buckets.entry(key.clone()).or_insert(Bucket { tokens: self.burst, at: now });
        let refilled = (bucket.tokens + now.saturating_duration_since(bucket.at).as_secs_f64() * self.per_second).min(self.burst);
        bucket.at = now;
        if refilled >= 1.0 {
            bucket.tokens = refilled - 1.0;
            true
        } else {
            bucket.tokens = refilled;
            false
        }
    }

    /// Forget the keys whose buckets are full again: they would start full anyway.
    pub(crate) fn sweep(&self, now: Instant) {
        self.buckets.lock().retain(|_, bucket| bucket.tokens + now.saturating_duration_since(bucket.at).as_secs_f64() * self.per_second < self.burst);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.buckets.lock().len()
    }
}

/// The client address the per-client rates count by: an IPv4 address, or the /64 network of an
/// IPv6 one (a host usually has the whole /64, and could otherwise ask from a new address each
/// time).
pub(crate) fn client_key(ip: IpAddr) -> IpAddr {
    match canonical(ip) {
        IpAddr::V6(v6) => IpAddr::V6(Ipv6Addr::from(u128::from(v6) & !((1u128 << 64) - 1))),
        v4 => v4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_passes_then_the_rate_holds() {
        let rate = Rate::new(3, Duration::from_secs(60));
        let start = Instant::now();
        let key = "a".to_owned();
        assert!((0..3).all(|_| rate.allow(&key, start)));
        assert!(!rate.allow(&key, start));
        // One more every 20 seconds.
        assert!(!rate.allow(&key, start + Duration::from_secs(19)));
        assert!(rate.allow(&key, start + Duration::from_secs(21)));
        assert!(!rate.allow(&key, start + Duration::from_secs(22)));
        // Another key has its own bucket.
        assert!(rate.allow(&"b".to_owned(), start));
    }

    #[test]
    fn full_buckets_are_forgotten() {
        let rate = Rate::new(2, Duration::from_secs(60));
        let start = Instant::now();
        rate.allow(&1, start);
        rate.allow(&2, start);
        rate.allow(&2, start);
        assert_eq!(rate.len(), 2);
        rate.sweep(start + Duration::from_secs(40));
        // 1 is full again after 30 s; 2 needs 60 s.
        assert_eq!(rate.len(), 1);
        rate.sweep(start + Duration::from_secs(61));
        assert_eq!(rate.len(), 0);
    }

    #[test]
    fn a_zero_rate_still_allows_one() {
        let rate = Rate::new(0, Duration::from_secs(3600));
        let now = Instant::now();
        assert!(rate.allow(&(), now));
        assert!(!rate.allow(&(), now));
    }

    #[test]
    fn ipv6_counts_by_its_64_network_and_mapped_ipv4_as_ipv4() {
        let a: IpAddr = "2001:db8:1:2:aaaa::1".parse().unwrap();
        let b: IpAddr = "2001:db8:1:2:bbbb::9".parse().unwrap();
        let other: IpAddr = "2001:db8:1:3::1".parse().unwrap();
        assert_eq!(client_key(a), client_key(b));
        assert_ne!(client_key(a), client_key(other));
        assert_eq!(client_key("::ffff:203.0.113.7".parse().unwrap()), "203.0.113.7".parse::<IpAddr>().unwrap());
        assert_eq!(client_key("203.0.113.7".parse().unwrap()), "203.0.113.7".parse::<IpAddr>().unwrap());
    }
}
