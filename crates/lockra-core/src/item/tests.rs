//! The item's merge: part by part, conflicts derived from what each write had seen, deletions
//! covering what came before them. Devices are replicas of the items with a clock each, merged
//! with lockra-sync's `merge` as the sync merges them.

use lockra_sync::{Clock, Tombstone, merge};
use proptest::prelude::*;

use super::*;

const ITEM: Uuid = Uuid::from_u128(0x100);
const USERNAME: Uuid = Uuid::from_u128(0x1);
const PASSWORD: Uuid = Uuid::from_u128(0x2);

/// A device: its clock and its replica of the items.
#[derive(Debug, Clone)]
struct Device {
    clock: Clock,
    items: Vec<VaultItem>,
    tombstones: Vec<Tombstone>,
    /// The next write id it hands out (unique per device here, random in the app).
    next_wid: u64,
}

impl Device {
    fn new(number: u64) -> Self {
        Self { clock: Clock::new(number), items: Vec::new(), tombstones: Vec::new(), next_wid: number << 32 }
    }

    /// A copy of this vault on another computer, under the same device number until it takes
    /// another (`renumber`); its write ids differ, as random ones would.
    fn copy(&self, salt: u64) -> Self {
        Self { next_wid: self.next_wid ^ (salt << 48), ..self.clone() }
    }

    /// Take a new device number, as the app does on a clash: its clock goes on from where it was.
    fn renumber(&mut self, number: u64) {
        let mut clock = Clock::new(number);
        clock.observe(self.clock.last());
        self.clock = clock;
    }

    fn wid(&mut self) -> u64 {
        self.next_wid += 1;
        self.next_wid
    }

    fn item(&self) -> &VaultItem {
        self.items.iter().find(|i| i.id == ITEM).expect("the item is here")
    }

    fn has_item(&self) -> bool {
        self.items.iter().any(|i| i.id == ITEM)
    }

    /// Make the login: `octocat` and the password `first`.
    fn create(&mut self, wall_ms: u64) {
        let (stamp, mwid) = (self.clock.tick(wall_ms), self.wid());
        let mut item = VaultItem::login(ITEM, wall_ms, stamp, mwid);
        item.title = "GitHub".into();
        let (stamp, wid) = (self.clock.tick(wall_ms), self.wid());
        item.write_field(USERNAME, Purpose::Username, stamp, wid, wall_ms, |f| f.value = Zeroizing::new("octocat".into()));
        let (stamp, wid) = (self.clock.tick(wall_ms), self.wid());
        item.write_field(PASSWORD, Purpose::Password, stamp, wid, wall_ms, |f| {
            f.value = Zeroizing::new("first".into());
            f.hidden = true;
        });
        self.items.push(item);
    }

    fn write(&mut self, field: Uuid, value: &str, wall_ms: u64) {
        let (stamp, wid) = (self.clock.tick(wall_ms), self.wid());
        let purpose = if field == USERNAME { Purpose::Username } else { Purpose::Password };
        let Some(item) = self.items.iter_mut().find(|i| i.id == ITEM) else { return };
        item.write_field(field, purpose, stamp, wid, wall_ms, |f| {
            f.value = Zeroizing::new(value.into());
            f.hidden = field == PASSWORD;
        });
    }

    fn set_password(&mut self, value: &str, wall_ms: u64) {
        self.write(PASSWORD, value, wall_ms);
    }

    fn set_notes(&mut self, value: &str, wall_ms: u64) {
        let (stamp, mwid) = (self.clock.tick(wall_ms), self.wid());
        let Some(item) = self.items.iter_mut().find(|i| i.id == ITEM) else { return };
        item.notes = Zeroizing::new(value.into());
        item.meta_written(stamp, mwid, wall_ms);
    }

    fn set_title(&mut self, value: &str, wall_ms: u64) {
        let (stamp, mwid) = (self.clock.tick(wall_ms), self.wid());
        let Some(item) = self.items.iter_mut().find(|i| i.id == ITEM) else { return };
        item.title = value.into();
        item.meta_written(stamp, mwid, wall_ms);
    }

    fn readopt(&mut self, known: &Field, wall_ms: u64) {
        let stamp = self.clock.tick(wall_ms);
        let Some(item) = self.items.iter_mut().find(|i| i.id == ITEM) else { return };
        item.readopt_field(known, stamp, wall_ms);
    }

    fn settle(&mut self, wall_ms: u64) {
        let (stamp, wid) = (self.clock.tick(wall_ms), self.wid());
        let Some(item) = self.items.iter_mut().find(|i| i.id == ITEM) else { return };
        let value = item.field(PASSWORD).map(|f| f.value.clone()).unwrap_or_default();
        item.settle_field(PASSWORD, value, stamp, wid, wall_ms);
    }

    fn delete(&mut self, wall_ms: u64) {
        if !self.has_item() {
            return;
        }
        let stamp = self.clock.tick(wall_ms);
        self.items.retain(|i| i.id != ITEM);
        match self.tombstones.iter_mut().find(|t| t.id == ITEM) {
            Some(t) => t.stamp = t.stamp.max(stamp),
            None => self.tombstones.push(Tombstone { id: ITEM, stamp, counter: None }),
        }
    }

    /// Take in `other`'s replica, as a sync run does, and move the clock past what came in.
    fn absorb(&mut self, other: &Self) -> bool {
        let changed = merge(&mut self.items, &mut self.tombstones, &other.items, &other.tombstones);
        let latest = self.items.iter().map(Record::stamp).chain(self.tombstones.iter().map(|t| t.stamp)).max();
        if let Some(latest) = latest {
            self.clock.observe(latest);
        }
        changed
    }

    fn password(&self) -> String {
        self.item().field(PASSWORD).map(|f| f.value.to_string()).unwrap_or_default()
    }

    /// The replica as bytes: the items in id order, each exactly as held, and the tombstones.
    fn bytes(&self) -> Vec<u8> {
        let mut items = self.items.clone();
        items.sort_by_key(|i| i.id);
        let mut tombstones = self.tombstones.clone();
        tombstones.sort_by_key(|t| t.id);
        serde_json::to_vec(&(items, tombstones)).unwrap()
    }
}

/// Two devices that both hold the login made on the first.
fn pair() -> (Device, Device) {
    let mut a = Device::new(1);
    a.create(1_000);
    let mut b = Device::new(2);
    b.absorb(&a);
    (a, b)
}

fn sync(a: &mut Device, b: &mut Device) {
    let theirs = b.clone();
    a.absorb(&theirs);
    let ours = a.clone();
    b.absorb(&ours);
}

#[test]
fn notes_changed_on_one_device_and_the_password_on_another_both_stay() {
    let (mut a, mut b) = pair();
    a.set_notes("recovery codes are in the safe", 2_000);
    b.set_password("second", 2_100);
    sync(&mut a, &mut b);
    for device in [&a, &b] {
        assert_eq!(device.password(), "second", "the password stays");
        assert_eq!(&*device.item().notes, "recovery codes are in the safe", "and so do the notes");
        assert!(!device.item().conflict(), "different parts: no conflict");
    }
    assert_eq!(a.bytes(), b.bytes());
}

#[test]
fn one_password_changed_on_two_devices_at_once_is_a_conflict() {
    let (mut a, mut b) = pair();
    a.set_password("from a", 2_000);
    b.set_password("from b", 2_100);
    sync(&mut a, &mut b);
    assert_eq!(a.bytes(), b.bytes());
    assert_eq!(a.password(), "from b", "the later write wins");
    assert!(a.item().conflict());
    assert_eq!(a.item().conflicted_fields().map(|f| f.id).collect::<Vec<_>>(), [PASSWORD]);
    let history: Vec<&str> = a.item().history.iter().map(|p| p.value.as_str()).collect();
    assert_eq!(history, ["from a", "first"], "the other value and the first one, newest first");
}

#[test]
fn changes_made_one_after_another_are_no_conflict() {
    let (mut a, mut b) = pair();
    a.set_password("second", 2_000);
    sync(&mut a, &mut b);
    b.set_password("third", 3_000);
    sync(&mut a, &mut b);
    a.set_password("fourth", 4_000);
    sync(&mut a, &mut b);
    assert_eq!(b.password(), "fourth");
    assert!(!a.item().conflict() && !b.item().conflict(), "{:?}", a.item());
    assert_eq!(a.item().history.len(), 3);
}

#[test]
fn a_device_whose_clock_is_behind_still_conflicts_with_a_run_of_changes() {
    // A goes from the first password through a run of changes; B, its clock behind, changes the
    // first one meanwhile. Neither the stamps nor the device numbers tell they were written apart;
    // what B's value's author had seen does.
    let (mut a, mut b) = pair();
    for (n, wall_ms) in (2..=12).zip((5_000..).step_by(10)) {
        a.set_password(&format!("v{n}"), wall_ms);
    }
    a.set_password("v13", 9_000);
    b.set_password("v50", 1_500);
    sync(&mut a, &mut b);
    assert_eq!(a.password(), "v13");
    assert!(a.item().history.iter().any(|p| p.value.as_str() == "v50"), "the lost value is kept");
    assert!(a.item().conflict() && b.item().conflict());
    assert_eq!(a.bytes(), b.bytes());
}

#[test]
fn a_conflicting_value_pushed_out_of_the_history_takes_the_conflict_with_it() {
    // The history keeps the newest twenty values by when they were written: a value from a
    // device whose clock is behind is the oldest, and twenty later ones push it out, and with it
    // the conflict it would raise (docs/formats.md §2).
    let (mut a, mut b) = pair();
    for (n, wall_ms) in (2..=40).zip((5_000..).step_by(10)) {
        a.set_password(&format!("v{n}"), wall_ms);
    }
    b.set_password("behind", 1_500);
    sync(&mut a, &mut b);
    assert!(a.item().history.iter().all(|p| p.value.as_str() != "behind"));
    assert!(!a.item().conflict());
    assert_eq!(a.bytes(), b.bytes());
}

#[test]
fn two_copies_of_a_vault_changing_the_password_apart_conflict_after_one_renumbers() {
    let mut a = Device::new(1);
    a.create(1_000);
    let mut copy = a.copy(1);
    // The copy finds its name taken and becomes another device, then each changes the password
    // at another time.
    copy.renumber(9);
    a.set_password("on the original", 2_000);
    copy.set_password("on the copy", 3_000);
    sync(&mut a, &mut copy);
    assert_eq!(a.password(), "on the copy");
    assert!(a.item().conflict() && copy.item().conflict());
    assert_eq!(a.bytes(), copy.bytes());
}

#[test]
fn two_copies_writing_the_same_value_at_the_same_moment_end_alike_without_a_conflict() {
    let mut a = Device::new(1);
    a.create(1_000);
    let mut copy = a.copy(1);
    a.set_password("same", 2_000);
    copy.set_password("same", 2_000);
    assert_eq!(a.item().field(PASSWORD).unwrap().stamp, copy.item().field(PASSWORD).unwrap().stamp, "one stamp, two writes");
    assert_ne!(a.item().field(PASSWORD).unwrap().wid, copy.item().field(PASSWORD).unwrap().wid);
    sync(&mut a, &mut copy);
    assert_eq!(a.bytes(), copy.bytes(), "the write id decides, the same way on both");
    assert_eq!(a.password(), "same");
    assert!(!a.item().conflict(), "the same value either way: nothing to choose");
    assert!(a.item().history.iter().any(|p| p.value.as_str() == "same"), "the other write is kept all the same");
}

#[test]
fn one_write_against_two_at_the_same_moment_is_still_a_conflict() {
    let mut a = Device::new(1);
    a.create(1_000);
    let mut copy = a.copy(1);
    a.set_password("once", 2_000);
    copy.set_password("twice, first", 2_000);
    copy.set_password("twice, second", 2_000);
    sync(&mut a, &mut copy);
    assert_eq!(a.bytes(), copy.bytes());
    assert_eq!(a.password(), "twice, second");
    assert!(a.item().conflict());
    let history: BTreeSet<&str> = a.item().history.iter().map(|p| p.value.as_str()).collect();
    assert_eq!(history, BTreeSet::from(["first", "once", "twice, first"]), "nothing swallowed");
}

#[test]
fn the_metadata_at_the_same_moment_goes_to_the_larger_write_id() {
    let mut a = Device::new(1);
    a.create(1_000);
    let mut copy = a.copy(1);
    a.set_title("Original", 2_000);
    copy.set_title("Copy", 2_000);
    let (ours, theirs) = (a.item().mwid, copy.item().mwid);
    assert_eq!(a.item().stamp, copy.item().stamp);
    sync(&mut a, &mut copy);
    assert_eq!(a.bytes(), copy.bytes());
    assert_eq!(a.item().title, if theirs > ours { "Copy" } else { "Original" });
}

#[test]
fn two_copies_taking_the_same_value_back_at_the_same_moment_end_alike_in_any_order() {
    let mut a = Device::new(1);
    a.create(1_000);
    let first = a.item().field(PASSWORD).unwrap().clone();
    let mut copy = a.copy(1);
    a.set_password("a", 2_000);
    copy.set_password("b", 2_000);
    // Both restore the first value at the same moment: one write, seen differently.
    a.readopt(&first, 3_000);
    copy.readopt(&first, 3_000);
    let (ours, theirs) = (a.item().field(PASSWORD).unwrap().clone(), copy.item().field(PASSWORD).unwrap().clone());
    assert_eq!((ours.stamp, ours.wid), (theirs.stamp, theirs.wid));
    assert_ne!(ours.seen, theirs.seen);
    let mut third = Device::new(3);
    let replicas = [a, copy, third.clone()];
    let mut results = Vec::new();
    for order in [[0, 1, 2], [2, 1, 0], [1, 0, 2], [2, 0, 1]] {
        let mut devices = replicas.clone();
        for _ in 0..2 {
            for &from in &order {
                for to in 0..3 {
                    let other = devices[from].clone();
                    devices[to].absorb(&other);
                }
            }
        }
        for device in &devices {
            results.push(device.bytes());
        }
    }
    assert!(results.iter().all(|r| *r == results[0]), "every order, every replica, byte for byte");
    third.absorb(&replicas[0]);
    third.absorb(&replicas[1]);
    let seen = &third.item().field(PASSWORD).unwrap().seen;
    assert!(ours.seen.iter().chain(&theirs.seen).all(|w| seen.contains(w)), "the same write has seen what both copies saw");
    assert_eq!(third.password(), "first");
    // Both copies replaced their own value with the first one, so the value kept has seen both:
    // nothing is left to choose between.
    assert!(!third.item().conflict());
}

#[test]
fn settling_a_conflict_clears_it_on_every_replica() {
    let (mut a, mut b) = pair();
    a.set_password("from a", 2_000);
    b.set_password("from b", 2_100);
    sync(&mut a, &mut b);
    assert!(a.item().conflict());
    // Keep the current value: a new write that has seen everything in the history.
    a.settle(3_000);
    assert!(!a.item().conflict());
    assert_eq!(a.password(), "from b");
    sync(&mut a, &mut b);
    assert!(!b.item().conflict(), "settled everywhere");
    assert_eq!(a.bytes(), b.bytes());
    // A change made without seeing the settlement, and winning, brings a conflict back.
    let mut c = Device::new(3);
    c.absorb(&a);
    a.set_password("after", 4_000);
    c.set_password("unaware", 4_100);
    sync(&mut a, &mut c);
    assert!(a.item().conflict());
    // Nothing to settle without the field.
    assert!(!a.items[0].clone().settle_field(Uuid::from_u128(77), Zeroizing::new("x".into()), Hlc::at(9), 1, 9));
}

#[test]
fn a_login_deleted_while_another_device_changes_it_comes_back_with_the_later_parts() {
    let (mut a, mut b) = pair();
    let mut c = b.clone();
    c.clock = Clock::new(3);
    c.clock.observe(b.clock.last());
    // C renames it before the deletion; A deletes it; B, having seen neither, changes the
    // password after the deletion.
    c.set_title("GitHub (work)", 1_500);
    a.delete(2_000);
    b.set_password("after the deletion", 3_000);
    let replicas = [a, b, c];
    let mut results = Vec::new();
    for order in [[0, 1, 2], [2, 1, 0], [1, 2, 0], [2, 0, 1]] {
        let mut devices = replicas.clone();
        for _ in 0..2 {
            for &from in &order {
                for to in 0..3 {
                    let other = devices[from].clone();
                    devices[to].absorb(&other);
                }
            }
        }
        for device in &devices {
            results.push(device.bytes());
        }
    }
    assert!(results.iter().all(|r| *r == results[0]), "whichever came first, the deletion or the rename");
    let mut a = replicas[0].clone();
    a.absorb(&replicas[2]);
    a.absorb(&replicas[1]);
    let item = a.item();
    assert_eq!(a.password(), "after the deletion");
    assert_eq!(item.fields.iter().map(|f| f.id).collect::<Vec<_>>(), [PASSWORD], "the user name was written before the deletion");
    assert_eq!((item.title.as_str(), item.stamp, item.mwid), ("", Hlc::default(), 0), "and so was every name");
    assert!(item.history.is_empty(), "the old password went with the deletion");
    assert_eq!(item.kind, Lenient::Known(ItemKind::Login));
    // A metadata change after the deletion comes back as well.
    let mut b = replicas[1].clone();
    b.set_notes("new notes", 3_500);
    a.absorb(&b);
    assert_eq!(&*a.item().notes, "new notes");
}

#[test]
fn the_history_keeps_the_newest_twenty_and_seen_only_what_it_holds() {
    let (mut a, _) = pair();
    for n in 0..30 {
        a.set_password(&format!("p{n}"), 2_000 + n);
    }
    let item = a.item();
    assert_eq!(item.history.len(), MAX_HISTORY);
    assert_eq!(item.history[0].value.as_str(), "p28", "newest first");
    let held: BTreeSet<u64> = item.history.iter().map(|p| p.wid).collect();
    let field = item.field(PASSWORD).unwrap();
    assert!(field.seen.iter().all(|w| held.contains(w)));
    assert_eq!(field.seen.len(), MAX_HISTORY);
    assert!(field.seen.windows(2).all(|w| w[0] < w[1]), "sorted, once each");
    // A text field keeps no history and has seen nothing.
    a.write(USERNAME, "renamed", 3_000);
    assert!(a.item().field(USERNAME).unwrap().seen.is_empty());
    assert!(a.item().history.iter().all(|p| p.field == PASSWORD));
}

#[test]
fn fields_added_on_two_devices_at_once_end_in_one_order() {
    let (mut a, mut b) = pair();
    for (device, id, label, wall_ms) in [(&mut a, Uuid::from_u128(0x30), "PIN", 2_000), (&mut b, Uuid::from_u128(0x20), "Recovery", 2_000)] {
        let (stamp, wid) = (device.clock.tick(wall_ms), device.wid());
        let item = device.items.iter_mut().find(|i| i.id == ITEM).unwrap();
        item.write_field(id, Purpose::Custom, stamp, wid, wall_ms, |f| f.label = label.into());
    }
    sync(&mut a, &mut b);
    assert_eq!(a.bytes(), b.bytes());
    let labels: Vec<&str> = a.item().fields.iter().map(|f| f.label.as_str()).collect();
    assert_eq!(labels, ["", "", "PIN", "Recovery"], "user name, password, then the custom fields as added (stamp, then id)");
    // Removing one keeps its register, so the removal syncs; a hidden one's value goes to the
    // history.
    let (stamp, wid) = (a.clock.tick(3_000), a.wid());
    let item = a.items.iter_mut().find(|i| i.id == ITEM).unwrap();
    item.write_field(PASSWORD, Purpose::Password, stamp, wid, 3_000, |f| {
        f.removed = true;
        f.value = Zeroizing::new(String::new());
    });
    assert!(a.item().field_of(Purpose::Password).is_none());
    assert_eq!(a.item().history[0].value.as_str(), "first");
    sync(&mut a, &mut b);
    assert!(b.item().field(PASSWORD).unwrap().removed);
}

#[test]
fn an_item_taken_in_whole_is_put_in_the_one_order() {
    let (mut a, _) = pair();
    a.set_password("second", 2_000);
    a.set_password("third", 3_000);
    let tidy = a.item().clone();
    assert!(tidy.is_tidy());
    let mut untidy = tidy.clone();
    untidy.fields.reverse();
    untidy.history.reverse();
    untidy.history.push(untidy.history[0].clone());
    assert!(!untidy.is_tidy());
    let (mut items, mut tombstones) = (Vec::new(), Vec::new());
    assert!(crate::sync::merge_items(&mut items, &mut tombstones, &[untidy], &[]));
    assert_eq!(items, [tidy], "as every replica holds it");
}

#[test]
fn what_a_later_lockra_writes_is_kept_and_read_safely() {
    let text = r#"{"id":"00000000-0000-0000-0000-000000000100","kind":"bank_card","title":"Card","fields":[{"id":"00000000-0000-0000-0000-000000000001","purpose":"card_number","value":"4111","created":{"wall_ms":1,"counter":0,"device":1},"stamp":{"wall_ms":1,"counter":0,"device":1},"wid":5,"mask":"xx-xx"}],"urls":[{"url":"example.com","matching":"fuzzy","weight":2}],"protection":"vault_only","autofill":{"fill_otp":"ask","later":true},"color":"lime","stamp":{"wall_ms":1,"counter":0,"device":1},"mwid":3,"expires":"2030-01"}"#;
    let item: VaultItem = serde_json::from_str(text).unwrap();
    assert!(item.read_only());
    assert_eq!(item.protection_level(), Protection::NoExtension, "rather not fill");
    assert_eq!(item.urls[0].rule(), Matching::Never, "rather not match");
    assert_eq!(item.autofill.fill_otp, Lenient::Unknown("ask".into()));
    assert_eq!(item.fields[0].purpose, Lenient::Unknown("card_number".into()));
    // Written back as it came, unknown keys and values included.
    let again: serde_json::Value = serde_json::to_value(&item).unwrap();
    let original: serde_json::Value = serde_json::from_str(text).unwrap();
    for key in ["kind", "expires", "color", "protection"] {
        assert_eq!(again[key], original[key], "{key}");
    }
    assert_eq!(again["fields"][0]["mask"], "xx-xx");
    assert_eq!(again["urls"][0]["weight"], 2);
    assert_eq!(again["autofill"]["later"], true);
    assert_eq!(serde_json::from_value::<VaultItem>(again).unwrap(), item);
    // A plain login writes none of what it leaves at its default.
    let (a, _) = pair();
    let text = serde_json::to_string(a.item()).unwrap();
    for absent in ["\"urls\"", "\"tags\"", "\"notes\"", "\"color\"", "\"protection\"", "\"autofill\"", "\"history\"", "\"removed\"", "\"seen\""] {
        assert!(!text.contains(absent), "{absent} in {text}");
    }
}

#[test]
fn a_site_rule_about_one_domain_has_the_same_id_everywhere() {
    let id = SiteRule::single_id(SiteRuleKind::NeverSave, "Example.com");
    assert_eq!(id, SiteRule::single_id(SiteRuleKind::NeverSave, "example.com"));
    assert_ne!(id, SiteRule::single_id(SiteRuleKind::NeverFill, "example.com"));
    assert_ne!(id, SiteRule::single_id(SiteRuleKind::NeverSave, "example.org"));
    assert_eq!(id.get_version_num(), 8);
    // Merged whole, the later version winning.
    let rule = |stamp: Hlc, domains: &[&str]| SiteRule {
        id,
        kind: Lenient::Known(SiteRuleKind::NeverSave),
        domains: domains.iter().map(|d| (*d).to_owned()).collect(),
        stamp,
        extra: Map::new(),
    };
    let mut ours = vec![rule(Hlc::at(1), &["example.com"])];
    assert!(merge(&mut ours, &mut Vec::new(), &[rule(Hlc::at(2), &["example.com", "example.net"])], &[]));
    assert_eq!(ours[0].domains.len(), 2);
    assert_eq!(Record::stamp(&ours[0]), Hlc::at(2));
    let later: SiteRule =
        serde_json::from_str(r#"{"id":"00000000-0000-0000-0000-000000000001","kind":"never_autofill_otp","stamp":{"wall_ms":1,"counter":0,"device":1}}"#)
            .unwrap();
    assert_eq!(later.kind, Lenient::Unknown("never_autofill_otp".into()));
}

#[test]
fn write_ids_are_random_and_the_local_parts_say_when_they_are_empty() {
    assert_ne!(random_write_id(), random_write_id());
    assert!(ItemListView::default().is_empty());
    assert!(!ItemListView { collapsed_groups: vec![String::new()] }.is_empty());
}

/// One device's step: change a part of the login, take an earlier value back, settle, delete it,
/// or take in another device's replica.
#[derive(Debug, Clone)]
enum Op {
    Password(u8),
    Username(u8),
    Notes(u8),
    Readopt(usize),
    Settle,
    Delete,
    Absorb(usize),
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0u8..4).prop_map(Op::Password),
        2 => (0u8..4).prop_map(Op::Username),
        2 => (0u8..4).prop_map(Op::Notes),
        1 => (0usize..64).prop_map(Op::Readopt),
        1 => Just(Op::Settle),
        1 => Just(Op::Delete),
        2 => (0usize..3).prop_map(Op::Absorb),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Three replicas (the first two copies of one vault, so stamps can be equal) edit the login,
    /// take values back, settle, delete and sync among themselves; then they exchange their
    /// replicas in two different orders: all end with the same bytes, and taking a replica in
    /// again changes nothing.
    #[test]
    fn replicas_of_logins_converge_whatever_the_order(
        ops in proptest::collection::vec((0usize..3, op()), 0..60),
        order in Just(vec![0usize, 1, 2]).prop_shuffle(),
    ) {
        let mut first = Device::new(1);
        first.create(1_000);
        let mut devices = vec![first.clone(), first.copy(1), Device::new(3)];
        devices[2].absorb(&first);
        let mut known: Vec<Field> = first.item().fields.clone();
        for (step, (device, op)) in ops.iter().enumerate() {
            let wall_ms = 2_000 + step as u64 / 3;
            match op {
                Op::Password(n) => devices[*device].set_password(&format!("p{n}"), wall_ms),
                Op::Username(n) => devices[*device].write(USERNAME, &format!("u{n}"), wall_ms),
                Op::Notes(n) => devices[*device].set_notes(&format!("n{n}"), wall_ms),
                Op::Readopt(i) if !known.is_empty() => {
                    let field = known[i % known.len()].clone();
                    devices[*device].readopt(&field, wall_ms);
                }
                Op::Readopt(_) => {}
                Op::Settle => devices[*device].settle(wall_ms),
                Op::Delete => devices[*device].delete(wall_ms),
                Op::Absorb(from) => {
                    let other = devices[*from].clone();
                    devices[*device].absorb(&other);
                }
            }
            if devices[*device].has_item() {
                known.extend(devices[*device].item().fields.iter().cloned());
            }
        }
        let mut results = Vec::new();
        for forward in [true, false] {
            let mut replicas = devices.clone();
            for _ in 0..3 {
                let sequence: Vec<usize> = if forward { order.clone() } else { order.iter().rev().copied().collect() };
                for &from in &sequence {
                    for to in 0..3 {
                        let other = replicas[from].clone();
                        replicas[to].absorb(&other);
                    }
                }
            }
            let mut again = replicas[0].clone();
            prop_assert!(!again.absorb(&replicas[1]), "idempotent");
            for replica in &replicas {
                if replica.has_item() {
                    let item = replica.item();
                    prop_assert!(item.history.len() <= MAX_HISTORY);
                    for field in &item.fields {
                        prop_assert!(field.seen.windows(2).all(|w| w[0] < w[1]), "{:?}", field.seen);
                    }
                }
                results.push(replica.bytes());
            }
        }
        for result in &results {
            prop_assert_eq!(result, &results[0]);
        }
    }
}
