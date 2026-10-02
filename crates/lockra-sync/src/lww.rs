//! Last-writer-wins merge of records keyed by id, with tombstones for deletions: a state-based
//! CRDT (an LWW element set). Merging is commutative, associative and idempotent, so devices that
//! have seen the same snapshots hold the same records, whatever order they saw them in.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Hlc;

/// A record a replica holds, such as an account. Every change to what syncs (a rename, a moved
/// group, the next HOTP counter) takes a new stamp; the whole later version wins. Fields merged by
/// rules of their own would not converge once a deletion is involved (the deleted version's value
/// is gone on the devices that applied the deletion first), so there are none.
pub trait Record: Clone + PartialEq {
    /// Its id, the same on every device.
    fn id(&self) -> Uuid;
    /// When it last changed.
    fn stamp(&self) -> Hlc;
}

/// A deleted record: its id and when it was deleted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    /// The deleted record's id.
    pub id: Uuid,
    /// When it was deleted.
    pub stamp: Hlc,
}

/// Fold another replica's records and tombstones into `records` and `tombstones`; `true` when
/// they changed. The later of a record's versions wins; a deletion later than a record's last
/// change removes it, and a change later than a deletion brings it back.
pub fn merge<R: Record>(records: &mut Vec<R>, tombstones: &mut Vec<Tombstone>, their_records: &[R], their_tombstones: &[Tombstone]) -> bool {
    let mut changed = false;
    for theirs in their_tombstones {
        match tombstones.iter_mut().find(|own| own.id == theirs.id) {
            Some(own) => {
                if theirs.stamp > own.stamp {
                    own.stamp = theirs.stamp;
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
                if theirs.stamp() > own.stamp() {
                    *own = theirs.clone();
                    changed = true;
                }
            }
            None => {
                if !deleted(tombstones, theirs) {
                    records.push(theirs.clone());
                    changed = true;
                }
            }
        }
    }
    let before = records.len();
    records.retain(|record| !deleted(tombstones, record));
    changed | (records.len() != before)
}

/// A tombstone at or after `record`'s last change.
fn deleted<R: Record>(tombstones: &[Tombstone], record: &R) -> bool {
    tombstones.iter().any(|t| t.id == record.id() && t.stamp >= record.stamp())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    /// A record with a value that wins with its stamp.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Item {
        id: Uuid,
        stamp: Hlc,
        value: u8,
    }

    impl Record for Item {
        fn id(&self) -> Uuid {
            self.id
        }

        fn stamp(&self) -> Hlc {
            self.stamp
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

    #[test]
    fn the_later_version_wins_whole() {
        let mut ours = Replica { items: vec![Item { id: id(1), stamp: at(10, 1), value: 1 }], tombstones: vec![] };
        let theirs = Replica { items: vec![Item { id: id(1), stamp: at(20, 2), value: 2 }], tombstones: vec![] };
        assert!(ours.absorb(&theirs));
        assert_eq!(ours.items, [Item { id: id(1), stamp: at(20, 2), value: 2 }]);
        assert!(!ours.absorb(&theirs), "the same snapshot again changes nothing");
        let older = Replica { items: vec![Item { id: id(1), stamp: at(5, 3), value: 9 }], tombstones: vec![] };
        assert!(!ours.absorb(&older), "an older version loses");
        // A record the other replica has never seen arrives.
        assert!(ours.absorb(&Replica { items: vec![Item { id: id(2), stamp: at(1, 3), value: 4 }], tombstones: vec![] }));
        assert_eq!(ours.items.len(), 2);
    }

    #[test]
    fn a_later_deletion_removes_and_a_later_change_brings_back() {
        let item = Item { id: id(1), stamp: at(10, 1), value: 1 };
        let mut ours = Replica { items: vec![item.clone()], tombstones: vec![] };
        let deletion = Replica { items: vec![], tombstones: vec![Tombstone { id: id(1), stamp: at(20, 2) }] };
        assert!(ours.absorb(&deletion));
        assert!(ours.items.is_empty());
        // The old version arriving from a third device stays deleted.
        assert!(!ours.absorb(&Replica { items: vec![item.clone()], tombstones: vec![] }));
        assert!(ours.items.is_empty());
        // An edit after the deletion wins.
        let revived = Item { stamp: at(30, 3), value: 7, ..item };
        assert!(ours.absorb(&Replica { items: vec![revived.clone()], tombstones: vec![] }));
        assert_eq!(ours.items, [revived]);
        // An older tombstone changes nothing; a newer one for an unknown record is kept.
        assert!(!ours.absorb(&Replica { items: vec![], tombstones: vec![Tombstone { id: id(1), stamp: at(15, 4) }] }));
        assert!(ours.absorb(&Replica { items: vec![], tombstones: vec![Tombstone { id: id(9), stamp: at(1, 4) }] }));
        assert_eq!(ours.tombstones.len(), 2);
    }

    /// One device's edit: create or change record `n`, or delete it.
    #[derive(Debug, Clone)]
    enum Op {
        Put { n: u8, value: u8 },
        Delete { n: u8 },
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![(0u8..6, any::<u8>()).prop_map(|(n, value)| Op::Put { n, value }), (0u8..6).prop_map(|n| Op::Delete { n }),]
    }

    /// Apply `op` on `replica` (device `device`) at stamp `stamp`.
    fn apply(replica: &mut Replica, op: &Op, stamp: Hlc) {
        match *op {
            Op::Put { n, value } => {
                let item = Item { id: id(n), stamp, value };
                replica.items.retain(|i| i.id != item.id);
                replica.tombstones.retain(|t| t.id != item.id || t.stamp > stamp);
                replica.items.push(item);
            }
            Op::Delete { n } => {
                replica.items.retain(|i| i.id != id(n));
                replica.tombstones.retain(|t| t.id != id(n));
                replica.tombstones.push(Tombstone { id: id(n), stamp });
            }
        }
    }

    proptest! {
        /// Three devices edit independently, then exchange snapshots in two different orders: they
        /// all end up with the same records, and merging once more changes nothing.
        #[test]
        fn replicas_converge_whatever_the_order(
            ops in proptest::collection::vec((0u64..3, op()), 0..40),
            order in Just(vec![0usize, 1, 2]).prop_shuffle(),
        ) {
            let mut replicas = vec![Replica::default(), Replica::default(), Replica::default()];
            for (step, (device, op)) in ops.iter().enumerate() {
                let stamp = Hlc { wall_ms: step as u64, counter: 0, device: *device };
                apply(&mut replicas[usize::try_from(*device).unwrap()], op, stamp);
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
        }
    }
}
