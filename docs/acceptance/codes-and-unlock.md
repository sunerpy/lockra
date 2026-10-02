# Groups, account colours and Touch ID / Windows Hello

What is proven automatically, what was run here, and what has to be checked on real devices before
a release is announced. The design is in `docs/formats.md` (§1 the device slot's `check`, §2 the
entries' `color` and `mark`, the local part's `view`), `docs/security.md` ("At rest") and
`DESIGN.md` ("Account colours").

## Automated

| Layer    | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| vault    | `crates/lockra-vault`: a device slot's check is in the authenticated header (taking it out breaks the file for every key), is kept by a new master password, goes with the device slot, never reaches a backup, and a check from a newer Lockra is kept as it was and named                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| core     | `crates/lockra-core/src/tests.rs`: folded groups kept in the vault for this device only (hidden while locked, back after unlocking and a restart, in no backup and not in `settings.json`, no automatic backup for a fold); the list grouped by default; an account's colour and avatar text (two characters as people count them, cleared, kept by a backup, an unknown colour read as automatic); `tests/sync.rs`: the colour and text reach another device; the check before the remembered key (one passing check to turn it on, cancelled, failed and unavailable told apart, the master password never checked, the vault file keeps it across a restart, the master password to turn it off, a sensor gone refuses instead of skipping) |
| bridge   | `crates/lockra-bridge/tests/contract.rs`: the new commands (`view_collapse_groups`, `device_biometric_enable`, `device_biometric_disable`, `vault_unlock_device` with its reason) and views against the fixtures, every command dispatched once on a real core                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| web      | `Codes.test.tsx`: sections that fold one by one and all at once, a search through folded sections, the group view switch, no sections without groups, the pin and edit buttons, the right-click menu at the pointer and the context-menu key or Shift F10 beside the row; `EntryDialogs.test.tsx`: the colour swatches (arrow keys move the choice), the avatar text cut to two characters, the preview; `Unlock.test.tsx`, `Settings.test.tsx`: the Touch ID and Windows Hello button and switch, a cancelled check left silent; `Menu.test.tsx`: the context menu's place, edges, keys and focus; `theme.test.ts`: ten colours in four themes, each text at 4.5:1 or more on its fill                                                        |
| platform | `cargo xwin clippy -p lockra-desktop --target x86_64-pc-windows-msvc -- -D warnings` (the Windows Hello path); the shell's `biometrics.rs` in a probe crate, `cargo clippy --target aarch64-apple-darwin -- -D warnings` (the Touch ID path, `unsafe_code` forbidden there too); CI's `macos-check` builds the shell on macOS 15                                                                                                                                                                                                                                                                                                                                                                                                               |

## Run here

`make smoke-desktop`, `make site-screens` and `make showcase`, 2026-10-02, Ubuntu 24.04 x64 under
Xvfb. The desktop smoke opens a row's edit button, picks purple and types "GH"
(`screens/desktop/edit-appearance-1280-light`), puts two accounts in a group
(`codes-groups-1280-light`), folds every section (`codes-groups-folded-1280-light`) and opens a
row's menu with a right click (`codes-context-1280-light`). With the debug build's stand-in check
(`LOCKRA_DEV_BIOMETRIC=touch_id`, which always passes) it turns Touch ID on with the switch in
Settings › Security (`settings-biometric-1280-light`), locks, and opens the vault with the unlock
screen's button (`unlock-touch-id-1280-light`): the webview, the shell's commands and the core
for real, only the system's prompt stood in. The showcase shows the ten colours and two marks in
each of the four themes; the site's codes pages show the sections, the colours and the row buttons.

Problems found by looking at them, and fixed:

1. A pinned account showed its star twice, beside the name and on the pin button; the name keeps
   it only where a row has no pin button.
2. The avatar text field was narrower than the dialog's other fields, which folded its hint into
   three lines; it now takes the dialog's width like them.
3. The smoke's first check of the edit dialog read the avatar's text through WebDriver, which
   WebKitWebDriver gives as empty for an `aria-hidden` element: it reads `textContent` and the
   colour instead.
4. The Touch ID switch stood apart from "remember on this device", below a gap like a section of
   its own; it is now the next row of the same list.
5. The fingerprint icon ran together into a blot at the unlock button's 14 px; it is now two
   ridges and the core.

Found after 0.5.0 was released, and fixed:

- The goal review saw that a backup restored by merging, and an account given a new secret through
  an import, came back without its colour and avatar text (`import.rs` carried only the group and
  the pin); both keep them now
  (`a_merged_backup_and_a_replaced_secret_keep_an_accounts_colour_and_mark`).
- The two Touch ID core tests waited for the look at the sensor with a fixed number of yields, which
  a busy machine outran once; they wait for the condition itself, with a deadline (ten full runs of
  the suite in a row passed).

## Touch ID from the start, and several accounts at once (0.6.0)

Asked for on 2026-10-02: on macOS the locked screen offered only the master password, and an
account's group could only be changed one account at a time.

Why Touch ID was missing: its switch appeared only once **Remember on this device** was on, and the
unlock screen said nothing about it until both were on. The keychain (keyring 4.2's v1 mode uses the
macOS Keychain) and robius-authentication's macOS path were read and behave as expected. Now:

- Settings › Security offers **Unlock with Touch ID** (or **Unlock with Windows Hello**) wherever
  the computer has it; turning it on passes one check and turns **Remember on this device** on in
  the same write (`touch_id_turns_on_before_remember_on_this_device_and_brings_it_along`); without a
  keychain it asks for no fingerprint
  (`touch_id_without_a_keychain_asks_for_no_fingerprint_and_leaves_nothing_behind`).
- Locked, a computer with Touch ID that has not turned it on says where the switch is
  (`unlock-touch-id-offer-1280-light`); a vault that asks for Touch ID keeps its button while the
  sensor is away (a lid closed) and says why it cannot open.
- macOS no longer hides Touch ID for the rest of a run after a check found none: a check that
  passes brings it back.

Several accounts: **Select** ticks rows (a click or Space), a section's box (with "some" shown in
between), **Select all** (what the search and the group menu show) or Ctrl/⌘ A; **Move to group…**
moves the ticked accounts with one `entries_set_group` command: one write, a new stamp for each
entry that changes so that sync carries it, and every account put back when the write fails
(`several_accounts_change_group_in_one_write`,
`a_group_set_on_several_accounts_reaches_the_other_devices`). A row's menu starts a selection with
that row ticked; Esc leaves. Screens: `codes-select-1280-light`, `codes-move-1280-light`.

Problems found by looking at them, and fixed:

1. The unlock screen's pointer broke 解锁 across its two lines; the lines are balanced now.
2. A counter-based (HOTP) row kept its "next code" button while selecting; it steps aside with the
   other buttons.

## Not verified here

Touch ID on a real Mac (the prompt's words, the lockout after failures, a lid closed on an external
keyboard without Touch ID); Windows Hello on a real PC (its dialog in front of Lockra, fingerprint,
face and PIN); emoji avatar text with each system's fonts. The manual checks are in
`docs/release.md` ("Manual checks on real devices"); the owner decided on 2026-10-02 to make them
on real devices with the 0.5.0 builds rather than hold the change for them.

Known and accepted (the owner's decision, 2026-10-02): on Windows, robius-authentication looks at
Windows Hello again just before its prompt and asks for the signed-in account's Windows password
when Hello has become unavailable since Lockra's own look an instant earlier (`docs/security.md`).
Avoiding that prompt altogether would take Windows' window-owned interop call, which is `unsafe` in
Rust, or a patched copy of the dependency; neither was wanted.
