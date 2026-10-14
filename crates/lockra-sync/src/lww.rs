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
    /// Fold another replica's version of this record in; `true` when this one changed. The
    /// default is the whole record, last writer wins: the later version replaces this one. A
    /// record whose parts change apart (a login's fields) merges them one by one instead, and
    /// must stay commutative, associative and idempotent, as [`merge`] is.
    fn fold(&mut self, theirs: &Self) -> bool {
        if theirs.stamp() > self.stamp() {
            *self = theirs.clone();
            true
        } else {
            false
        }
    }
    /// Drop the parts a deletion at `deleted` covers, those stamped at or before it; `true` when
    /// something went. A record that folds part by part must: a version brought back by a change
    /// after the deletion then keeps only what was written after it, on every replica alike,
    /// whichever arrived first, the deletion or an older version folded in elsewhere. The default
    /// has one part, the whole record, which the deletion check covers already.
    fn forget(&mut self, _deleted: Hlc) -> bool {
        false
    }
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
            // Both hold it: the record folds the other version in (by default the later one wins
            // whole), then its counter rises to the higher of the two.
            Some(own) => {
                let floor = higher(own.counter(), theirs.counter());
                match tombstones.iter().find(|t| t.id == theirs.id()) {
                    // Brought back after a deletion here: only what came after it folds in.
                    Some(tombstone) => {
                        let mut later = theirs.clone();
                        later.forget(tombstone.stamp);
                        changed |= own.fold(&later);
                    }
                    None => changed |= own.fold(theirs),
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
                    // Brought back: what the deletion covers stays deleted.
                    changed |= record.forget(tombstone.stamp);
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

        /// The default fold is the merge as it was before records could fold: the same records,
        /// in the same order, the same tombstones and the same answer, whatever the replicas hold.
        #[test]
        fn the_default_fold_merges_as_before(
            ops in proptest::collection::vec((0u64..3, op()), 0..40),
            pairs in proptest::collection::vec((0usize..3, 0usize..3), 1..12),
        ) {
            let mut replicas = vec![Replica::default(), Replica::default(), Replica::default()];
            for (step, (device, op)) in ops.iter().enumerate() {
                let stamp = Hlc { wall_ms: step as u64, counter: 0, device: *device };
                apply(&mut replicas[usize::try_from(*device).unwrap()], op, stamp);
            }
            let mut before = replicas.clone();
            for &(from, to) in &pairs {
                let other = replicas[from].clone();
                let changed = replicas[to].absorb(&other);
                let theirs = before[from].clone();
                let target = &mut before[to];
                let expected = merge_before_fold(&mut target.items, &mut target.tombstones, &theirs.items, &theirs.tombstones);
                prop_assert_eq!(changed, expected);
                prop_assert_eq!(&replicas[to], &before[to]);
            }
        }
    }

    /// [`merge`] as it was before [`Record::fold`]: a later version replaced the whole record.
    fn merge_before_fold<R: Record>(records: &mut Vec<R>, tombstones: &mut Vec<Tombstone>, their_records: &[R], their_tombstones: &[Tombstone]) -> bool {
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

    /// A record whose parts change apart: registers by key, each with its own stamp, the later
    /// one of each kept; ties go to the larger value. Its registers stay sorted by key, so two
    /// replicas that hold the same registers hold the same array.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
    struct Card {
        id: Uuid,
        registers: Vec<Register>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
    struct Register {
        key: u8,
        value: u8,
        stamp: Hlc,
    }

    impl Record for Card {
        fn id(&self) -> Uuid {
            self.id
        }

        fn stamp(&self) -> Hlc {
            self.registers.iter().map(|r| r.stamp).max().unwrap_or_default()
        }

        fn fold(&mut self, theirs: &Self) -> bool {
            let mut changed = false;
            for register in &theirs.registers {
                match self.registers.iter_mut().find(|own| own.key == register.key) {
                    Some(own) => {
                        if (register.stamp, register.value) > (own.stamp, own.value) {
                            *own = *register;
                            changed = true;
                        }
                    }
                    None => {
                        self.registers.push(*register);
                        changed = true;
                    }
                }
            }
            self.registers.sort_by_key(|r| r.key);
            changed
        }

        fn forget(&mut self, deleted: Hlc) -> bool {
            let before = self.registers.len();
            self.registers.retain(|r| r.stamp > deleted);
            self.registers.len() != before
        }
    }

    #[derive(Debug, Clone, Default, PartialEq)]
    struct Cards {
        cards: Vec<Card>,
        tombstones: Vec<Tombstone>,
    }

    impl Cards {
        fn absorb(&mut self, other: &Self) -> bool {
            merge(&mut self.cards, &mut self.tombstones, &other.cards, &other.tombstones)
        }

        /// The records in id order (they arrive in any order), each exactly as held: its
        /// registers are not sorted here, they must already be in their one order.
        fn normal(&self) -> (Vec<u8>, Vec<Tombstone>) {
            let mut cards = self.cards.clone();
            cards.sort_by_key(|c| c.id);
            let mut tombstones = self.tombstones.clone();
            tombstones.sort_by_key(|t| t.id);
            (serde_json::to_vec(&cards).unwrap(), tombstones)
        }

        /// Set register `key` of card `n` at `stamp`: the card's other registers stay as they are.
        fn set(&mut self, n: u8, key: u8, value: u8, stamp: Hlc) {
            let register = Register { key, value, stamp };
            match self.cards.iter_mut().find(|c| c.id == id(n)) {
                Some(card) => match card.registers.iter_mut().find(|r| r.key == key) {
                    Some(own) => *own = register,
                    None => {
                        card.registers.push(register);
                        card.registers.sort_by_key(|r| r.key);
                    }
                },
                None => self.cards.push(Card { id: id(n), registers: vec![register] }),
            }
        }

        fn delete(&mut self, n: u8, stamp: Hlc) {
            if self.cards.iter().any(|c| c.id == id(n)) {
                self.cards.retain(|c| c.id != id(n));
                match self.tombstones.iter_mut().find(|t| t.id == id(n)) {
                    Some(t) => t.stamp = t.stamp.max(stamp),
                    None => self.tombstones.push(Tombstone { id: id(n), stamp, counter: None }),
                }
            }
        }
    }

    #[test]
    fn records_that_fold_keep_the_later_part_of_each_side() {
        let mut ours = Cards::default();
        ours.set(1, 0, 1, at(10, 1));
        ours.set(1, 1, 1, at(10, 1));
        let mut theirs = ours.clone();
        // Their change to the first part, ours to the second one, later.
        theirs.set(1, 0, 2, at(20, 2));
        ours.set(1, 1, 3, at(30, 1));
        assert!(ours.absorb(&theirs));
        assert_eq!(ours.cards[0].registers, [Register { key: 0, value: 2, stamp: at(20, 2) }, Register { key: 1, value: 3, stamp: at(30, 1) }]);
        assert!(theirs.absorb(&ours));
        assert_eq!(theirs.normal(), ours.normal());
        assert!(!ours.absorb(&theirs), "the same state again changes nothing");
        // A deletion after every part removes the record; one between the parts does not.
        let mut deleted = ours.clone();
        deleted.absorb(&Cards { cards: vec![], tombstones: vec![tombstone(1, at(25, 3))] });
        // The second part came after the deletion and brings the record back, alone: the first
        // part is one the deletion covers.
        assert_eq!(deleted.cards[0].registers, [Register { key: 1, value: 3, stamp: at(30, 1) }]);
        // A version from before the deletion, folded in afterwards, brings nothing back either,
        // and says so.
        assert!(!deleted.absorb(&theirs), "{deleted:?}");
        assert_eq!(deleted.cards[0].registers, [Register { key: 1, value: 3, stamp: at(30, 1) }]);
        assert!(deleted.absorb(&Cards { cards: vec![], tombstones: vec![tombstone(1, at(40, 3))] }));
        assert!(deleted.cards.is_empty());
    }

    /// One device's edit of a card: set one of its parts, or delete it.
    #[derive(Debug, Clone)]
    enum CardOp {
        Set { n: u8, key: u8, value: u8 },
        Delete { n: u8 },
    }

    fn card_op() -> impl Strategy<Value = CardOp> {
        prop_oneof![
            4 => (0u8..4, 0u8..4, any::<u8>()).prop_map(|(n, key, value)| CardOp::Set { n, key, value }),
            1 => (0u8..4).prop_map(|n| CardOp::Delete { n }),
        ]
    }

    proptest! {
        /// Records that fold part by part converge as whole records do: whatever the order, with
        /// snapshots taken in more than once, three replicas end with the same records, their
        /// parts in the same array byte for byte.
        #[test]
        fn records_that_fold_converge_whatever_the_order(
            ops in proptest::collection::vec((0u64..3, card_op()), 0..48),
            order in Just(vec![0usize, 1, 2]).prop_shuffle(),
            first in 0usize..3,
        ) {
            let mut replicas = vec![Cards::default(), Cards::default(), Cards::default()];
            for (step, (device, op)) in ops.iter().enumerate() {
                let stamp = Hlc { wall_ms: step as u64 / 2, counter: 0, device: *device };
                let replica = &mut replicas[usize::try_from(*device).unwrap()];
                match *op {
                    CardOp::Set { n, key, value } => replica.set(n, key, value, stamp),
                    CardOp::Delete { n } => replica.delete(n, stamp),
                }
                // Now and then a device takes in another's snapshot as it is at that moment.
                if step % 5 == 4 {
                    let from = (usize::try_from(*device).unwrap() + first) % 3;
                    let other = replicas[from].clone();
                    replicas[usize::try_from(*device).unwrap()].absorb(&other);
                }
            }
            let mut forward = replicas.clone();
            for _ in 0..2 {
                for &from in &order {
                    for to in 0..3 {
                        let other = forward[from].clone();
                        forward[to].absorb(&other);
                    }
                }
            }
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
