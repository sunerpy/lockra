//! Hybrid logical clock stamps (Kulkarni et al., 2014): they follow wall time, never go backwards
//! on a device, and come after every stamp the device has seen, so "the last change wins" holds
//! across devices whose clocks disagree.

use serde::{Deserialize, Serialize};

/// When something changed: wall-clock milliseconds, a counter for changes within the same
/// millisecond, and the number of the device that made the change, which breaks ties. Stamps
/// compare in that order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Hlc {
    /// Wall time, Unix milliseconds.
    pub wall_ms: u64,
    /// Changes within the same millisecond.
    pub counter: u32,
    /// The device that made the change.
    pub device: u64,
}

impl Hlc {
    /// A stamp from wall time alone: records written before stamps existed.
    pub fn at(wall_ms: u64) -> Self {
        Self { wall_ms, counter: 0, device: 0 }
    }
}

/// One device's clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clock {
    device: u64,
    last: Hlc,
}

impl Clock {
    /// The clock of `device`, which has issued and seen nothing yet.
    pub fn new(device: u64) -> Self {
        Self { device, last: Hlc::default() }
    }

    /// The device this clock stamps for.
    pub fn device(&self) -> u64 {
        self.device
    }

    /// The latest stamp issued or seen.
    pub fn last(&self) -> Hlc {
        self.last
    }

    /// A stamp for a change made at `now_ms`: later than every stamp issued or seen.
    pub fn tick(&mut self, now_ms: u64) -> Hlc {
        let (wall_ms, counter) = if now_ms > self.last.wall_ms { (now_ms, 0) } else { (self.last.wall_ms, self.last.counter.saturating_add(1)) };
        self.last = Hlc { wall_ms, counter, device: self.device };
        self.last
    }

    /// Note a stamp from another device: the next tick comes after it.
    pub fn observe(&mut self, stamp: Hlc) {
        if (stamp.wall_ms, stamp.counter) > (self.last.wall_ms, self.last.counter) {
            self.last = Hlc { wall_ms: stamp.wall_ms, counter: stamp.counter, device: self.device };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_follow_wall_time_and_never_go_back() {
        let mut clock = Clock::new(7);
        let a = clock.tick(1_000);
        assert_eq!(a, Hlc { wall_ms: 1_000, counter: 0, device: 7 });
        let b = clock.tick(1_000);
        assert_eq!(b, Hlc { wall_ms: 1_000, counter: 1, device: 7 });
        // The wall clock went back: the stamp still moves forward.
        let c = clock.tick(500);
        assert_eq!(c, Hlc { wall_ms: 1_000, counter: 2, device: 7 });
        let d = clock.tick(2_000);
        assert_eq!(d, Hlc { wall_ms: 2_000, counter: 0, device: 7 });
        assert!(a < b && b < c && c < d);
        assert_eq!(clock.last(), d);
        assert_eq!(clock.device(), 7);
    }

    #[test]
    fn a_stamp_seen_from_a_device_ahead_is_overtaken_by_the_next_tick() {
        let mut behind = Clock::new(1);
        let ahead = Hlc { wall_ms: 9_000, counter: 4, device: 2 };
        behind.observe(ahead);
        let next = behind.tick(1_000);
        assert!(next > ahead, "{next:?} after {ahead:?}");
        assert_eq!(next.device, 1);
        // An older stamp changes nothing.
        behind.observe(Hlc::at(10));
        assert!(behind.tick(1_000) > next);
    }

    #[test]
    fn the_device_breaks_ties_and_old_records_sort_first() {
        let a = Hlc { wall_ms: 5, counter: 0, device: 1 };
        let b = Hlc { wall_ms: 5, counter: 0, device: 2 };
        assert!(a < b);
        assert!(Hlc::at(5) < a);
        assert_eq!(serde_json::to_value(a).unwrap(), serde_json::json!({ "wall_ms": 5, "counter": 0, "device": 1 }));
    }
}
