# Multi-device sync

What is proven automatically, what was run here, and what has to be checked against real services
before a release is announced. The design is in `docs/security.md` ("Sync") and
`docs/formats.md` (§9).

## Automated

| Layer   | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| sync    | `crates/lockra-sync`: a keyring opens only with its device's master password, the sync key and its own space, and two keyrings under one password do not read alike; a join opens the space with any device's master password and tries at most 32 keyrings; altered, moved and foreign snapshots are unreadable, and a keyring swapped into a header breaks its snapshot; an older snapshot of a device is refused, a new etag at a sequence number already seen is merged; a copied vault writing under the same name is a device clash; a write whose answer was lost is recognised as the device's own, and one that never landed lends no sequence number; a new keyring is written even when nothing else changed; an object over 16 MiB is never read; the merge converges for any order of three replicas and no HOTP counter goes back (property test, deletions and revivals included); the invitation round-trips and nothing else reads as one                                                                                              |
| storage | `make sync-it` (`crates/lockra-remote/tests/live.rs`): the Versity S3 gateway and rclone's WebDAV server in Docker: listing, reading, conditional writes on S3 (an existing object is not replaced, a stale etag loses) and plain writes on WebDAV, deletion, an empty object, an object over the limit refused, wrong credentials refused on S3 (403) and WebDAV (401), and two devices syncing a space through each server, joining with either device's master password and changing something at the same moment (both changes kept, WebDAV included); the same contract on the in-memory store; a closed port is a network failure and a redirect may not leave HTTPS (unit tests)                                                                                                                                                                                                                                                                                                                                                                 |
| core    | `crates/lockra-core/src/tests/sync.rs` on the fake storage and the paused clock: create, join from no vault and into a vault, every failure told apart, the runs (unlock, 3 s after a change, every 5 minutes, none while locked), a failed run shown and recovered, a copied vault renumbered, a new master password reaching the space even after a failed run, backups and the webview's state without the credentials or the sync key, a write cut off by locking recognised on the next run, a restore that replaces reaching the other devices, every device's master password opening the space and one replaced on both devices opening nothing, a vault joining with its own master password and another device's typed in apart, two devices writing at once on a storage without conditional writes losing neither accounts nor keyrings, a run stopped before it writes where the space no longer is, two devices advancing an HOTP counter apart (no code shown twice), new storage settings refused where no snapshot of this space opens |
| bridge  | `crates/lockra-bridge/tests/contract.rs`: the sync commands and views against the fixtures; every command dispatched once on a real core, and the storage's secret and the sync key found in no answer or event but `sync_create` and `sync_invite`                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| web     | `apps/desktop/src/pages/settings/Sync.test.tsx`, `Welcome.test.tsx`: setting up (the sync key shown once, the secret view closed), joining with the sync key (this vault's master password checked, another device's typed in apart) and from an invitation on the welcome screen, sync now, rename, new storage settings, removing a device and the unreadable objects, the invitation hiding itself after two minutes, turning off, a secret answer arriving after Settings closed ending the secret view                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |

## Run here

`make smoke-sync` (`scripts/smoke-sync-linux.sh`), 2026-10-02, Ubuntu 24.04 x64 under Xvfb: two
copies of the real app, each with its own data and settings folders, and the Versity S3 gateway
(versitygw 1.8.0) in Docker on 127.0.0.1. Device A opens Settings › Sync's join form (shot, then
cancelled), fills in the S3 form and starts syncing; device B joins from the welcome screen with
the storage and the sync key; the storage is read back over S3: one object per device and nothing
else, none of them holding an account name, a secret or a device name in clear; B's first
snapshot is fetched and later put back with a signed S3 request.

```text
smoke-sync: A set up the space (1 object, nothing readable) and kept the sync key
smoke-sync: B joined with the sync key, got A's accounts, and renamed, deleted and added one
smoke-sync: A has B's rename, deletion and new account, and lists both devices
smoke-sync: A refused B's older snapshot, kept its accounts and named B in the banner
smoke-sync: passed
```

Screenshots in `screens/sync/`: `sync-off-light`, `sync-join-settings-light` (joining from a vault:
this device's master password and the space's, apart), `sync-create-light`, `sync-key-light`,
`sync-on-light` (device A), `sync-join-welcome-light` (device B's welcome screen),
`sync-on-b-light` (device B), `sync-rolled-back-light` (device A after the rollback).

Problems found by looking at them, and fixed:

1. Settings › Sync's two entry buttons ran across the whole pane; they now take their own width.
2. The sync key's dialog said that a QR code would hide (the export's sentence); it now names the
   sync key, and the invitation names the invitation.
3. The sync key broke inside a group of four; it now breaks only at a dash.
4. Turning sync off was a filled red button beside the invitation; it is now a red text action.
5. Joining from a vault put the optional space password alone on a row under the device name; the
   two passwords now share a row, the device name above them.

## Providers, sealed invitations and timelier runs (0.7.3)

Automated:

- `packages/shared/src/storage-presets.test.ts`: each provider's address from what is typed, with
  its addressing; AWS's China partition kept to its two regions and its own domain, and refused
  under the global one (and the other way round); R2's account endpoint and region `auto`; MinIO
  with path-style requests; saved settings found on their provider only when it would make them
  again. `packages/ui/src/components/StorageFields.test.tsx`: the provider list fills the form,
  suggests regions, and a refusal adds the provider's likely cause.
- `crates/lockra-sync/src/invite.rs`: a sealed invitation opens with its code only (case, spaces,
  dashes and Crockford's look-alike letters do not matter), a wrong code is told from a damaged
  text, hostile Argon2 parameters are refused before any work, and two invitations of one space
  share neither text nor code.
- `crates/lockra-core/src/tests/sync.rs`: a sealed invitation joins with its code, and the
  biometric check that unlocks the vault can show an invitation; a vault joins with its own master
  password and is asked for the space's only when that opens nothing; the device that made the
  space is reminded of the sync key until the key file is saved or the user says it is kept;
  runs come every minute in front and every five minutes behind, one half a minute after the last
  on coming back, five minutes after a failure in front too; what a run brought is told once,
  with the devices it came from, and without them when the write failed afterwards.
- `crates/lockra-bridge/tests/contract.rs` and the fixtures: the sealed invitation's answer, the
  sync key commands and the `sync_brought` notice.
- Web: `apps/desktop/src/pages/settings/Sync.test.tsx` and `apps/mobile/src/sync.test.tsx`: the
  code field for a sealed invitation, the biometric button, the second password only when asked,
  the key file and the reminder; `packages/shared/src/labels.test.ts`: the `sync_brought` text
  names only the counts that are not zero.

To check on real devices:

1. An invitation sent from Windows over a chat app, joined on the phone with its code typed in;
   a wrong code refused, the QR code still scanned without one.
2. Touch ID, Windows Hello and the fingerprint showing an invitation and saving the sync key;
   cancelling the check asks for the master password.
3. The sync key file saved on Windows, macOS and Android through the system's save dialog.
4. On a computer, Lockra in front syncs every minute and behind other windows every five; coming
   back to it syncs; an account added on the phone appears with "Synced from …".
5. The provider forms against the real services below, AWS's China regions with a China account.

## Cloud drive folders (0.7.4)

Automated:

- `crates/lockra-remote/src/folder.rs`: only a space's paths are reached, through folders opened
  one at a time without following a link: a linked directory refused, a linked object no object, a
  link at the temporary name replaced, the folder itself replaced by a link refused, and folders
  swapped for links while a write runs leaving it in the folder it opened; the drive's own files
  (conflicted copies, downloads in progress, `.tmp`) are no objects; an object over 16 MiB is
  listed and never read; a write leaves neither `.prev` nor `.tmp`; a missing folder is reported
  and never made again, and works again once back; a folder this user may not write is refused;
  on Windows, a file the drive holds for a moment is replaced once it lets go.
  `crates/lockra-remote/src/watch.rs`: a snapshot written into the folder is heard through the
  system's file events (native on Linux, Windows and macOS in CI), reads and the drive's own files
  are not, and a missing folder is not watched.
- `crates/lockra-remote/tests/folder.rs`: the sync step through real folders: two devices in one,
  a drive that carries files late, cut short and beside conflicted copies, an older snapshot put
  back (refused, then written again), a copied vault, a folder that went away.
- `crates/lockra-sync`: a folder is an absolute path with no prefix; an invitation of a folder's
  space holds the sync key alone, and one naming a folder is refused; an invitation with its
  storage reads as in 0.7.
- `crates/lockra-core/src/tests/sync.rs`: a space goes into the folder the dialog chose and never
  where the interface says; the invitation holds the key alone, a phone given it is asked for its
  storage before any password and joins over the same drive's WebDAV, another computer chooses
  its own copy of the folder; a folder is looked at every 15 s in front and every minute behind
  and after a failure, a missing one told as such; a space moves into the drive's folder that
  holds it; a change the drive brings runs a sync a second later (the changes within that second,
  one run),
  and the folder is watched only while its space is open here (not while locked, not after the
  space moved or sync was turned off). `entry.rs`: a storage of a kind this version does not know (as 0.7.3 sees a folder)
  leaves sync off and the vault whole.
- `crates/lockra-bridge/tests/contract.rs`: a folder from the webview has no path, and one with a
  path is refused through dispatch. Web: the folder kind on the desktop only, the dialog's folder
  shown, a key-only invitation asking for this device's storage (the desktop's folder, the phone's
  WebDAV), the invitation dialog saying it holds the key alone.
- `apps/desktop/src-tauri/tests/ipc.rs`: two computers on the shell's own storage and one real
  folder: the folder chosen as the dialog does, the key-only invitation, the account crossing both
  ways, the way back by the watch alone (no **Sync now**), ciphertext only in the folder.

To check on real devices:

1. Windows with OneDrive, and a Mac with iCloud Drive, each with a space in the drive's folder:
   a change on one shows on the other about a second after the drive carried it, Lockra in front
   or behind other windows; the folder holds one `.lks` file per device under `lockra-sync-v1/`,
   and nothing else of Lockra's.
2. Jianguoyun (or Nextcloud): the computer in the client's folder, the phone over WebDAV at the
   same folder, joined by scanning the key-only invitation; an account added on the phone reaches
   the computer.
3. The drive's app stopped, the folder renamed: **Settings › Sync** says the folder is missing,
   and nothing is made at the old place; choosing the folder again in **Change storage settings**
   recovers.
4. The same vault file opened with 0.7.3: the accounts are there and sync is off.

## Lockra relay (0.8.1)

Automated:

- `crates/lockra-relay`: a space is made by its first write and bound to its token, another token
  refused everywhere and a missing one asked for; conditions on writes; a listing that waits for a
  change, and every waiting client hears it; every limit (snapshot size, devices, bytes per space
  and in all, spaces), the rates per client (an IPv6 /64 as one), per space and for new spaces; the
  client as the last untrusted `X-Forwarded-For` hop; only device snapshots under canonical space
  ids; a body that fails or never ends refused; what was kept read back at start and an unfinished
  write dropped; idle spaces removed; the connection limit; the settings and their environment.
- `crates/lockra-sync`: a relay is an `https://` address alone (loopback over HTTP); the access
  token is the documented derivation of the sync key (a fixed vector); an invitation to a relay
  carries its address and the sync key.
- `crates/lockra-remote/tests/relay.rs`, on a relay started in the test: the store contract, two
  devices syncing (one snapshot each, no account in clear on the relay's disk), another sync key's
  token denied, an address with no relay or nothing behind it, a listing or a snapshot larger than
  allowed not read, and the watch hearing another device's write and nothing once dropped.
- `crates/lockra-core/src/tests/sync.rs`: a relay space is opened with the space's access and
  watched while unlocked; a phone joins from the desktop's invitation with nothing but its master
  password; a space moves onto a relay that keeps nothing of it yet. Bridge fixtures for the relay
  storage; web: the form starting on the built-in relay, a relay of one's own, a relay space's
  invitation saying it holds the relay's address and the sync key (desktop and phone), the phone
  joining from a relay invitation.

Run here, 2026-10-09:

- `make smoke-sync` with its relay phases: two more copies of the real app and `lockra-relay` on
  127.0.0.1. Device C finds the form on the built-in relay, sets up a space on a relay of its own
  and shows an invitation; the relay's folder holds one snapshot and an `access` file equal to the
  SHA-256 of the token computed apart in Python from the sync key, and none of the accounts, the
  device name, the sync key or the token. Device D joins from the sealed invitation and its code
  alone, and C takes D's new account in.

  ```text
  smoke-sync: C set up a space on its relay (1 snapshot, nothing readable, only the token's SHA-256)
  smoke-sync: C showed a sealed invitation for its relay space and kept it with its code
  smoke-sync: D joined from C's invitation and code alone, got C's accounts, and added one
  smoke-sync: C has D's new account through the relay, and lists both devices
  smoke-sync: passed
  ```

  Screenshots in `screens/sync/`: `sync-relay-create-light` (the built-in relay, nothing to fill
  in), `sync-relay-invite-light` (the relay space's invitation), `sync-relay-join-light` (device
  D's welcome screen), `sync-relay-on-light` (device C with both devices).

- The built-in relay at `https://lockra-relay.onethinker.top` (docs/relay.md):
  `LOCKRA_IT_RELAY_URL=https://lockra-relay.onethinker.top cargo test -p lockra-remote --test relay a_live_relay`
  passed through its load balancer: two devices synced, another token was denied, the watch heard a
  write of the other device, and the test removed its snapshots. The relay's journal holds its start
  and nothing of the requests.

To check on real devices:

1. A computer on the built-in relay and the Android app: scanning the computer's invitation joins
   the phone with nothing typed in but a master password; an account added on either shows on the
   other within seconds while both are unlocked.
2. A vault on a relay opened with 0.8.0: the accounts are there and sync is off.

## Simpler pairing (0.8.3)

The invitation hands the space's data key over, so joining asks only for the joining device's own
master password; the sync key is the recovery key, shown on request only.

Automated:

- `crates/lockra-sync/src/invite.rs`: an invitation carries the data key besides the storage and
  the sync key, plain and sealed; one of 0.8.2 (no data key) reads with none; a data key that is
  not 32 bytes of standard Base64 is no invitation.
- `crates/lockra-core/src/tests/sync.rs`: a vault joins from an invitation under its own master
  password alone (no other device's, though the space's devices use another), and its keyring then
  recovers the space; a new device joins under a password of its own, held to the new-vault rule,
  and its vault opens with it; an invitation without the data key is refused as outdated, one with
  another key as invalid, with no vault made and nothing written; `sync_key_reveal` asks the master
  password or the biometric check, and nothing while locked or without a space. The recovery path
  (storage and sync key) still asks the space's password apart.
- `crates/lockra-bridge/tests/contract.rs`: `sync_create` answers nothing and is no secret view;
  `sync_key_reveal` is one; the dispatch run finds the sync key in no other answer.
- Web: setting up shows no key, the reminder shows it behind the master password and ends with
  "I have kept it"; the invitation shows its QR code, the text to send and its code behind "Can't
  scan?"; joining from an invitation asks no space password, a new vault's password twice; an
  invitation of 0.8.2 says to update its device (desktop and phone).

Run here, 2026-10-09: `make smoke-sync`, the real app on S3 and on `lockra-relay`.

```text
smoke-sync: A set up the space (1 object, nothing readable) and kept the recovery key it showed
smoke-sync: B recovered with the recovery key, got A's accounts, and renamed, deleted and added one
smoke-sync: A has B's rename, deletion and new account, and lists both devices
smoke-sync: A refused B's older snapshot, kept its accounts and named B in the banner
smoke-sync: C set up a space on its relay (1 snapshot, nothing readable, only the token's SHA-256)
smoke-sync: C showed a sealed invitation for its relay space and kept it with its code
smoke-sync: D joined from C's invitation and code under its own password, got C's accounts, and added one
smoke-sync: C has D's new account through the relay, and lists both devices
smoke-sync: passed
```

Device C's invitation went to the X clipboard through **Copy the invitation** and was read back
with `xclip`; the text never showed on screen. Screenshots: `sync-key-light` (the recovery key's
dialog over its reminder), `sync-relay-invite-light` (the QR code, "Can't scan?" open),
`sync-relay-join-light` (device D choosing its own password, typed twice).

To check on real devices:

1. A phone joins a computer's space by scanning, under a master password of its own; both see each
   other's new accounts.
2. A 0.8.2 device shows an invitation to the new version: refused with the word to update it.

## Pairing links for relay spaces (0.8.4)

A space on a relay copies its invitation as it is, the pairing link: no sealed text and no code.
The user chose a link that opens alone and does not expire; other storage keeps the sealed text
and its code.

Automated:

- `crates/lockra-core/src/tests/sync.rs`: a relay space's invitation has nothing sealed; its own
  link goes to the clipboard (excluded from history, cleared after the clipboard time), a note or
  another space's link does not; a computer joins from the pasted link with no code. On S3 the
  plain invitation is still refused and the sealed text copied.
- `crates/lockra-bridge/tests/contract.rs` and the fixtures: `sync_invite` with a sealed text and
  its code, and a relay space's with none.
- Web: the relay invitation shows the QR code, **Copy the pairing link** and the warning that the
  link does not expire, and no "Can't scan?" or code (desktop and phone); S3's still has them;
  the join form's field is "Pairing link or invitation".

Run here, 2026-10-09: `make smoke-sync`, the real app on S3 and on `lockra-relay`.

```text
smoke-sync: C set up a space on its relay (1 snapshot, nothing readable, only the token's SHA-256)
smoke-sync: C copied the pairing link of its relay space (no code) and kept it
smoke-sync: D joined from C's pairing link under its own password, got C's accounts, and added one
smoke-sync: C has D's new account through the relay, and lists both devices
smoke-sync: passed
```

Device C's pairing link went to the X clipboard through **Copy the pairing link** and was read back
with `xclip` as `lockra-invite:1:`; the link never showed on screen, and neither "Can't scan?" nor
a code did. Device D pasted it with no code field. Screenshots: `sync-relay-invite-light` (the QR
code beside **Copy the pairing link**, the warning that the link does not expire),
`sync-relay-join-light` (device D's welcome screen with the pairing link pasted).

To check on real devices:

1. The Android app on the built-in relay copies its pairing link; a computer pastes it into
   **Join an existing sync** with nothing else but its own master password.
2. A 0.8.3 computer pastes the pairing link of the new version and joins.

## Joining by the fingerprint on the phone (0.8.6)

Asked for on 2026-10-10: the phone should take the fingerprint instead of the master password,
joining a space and inviting a device among the cases. Inviting and the recovery key took it
already (0.7.3); joining could not, because a joining device seals its own keyring under its
master password, and the fingerprint gives none.

A join by the biometric check that unlocks an existing vault seals this device's keyring under a
random password kept nowhere, at the usual Argon2id cost (the storage cannot tell it apart from
any other keyring), and records `keyring_unsealed`. The next unlock with the master password seals
the keyring under it in the background and marks it to be written, as after a new password;
`change_password` does the same. A recovery without a password needs the space's, and a new
vault, and setting up a space, still take the password.

- Core: `a_vault_joins_from_an_invitation_by_the_biometric_check_and_seals_its_keyring_at_the_next_password`
  (the phone's password recovers nothing from the storage until the next unlock with it, and does
  after), `the_storage_settings_move_on_after_the_biometric_check_too`; the earlier joins with a
  password behave as before.
- Bridge: `sync_join`, `sync_set_storage` and `export_start` with an optional password and a
  reason; `keyring_unsealed` in the space's view.
- Phone (`apps/mobile/src/sync.test.tsx`): joining from the camera with the password left empty
  (the hint, the prompt's words, Settings › Sync's note until a password unlock, a fingerprint
  unlock leaving it), from a pasted invitation (no `password` sent), the recovery key mode keeping
  the password; the invite and recovery key pages asking by themselves where the fingerprint is
  the default unlock, waiting for the button where the password is, silent after a cancel, and
  saying why when the fingerprint cannot be used; the storage settings changed with it.

To check on real devices: on a phone with the fingerprint on, join a desktop's space by scanning
with the password left empty; Settings › Sync shows the note; lock, unlock with the master
password, and after the next sync recover the space on a new install with that phone's password.

## Not verified here

Real services: AWS S3, Cloudflare R2, Backblaze B2, Alibaba Cloud OSS, MinIO, Nextcloud,
Jianguoyun and a Synology NAS (the address forms, path-style or virtual-host addressing, and each
service's conditional-write support); a system proxy; the invitation's QR code scanned by another
device (the Android app is planned); screen-capture exclusion of the sync key and invitation views
on Windows and macOS (the same switch as the reveal view, checked there).
