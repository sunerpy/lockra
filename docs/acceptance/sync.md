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

## Not verified here

Real services: AWS S3, Cloudflare R2, Backblaze B2, Alibaba Cloud OSS, MinIO, Nextcloud,
Jianguoyun and a Synology NAS (the address forms, path-style or virtual-host addressing, and each
service's conditional-write support); a system proxy; the invitation's QR code scanned by another
device (the Android app is planned); screen-capture exclusion of the sync key and invitation views
on Windows and macOS (the same switch as the reveal view, checked there).
