//! Last-writer-wins merge of records keyed by id, with tombstones for deletions: a state-based
//! CRDT (an LWW element set), plus one field merged by its maximum, a counter that must never go
//! back (an HOTP counter: a code once shown must not come again). Merging is commutative,
//! associative and idempotent, so devices that have seen the same snapshots hold the same records,
//! whatever order they saw them in.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Hlc;

/// A record a replica holds, such as an account. Every change to what syncs (a rename, a moved
/// group, the next HOTP counter) takes a new stamp, and the whole later version wins, except its
/// counter: that is the highest any version of the record reached, a deleted one included (the
/// tombstone keeps it), so a counter never goes back on any device.
pub trait Record: Clone + PartialEq {
    /// Its id, the same on every device.
    fn id(&self) -> Uuid;
    /// When it last changed.
    fn stamp(&self) -> Hlc;
    /// The counter that never goes back, for a record that has one.
    fn counter(&self) -> Option<u64> {
        None
    }
    /// Raise the counter to `floor` (a record with a counter, whose counter is below it).
    fn raise_counter(&mut self, _floor: u64) {}
}

/// A deleted record: its id, when it was deleted, and the highest counter its versions reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    /// The deleted record's id.
    pub id: Uuid,
    /// When it was deleted.
    pub stamp: Hlc,
    /// The highest counter of the deleted versions, for a record that has one: a version brought
    /// back after the deletion continues from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counter: Option<u64>,
}

fn higher(a: Option<u64>, b: Option<u64>) -> Option<u64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

/// Raise `record`'s counter to `floor`; `true` when it went up.
fn raise<R: Record>(record: &mut R, floor: Option<u64>) -> bool {
    match (record.counter(), floor) {
        (Some(current), Some(floor)) if current < floor => {
            record.raise_counter(floor);
            true
        }
        _ => false,
    }
}

/// Fold another replica's records and tombstones into `records` and `tombstones`; `true` when
/// they changed. The later of a record's versions wins; a deletion at or after a record's last
/// change removes it, and a change after a deletion brings it back. Counters take the highest
/// value of every version, deleted ones included.
pub fn merge<R: Record>(records: &mut Vec<R>, tombstones: &mut Vec<Tombstone>, their_records: &[R], their_tombstones: &[Tombstone]) -> bool {
    let mut changed = false;
    for theirs in their_tombstones {
        match tombstones.iter_mut().find(|own| own.id == theirs.id) {
            Some(own) => {
                if theirs.stamp > own.stamp {
                    own.stamp = theirs.stamp;
                    changed = true;
                }
                let counter = higher(own.counter, theirs.counter);
                if counter != own.counter {
                    own.counter = counter;
                    changed = true;
                }
            }
            None => {
                tombstones.push(*theirs);
                changed = true;
            }
        }
    }
    for theirs in their_records {
        match records.iter_mut().find(|own| own.id() == theirs.id()) {
            Some(own) => {
                let floor = higher(own.counter(), theirs.counter());
                if theirs.stamp() > own.stamp() {
                    *own = theirs.clone();
                    changed = true;
                }
                changed |= raise(own, floor);
            }
            None => match tombstones.iter_mut().find(|t| t.id == theirs.id() && t.stamp >= theirs.stamp()) {
                // Deleted here already: only its counter is kept.
                Some(tombstone) => {
                    let counter = higher(tombstone.counter, theirs.counter());
                    if counter != tombstone.counter {
                        tombstone.counter = counter;
                        changed = true;
                    }
                }
                None => {
                    records.push(theirs.clone());
                    changed = true;
                }
            },
        }
    }
    // A record and its tombstone share the highest counter of every version, whichever reached
    // which first; then a deletion removes the versions before it, and a version after it
    // continues from that counter.
    let before = records.len();
    let mut kept = Vec::with_capacity(records.len());
    for mut record in records.drain(..) {
        match tombstones.iter_mut().find(|t| t.id == record.id()) {
            Some(tombstone) => {
                let counter = higher(tombstone.counter, record.counter());
                if counter != tombstone.counter {
                    tombstone.counter = counter;
                    changed = true;
                }
                if tombstone.stamp < record.stamp() {
                    changed |= raise(&mut record, counter);
                    kept.push(record);
                }
            }
            None => kept.push(record),
        }
    }
    *records = kept;
    changed | (records.len() != before)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A record with a value that wins with its stamp, and a counter that never goes back.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Item {
        id: Uuid,
        stamp: Hlc,
        value: u8,
        counter: Option<u64>,
    }

    impl Record for Item {
        fn id(&self) -> Uuid {
            self.id
        }

        fn stamp(&self) -> Hlc {
            self.stamp
        }

        fn counter(&self) -> Option<u64> {
            self.counter
        }

        fn raise_counter(&mut self, floor: u64) {
            self.counter = Some(self.counter.unwrap_or(0).max(floor));
        }
    }

    #[derive(Debug, Clone, Default, PartialEq)]
    struct Replica {
        items: Vec<Item>,
        tombstones: Vec<Tombstone>,
    }

    impl Replica {
        fn absorb(&mut self, other: &Self) -> bool {
            merge(&mut self.items, &mut self.tombstones, &other.items, &other.tombstones)
        }

        /// The state, independent of the order records arrived in.
        fn normal(&self) -> (Vec<Item>, Vec<Tombstone>) {
            let mut items = self.items.clone();
            items.sort_by_key(|i| i.id);
            let mut tombstones = self.tombstones.clone();
            tombstones.sort_by_key(|t| t.id);
            (items, tombstones)
        }
    }

    fn id(n: u8) -> Uuid {
        Uuid::from_bytes([n; 16])
    }

    fn at(wall_ms: u64, device: u64) -> Hlc {
        Hlc { wall_ms, counter: 0, device }
    }

    fn item(n: u8, stamp: Hlc, value: u8) -> Item {
        Item { id: id(n), stamp, value, counter: None }
    }

    fn tombstone(n: u8, stamp: Hlc) -> Tombstone {
        Tombstone { id: id(n), stamp, counter: None }
    }

    #[test]
    fn the_later_version_wins_whole() {
        let mut ours = Replica { items: vec![item(1, at(10, 1), 1)], tombstones: vec![] };
        let theirs = Replica { items: vec![item(1, at(20, 2), 2)], tombstones: vec![] };
        assert!(ours.absorb(&theirs));
        assert_eq!(ours.items, [item(1, at(20, 2), 2)]);
        assert!(!ours.absorb(&theirs), "the same snapshot again changes nothing");
        let older = Replica { items: vec![item(1, at(5, 3), 9)], tombstones: vec![] };
        assert!(!ours.absorb(&older), "an older version loses");
        // A record the other replica has never seen arrives.
        assert!(ours.absorb(&Replica { items: vec![item(2, at(1, 3), 4)], tombstones: vec![] }));
        assert_eq!(ours.items.len(), 2);
    }

    #[test]
    fn a_later_deletion_removes_and_a_later_change_brings_back() {
        let first = item(1, at(10, 1), 1);
        let mut ours = Replica { items: vec![first.clone()], tombstones: vec![] };
        let deletion = Replica { items: vec![], tombstones: vec![tombstone(1, at(20, 2))] };
        assert!(ours.absorb(&deletion));
        assert!(ours.items.is_empty());
        // The old version arriving from a third device stays deleted.
        assert!(!ours.absorb(&Replica { items: vec![first.clone()], tombstones: vec![] }));
        assert!(ours.items.is_empty());
        // An edit after the deletion wins.
        let revived = Item { stamp: at(30, 3), value: 7, ..first };
        assert!(ours.absorb(&Replica { items: vec![revived.clone()], tombstones: vec![] }));
        assert_eq!(ours.items, [revived]);
        // An older tombstone changes nothing; a newer one for an unknown record is kept.
        assert!(!ours.absorb(&Replica { items: vec![], tombstones: vec![tombstone(1, at(15, 4))] }));
        assert!(ours.absorb(&Replica { items: vec![], tombstones: vec![tombstone(9, at(1, 4))] }));
        assert_eq!(ours.tombstones.len(), 2);
    }

    #[test]
    fn a_counter_never_goes_back_whichever_version_wins() {
        // This device went to counter 7; another, offline, went from 5 to 6 later.
        let mut ours = Replica { items: vec![Item { counter: Some(7), ..item(1, at(10, 1), 1) }], tombstones: vec![] };
        let theirs = Replica { items: vec![Item { counter: Some(6), ..item(1, at(20, 2), 2) }], tombstones: vec![] };
        assert!(ours.absorb(&theirs));
        assert_eq!(ours.items, [Item { counter: Some(7), ..item(1, at(20, 2), 2) }], "their names, our counter");
        // The other way round, the same result.
        let mut other = theirs.clone();
        assert!(other.absorb(&Replica { items: vec![Item { counter: Some(7), ..item(1, at(10, 1), 1) }], tombstones: vec![] }));
        assert_eq!(other.normal(), ours.normal());
        assert!(!ours.absorb(&other));
    }

    #[test]
    fn a_deletion_keeps_the_counter_for_a_version_brought_back() {
        let mut ours = Replica { items: vec![Item { counter: Some(9), ..item(1, at(10, 1), 1) }], tombstones: vec![] };
        assert!(ours.absorb(&Replica { items: vec![], tombstones: vec![tombstone(1, at(20, 2))] }));
        assert_eq!(ours.tombstones, [Tombstone { counter: Some(9), ..tombstone(1, at(20, 2)) }]);
        // A device that never saw counter 9 edits the record after the deletion.
        let revived = Item { counter: Some(3), ..item(1, at(30, 3), 5) };
        assert!(ours.absorb(&Replica { items: vec![revived], tombstones: vec![] }));
        assert_eq!(ours.items[0].counter, Some(9));
        // A deleted version arriving later still raises the tombstone's counter.
        let mut deleted = Replica { items: vec![], tombstones: vec![tombstone(2, at(50, 1))] };
        assert!(deleted.absorb(&Replica { items: vec![Item { counter: Some(4), ..item(2, at(40, 2), 1) }], tombstones: vec![] }));
        assert_eq!(deleted.tombstones[0].counter, Some(4));
        assert!(deleted.items.is_empty());
    }

    #[test]
    fn a_tombstone_without_a_counter_reads_and_writes_as_before() {
        let plain: Tombstone = serde_json::from_str(r#"{"id":"01010101-0101-0101-0101-010101010101","stamp":{"wall_ms":1,"counter":0,"device":2}}"#).unwrap();
        assert_eq!(plain, tombstone(1, at(1, 2)));
        assert!(!serde_json::to_string(&plain).unwrap().contains("\"counter\":null"));
    }

    /// One device's edit: create or change record `n`, advance its counter, or delete it.
    #[derive(Debug, Clone)]
    enum Op {
        Put { n: u8, value: u8 },
        Next { n: u8 },
        Delete { n: u8 },
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u8..6, any::<u8>()).prop_map(|(n, value)| Op::Put { n, value }),
            (0u8..6).prop_map(|n| Op::Next { n }),
            (0u8..6).prop_map(|n| Op::Delete { n }),
        ]
    }

    /// Apply `op` on `replica` at stamp `stamp`; the highest counter it produced.
    fn apply(replica: &mut Replica, op: &Op, stamp: Hlc) -> Option<(u8, u64)> {
        match *op {
            Op::Put { n, value } => {
                let counter = replica.items.iter().find(|i| i.id == id(n)).and_then(|i| i.counter).or(Some(0));
                replica.items.retain(|i| i.id != id(n));
                replica.items.push(Item { id: id(n), stamp, value, counter });
                None
            }
            Op::Next { n } => {
                let item = replica.items.iter_mut().find(|i| i.id == id(n))?;
                let next = item.counter.unwrap_or(0) + 1;
                item.counter = Some(next);
                item.stamp = stamp;
                Some((n, next))
            }
            // As the app deletes: only a record the device holds, its counter kept in the tombstone.
            Op::Delete { n } => {
                let counter = replica.items.iter().find(|i| i.id == id(n))?.counter;
                replica.items.retain(|i| i.id != id(n));
                match replica.tombstones.iter_mut().find(|t| t.id == id(n)) {
                    Some(t) => {
                        t.stamp = t.stamp.max(stamp);
                        t.counter = higher(t.counter, counter);
                    }
                    None => replica.tombstones.push(Tombstone { id: id(n), stamp, counter }),
                }
                None
            }
        }
    }

    proptest! {
        /// Three devices edit independently, then exchange snapshots in two different orders: they
        /// all end up with the same records, merging once more changes nothing, and no live
        /// record's counter is below one any device reached for it.
        #[test]
        fn replicas_converge_whatever_the_order(
            ops in proptest::collection::vec((0u64..3, op()), 0..40),
            order in Just(vec![0usize, 1, 2]).prop_shuffle(),
        ) {
            let mut replicas = vec![Replica::default(), Replica::default(), Replica::default()];
            let mut reached = std::collections::BTreeMap::<u8, u64>::new();
            for (step, (device, op)) in ops.iter().enumerate() {
                let stamp = Hlc { wall_ms: step as u64, counter: 0, device: *device };
                if let Some((n, counter)) = apply(&mut replicas[usize::try_from(*device).unwrap()], op, stamp) {
                    let highest = reached.entry(n).or_insert(0);
                    *highest = (*highest).max(counter);
                }
            }
            // Gossip: everyone takes in everyone, in the order given, twice round.
            let mut forward = replicas.clone();
            for _ in 0..2 {
                for &from in &order {
                    for to in 0..3 {
                        let other = forward[from].clone();
                        forward[to].absorb(&other);
                    }
                }
            }
            // The reverse order on a fresh copy.
            let mut backward = replicas.clone();
            for _ in 0..2 {
                for &from in order.iter().rev() {
                    for to in (0..3).rev() {
                        let other = backward[from].clone();
                        backward[to].absorb(&other);
                    }
                }
            }
            let expected = forward[0].normal();
            for replica in forward.iter().chain(backward.iter()) {
                prop_assert_eq!(replica.normal(), expected.clone());
            }
            let mut again = forward[0].clone();
            prop_assert!(!again.absorb(&forward[1]), "idempotent");
            for item in &expected.0 {
                let n = item.id.as_bytes()[0];
                if let Some(&highest) = reached.get(&n) {
                    prop_assert!(item.counter.unwrap_or(0) >= highest, "{item:?} went back below {highest}");
                }
            }
        }
    }
}
