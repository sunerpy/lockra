# Lockra file and transfer formats

What Lockra writes and reads. The code is the authority: `crates/lockra-vault` (container),
`crates/lockra-core` (entries, settings, backups), `crates/lockra-transfer` (Google, Microsoft,
otpauth lists, QR codes, other apps' logins), `crates/lockra-otp` (codes and URIs),
`crates/lockra-sync` (the sync objects).

## 1. The container (vault and backups)

One layout for the vault and for every backup:

```
magic (8 bytes) | header length (u32, little-endian) | header (JSON, ≤ 64 KiB) | payload (ciphertext)
```

| Field           | Value                                                                                                                                                               |
| --------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| magic           | `LKRAVLT1` (vault) or `LKRABAK1` (backup); the header repeats it as `kind` and the two must agree                                                                   |
| `format`        | `1`; a larger number is `UnsupportedVersion`                                                                                                                        |
| `kind`          | `vault` or `backup`                                                                                                                                                 |
| `vault_id`      | UUID of the vault the file belongs to (a backup keeps its vault's id)                                                                                               |
| `created_at_ms` | when the vault was created                                                                                                                                          |
| `kdf`           | `{algorithm: "argon2id", m_kib, t, p, salt}` — default 64 MiB, 3 passes, 1 lane, 16-byte salt; a file asking for more than 256 MiB, 16 passes or 8 lanes is refused |
| `slots`         | the data key wrapped once per way in: `{kind: "password" \| "device", nonce, wrapped_dek, check?}`                                                                  |
| `payload_nonce` | 24 bytes                                                                                                                                                            |

Binary fields are Base64. Cryptography:

- The **payload** (the entries, JSON) is encrypted with XChaCha20-Poly1305 under a random 32-byte
  data key (DEK). Its associated data is **every byte before it**: the magic, the length and the
  whole header, slots included. Changing any header byte, including a slot, fails authentication.
- A **password slot** wraps the DEK with a key derived from the password by Argon2id (the header's
  `kdf`). A **device slot** wraps it with a random 32-byte key kept in the OS keychain
  (service `dev.lockra.desktop`, account = `vault_id`). A slot's associated data is
  `lockra/slot/v1 | vault_id (16 bytes) | kind (1 = password, 2 = device)`.
- A device slot's `check` is what the device confirms before its key is used: `"biometric"` (Touch
  ID, Windows Hello) or absent. It sits in the header, so a file whose check was taken out no longer
  opens with any key; a value this version does not know is kept as it is, and the device slot is
  then not used (the master password opens the vault).
- Every write rewrites the whole file: a fresh payload nonce, the current header as associated
  data. There is no in-place slot edit.

Slot operations:

| Operation                             | DEK         | Slots afterwards                                                                                                                      |
| ------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| create                                | new         | password                                                                                                                              |
| remember on this device               | kept        | password + device (new device key into the keychain)                                                                                  |
| stop remembering (needs the password) | **rotated** | password; the keychain entry is deleted                                                                                               |
| change the master password            | **rotated** | password (new salt) + device, if the keychain still returns the device key; otherwise the device slot is dropped and the user is told |
| ask for Touch ID / Windows Hello      | kept        | the device slot gains `check: "biometric"` after one check passes; turning it off (needs the password) removes it                     |

Errors: a password slot that does not open is `WrongPassword`; a payload that fails
authentication, a truncated file or a malformed header is `Corrupted`; a future `format` is
`UnsupportedVersion`; a foreign magic is `NotLockra`.

Writes are atomic: `vault.lockra.tmp` is written and synced, the current file is kept as
`vault.lockra.prev`, the new one is renamed into place, and the directory is synced (Unix). Files
are created `0600` on Unix.

## 2. The vault

`vault.lockra` in the app data directory (`~/.local/share/dev.lockra.desktop`,
`%APPDATA%\dev.lockra.desktop`, `~/Library/Application Support/dev.lockra.desktop`). Its payload
(JSON) is `{format, entries, tombstones, items, item_tombstones, site_rules, site_rule_tombstones,
local}`. The format is worked out from the content when the file is written: **3** once it holds a
login, a site rule, a deletion of either, or a generated password, **2** otherwise, each of the
four new arrays left out while empty. A vault that never held a login is therefore written as
before and still opens in Lockra 0.9, which refuses format 3 (`VaultUnsupported`) and leaves the
file as it is, rather than dropping what it cannot read. Format 1 (before 0.4.0) had the entries
only, and reads with every entry stamped at its `updated_at_ms`; a format above 3 is refused. The
entries:

| Field                                               |                                                                          |
| --------------------------------------------------- | ------------------------------------------------------------------------ |
| `id`                                                | UUID                                                                     |
| `kind`                                              | `{type: "totp", period}` (1–3600 s) or `{type: "hotp", counter}`         |
| `algorithm`                                         | `sha1`, `sha256`, `sha512`                                               |
| `digits`                                            | 6–8                                                                      |
| `secret`                                            | bytes (never sent to the webview except by an explicit reveal or export) |
| `issuer`, `account`, `group`, `favorite`            |                                                                          |
| `origin`                                            | `manual`, `uri`, `google`, `microsoft`, `backup` (see below)             |
| `created_at_ms`, `updated_at_ms`, `last_used_at_ms` | `last_used_at_ms` stays on this device: it does not sync                 |
| `stamp`                                             | `{wall_ms, counter, device}`: when it last changed (§9)                  |
| `color`, `mark`                                     | the avatar's colour and up to two characters; absent: from the name      |

`color` is one of `red orange amber green teal blue indigo purple pink gray`; absent (`auto`), the
avatar takes a colour from a hash of the name, the same on every device. A colour this version does
not know reads as `auto`, so neither the vault nor another device's snapshot fails over one. `mark`
holds at most two characters as people count them (an emoji with its joiners is one). An `origin`
this version does not know is kept as written and shown as `uri`; none is added while devices of
0.9, which fail on one, are supported (an account an import of logins makes is `uri`, the login
names its source).

`tombstones` lists deleted accounts as `{id, stamp, counter?}` (an HOTP account's counter at its
deletion), so that a deletion reaches the other devices of a sync space; `item_tombstones` and
`site_rule_tombstones` do the same for logins and site rules (`{id, stamp}`). Replacing an account's
secret through an import gives it a new id (the old id is tombstoned), so its HOTP counter starts
again with the new secret. `local` is this device's own part and never leaves the vault file (no
backup, no sync): `clock` (its device number and the latest stamp), `view` (the groups folded in
the code list, `{collapsed_groups}`, "" for the accounts in no group, the order the accounts
and the groups were dragged into on this device, `{entry_order}` (ids) and `{group_order}`
(names, "" for the accounts in no group, which move like a group since 0.8.9), and the group
pinned at the top, `{pinned_group}`: `{"type": "favorites"}`, `{"type": "group", "name"}` or
`{"type": "ungrouped"}`, a kind this version cannot read being no pin; each is left out while
empty: group names stay inside the encrypted file, never in `settings.json`), `generator_history`
(the passwords the generator made here, `{password, at_ms, site?}`: secrets, in the vault file
only) and `item_view` (the groups folded in the password list, `{collapsed_groups}`; development
builds of 0.10 also wrote an `identity` there, which is dropped). Every part but `clock` and
`sync` reads leniently: one that does not read (a later version's shape) is reset alone and the
vault opens. With sync on, `local` also holds `sync` (the storage settings and credentials, the space id, its
data key, the sync key, this device's name, its keyring and what the runs remember, among it
`keyring_unsealed`: the keyring was sealed under random bytes by a join with the biometric check,
and waits for the next unlock with the master password). A `sync` this version cannot read (one kept by an earlier build, or by a later
Lockra: to 0.7.3, a space in a cloud drive's folder, `{kind: "folder", path}`; to 0.8.0, a space on
a relay, `{kind: "relay", url}`) is left out and the vault opens without it: sync is off on that
device until it is set up again. Everything else stays: the accounts, the logins, the site rules
and every other part of `local` that reads.

### Logins

`items` holds the logins. Each merges part by part (§9): its **metadata** is one register, each
**field** another, and every register is ordered by `(stamp, write id, content)`, so that two
replicas pick the same version whatever they saw first.

| Field                                        |                                                                                                                    |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| `id`                                         | UUID                                                                                                               |
| `kind`                                       | `login`; a kind of a later Lockra is kept as written and the item is read-only                                     |
| `title`, `notes`                             | `notes` is treated as a secret (only shown on request), at most 64 KiB                                             |
| `fields`                                     | user name, email, password, custom fields: at most 64, below                                                       |
| `urls`                                       | `{url, matching}`, at most 32; `matching` is `domain` (absent), `host`, `starts_with`, `regex`, `exact` or `never` |
| `tags`, `group`, `favorite`, `color`, `mark` | as an account's                                                                                                    |
| `protection`                                 | `fill` (absent), `confirm` (ask before showing, copying, filling), `no_extension`                                  |
| `autofill`                                   | `{on_page_load, auto_submit, fill_otp}`, each `default` (absent), `on` or `off`                                    |
| `totp_entry`                                 | the id of the account whose codes go with it                                                                       |
| `history`                                    | hidden values its fields held before, `{field, value, set_at, wid}`, newest first, at most 20                      |
| `source`                                     | where an imported login came from (`bitwarden`, `kdbx`, …)                                                         |
| `created_at_ms`, `updated_at_ms`             | `updated_at_ms` is the last change of any part (merged by its maximum)                                             |
| `last_used_at_ms`                            | stays on this device: it does not sync                                                                             |
| `stamp`, `mwid`                              | the metadata register: when it was written and by which write (a random 64-bit number)                             |

A field is `{id, purpose, label?, value, hidden?, page_field?, removed?, created, stamp, wid,
seen?}`: `purpose` is `username`, `email`, `password` or `custom`; `page_field` is the page's field
a custom field fills; a removed field keeps its register (`removed: true`, empty value) so that the
removal syncs; `created` never changes and, with the purpose and the id, orders the fields (user
name, email, password, then custom fields as added, on every replica alike). `wid` names the write
that put the content there: a write of new content (an edit, an update from the browser, a
replacing import, settling a conflict) takes a fresh random one, a value taken back (a restore)
keeps its own. `seen` (sorted) lists the writes of that field the author of the content had seen:
the value it replaced, and what that one had seen, trimmed when written to the writes still in the
history.

- **The history.** A write of a hidden field puts the value it replaces into the history, unless
  it is the same write taken back; so does a merge with the version that loses. The history keeps
  one entry per `(field, wid)`, the latest `set_at`; none for a value a field holds now; the 20
  newest by `(set_at, wid)`. Every step gives the same result on every replica.
- **Conflicts** are derived, not stored: a field is in conflict when its history holds a value
  whose write is neither the current one nor in its `seen`, and which differs from the current
  value: two changes made without seeing each other, whatever the clocks or the device numbers
  said. Settling it (keeping the current value or taking the other) is a new write that has seen
  every write the history holds, and clears it on every replica once it syncs. A conflicting value
  pushed out of the 20 takes the conflict with it.
- **The same write on both sides** (two copies of a vault taking the same value back at the same
  moment) differs at most in `seen`: a merge takes the union and trims nothing, so `seen` can hold
  more than 20 writes for a while; the next write of that field on this device trims it.
- **Read leniently.** A `kind`, `purpose`, `matching`, `protection` or `autofill` value of a later
  Lockra is kept as written and written back unchanged: an unknown `matching` matches nothing and
  an unknown `protection` gives the browser extension nothing (rather not fill than fill where the
  user did not mean). Keys a later Lockra adds to a login, a field, a URL or `autofill` are kept as
  well.

`site_rules` holds `{id, kind, domains, stamp}`: `kind` is `never_save`, `never_fill`,
`equivalent` (domains that share their sign-ins), `match_host` or `match_domain`; one of a later
kind says nothing. A rule merges whole, the later version winning. A rule about one domain has the
id made from SHA-256 of the kind, a zero byte and the domain in lower case (its first 16 bytes as a
version 8 UUID), so that two devices adding the same rule end up with one.

`match_host` and `match_domain` hold one domain each (a host name, without scheme, path or port):
pages of that domain and its subdomains match by their full host, or by the site, whatever the
policy's `match_host` says (§10). Adding one takes the other kind's rule about the same host away
in the same change, tombstoned so that the removal syncs: a domain matches one way. Of the rules
whose domain is the page's host or a parent of it, the longest domain's decides; one domain under
both kinds, which two devices may leave until they sync, goes by the later stamp (the full host
when they tie). A Lockra without these kinds keeps them as written and applies neither.

Duplicates: the same secret and parameters is **the same account** (an import skips it); the same
issuer and account with a different secret is a **conflict** (both are kept by default, or the
import replaces the existing one). Duplicates inside one import are collapsed first.

A forgotten password: _reset_ renames the vault to `vault.lockra.reset-<ms>` (and `.prev` along
with it) and starts over; nothing is deleted.

## 3. Settings

`settings.json` in the app config directory, plain JSON without secrets: theme, follow the system
theme, accent, density, font size (12–18 px), reduce motion, locale (`system`, `zh-cn`, `en`),
auto-lock minutes (0 = never; default 5), clipboard clearing seconds (0 = never; default 30), hide
codes, code order (`name`, `added`, `recent`, `manual`: the order of the local part's
`view.entry_order`, accounts never dragged after it, oldest first), automatic backup `{enabled, dir, keep}` (keep
3–50, default 10) and automatic updates (`auto_update`, default off). From 0.3.2 the file also carries `schema: 2`;
a file without it (0.2.0 to 0.3.1) has `auto_update` read as off, because 0.3.0 could have carried
0.2.0's check-only `auto_check_updates` over into it, and 0.2.0's field itself is not read. Unknown or missing fields take their defaults.

The quick search's global shortcut is `quick_access_shortcut`: an accelerator in the syntax the
desktop shell registers (tauri-plugin-global-shortcut), modifiers then one key joined by `+`, the
key named by its physical position (`KeyboardEvent.code`: `KeyK`, `Digit1`, `Space`), for instance
`CommandOrControl+Shift+Space` (⌘ on a Mac, Ctrl elsewhere) or `Control+Alt+KeyK`. `null`, the
default, registers none; a blank value reads as `null`. Only a build that offers password
management registers it (docs/architecture.md, "The quick search"); whether the system took it is
not kept in the file.

`update-ready.json` in the data directory, next to the vault: `{"version": "0.3.0"}`, the release
the automatic update downloaded and has not installed yet; the next start installs that version
at once. It is removed when a check finds nothing newer or the update is installed, and a file
that does not parse counts as none.

## 4. Backups

A `*.lockrabackup` file is the container with magic `LKRABAK1` and one password slot.

- **Under the master password** (automatic backups, the copy before a restore, the default manual
  backup): the vault's password slot is copied as it is (same salt, cost and wrapped DEK), and the
  payload is sealed again under the backup's own header. Writing one needs no password, so it
  works however the vault was unlocked; opening it needs the master password **of that moment**.
  After a password change the DEK rotates, so earlier backups keep opening with the old password.
- **Under a separate backup password** (manual backup, by choice): a new DEK and salt.

A backup's payload is `{format, entries, tombstones, items, item_tombstones, site_rules,
site_rule_tombstones}`, of format 3 when it holds a login, a site rule or a deletion of either, 2
otherwise: the vault's `local` part (the sync space and its credentials, the generated passwords)
is left out, so a restored backup is a new device that joins its space again. Lockra 0.9 refuses a
backup of format 3; one it made holds no logins.

Automatic backups go to the chosen folder 3 seconds after the last change, named
`lockra-auto-YYYYMMDD-HHMMSS.lockrabackup` (UTC; `-2`, `-3`, … on a clash). Beyond `keep`, the
oldest files **of that pattern** are deleted; nothing else in the folder is touched. A folder that
cannot be written is reported in the app and tried again at the next change.

Restoring into a vault: _merge_ puts the backup's accounts and logins into the import preview
(§11, the logins linked to their codes through the map of the accounts they end as) and adds its
site rules quietly (the later version of each winning, the backup's deletions left out). Where
password management is off, a backup with logins or deletions of logins is refused
(`feature_unavailable`) rather than restoring its codes alone, and stays open for a _replace_; so
is opening such a backup in the import. _Replace_ first
writes the current vault as `pre-restore-YYYYMMDD-HHMMSS.lockrabackup` in the data directory.
Restoring on the welcome screen makes the backup the vault and its password the master password. A
vault file (`.lockra`) can be restored the same way. With sync on, _replace_ is a change made at
that moment: the backup's accounts are stamped anew and the accounts that go are tombstoned, so the
other devices end up with the backup's accounts too. A backup of format 3 replaces the logins and
the site rules the same way: the backup's deletions are kept, the logins and rules that go are
tombstoned, and each restored login takes its backup version back, metadata and every field
stamped anew after everything the space holds, their write ids kept; the values they replace go
into the history and count as seen, and a field the backup does not have is removed by a write of
its own. An older snapshot of another device then changes nothing back and raises no conflict. A
backup of format 2 leaves the logins and the site rules as they are.

## 5. otpauth URIs

Google's Key Uri Format, `otpauth://totp|hotp/<issuer>:<account>?secret=…&issuer=…`.

- **Read leniently**: scheme and parameter names in any case, `+` as a space, the separator
  literal or `%3A`, the `issuer` parameter before the label's prefix, defaults SHA1 / 6 digits /
  30 s / counter 0, Base32 without padding, in either case, with spaces or dashes.
- **Written strictly**: a literal separator, everything outside RFC 3986's unreserved set
  percent-encoded, and `algorithm`, `digits` and `period` (or `counter`) always spelled out, so the
  URI reads back to exactly the same account.
- A `.txt` list holds one URI per line; blank lines and lines starting with `#` are skipped; every
  other line that is not a URI shows in the preview as unsupported, with its line number.

## 6. Google Authenticator

Export screen: _Transfer accounts → Export accounts_; one or more QR codes of
`otpauth-migration://offline?data=<percent-encoded Base64 of a protobuf>`.

```
MigrationPayload { repeated OtpParameters otp_parameters = 1; int32 version = 2;
                   int32 batch_size = 3; int32 batch_index = 4; int32 batch_id = 5; }
OtpParameters    { bytes secret = 1; string name = 2; string issuer = 3;
                   Algorithm algorithm = 4;   // 1 SHA1, 2 SHA256, 3 SHA512, 4 MD5
                   DigitCount digits = 5;     // 1 six, 2 eight
                   OtpType type = 6;          // 1 HOTP, 2 TOTP
                   int64 counter = 7; }
```

- **Import**: standard and URL-safe Base64, with or without padding; an empty issuer is taken from
  a `issuer:account` name; MD5 and unknown types are listed as unsupported. Codes of one batch are
  tracked by `batch_id`, and the preview says which are still missing.
- **Export**: only 30-second TOTP and HOTP with 6 or 8 digits fit (there is no period field).
  At most 10 accounts per code, and fewer while the code would exceed version 20 at error
  correction M; `version = 1`, a random positive `batch_id`. Accounts that do not fit are listed
  with the reason.
- Android's export screen blocks screenshots: photograph it with another device (or screenshot on
  iOS) and import the photo.

## 7. Microsoft Authenticator

No export exists. The only bulk source is the app's database on a **rooted** Android phone:
`/data/data/com.azure.authenticator/databases/PhoneFactor` together with `PhoneFactor-wal` (the
database runs in WAL mode; recent accounts live in the WAL until a checkpoint; `-shm` is not
needed). Lockra recognises the two files by their headers, copies them into a private temporary
directory, opens the copy (the originals are never written), and reads
`accounts(name, username, oath_secret_key, account_type[, encrypted_oath_secret_key])`:

| `account_type` | Meaning                    | Read as                             |
| -------------- | -------------------------- | ----------------------------------- |
| 0              | third-party TOTP           | Base32 secret, 6 digits, SHA1, 30 s |
| 1              | personal Microsoft account | Base64 secret, 8 digits, SHA1, 30 s |
| other          | work or school (Entra)     | unsupported: no exportable secret   |

An empty `oath_secret_key` beside an `encrypted_oath_secret_key` is reported as encrypted (newer
app versions); a secret that does not decode is reported as invalid. **Export** to Microsoft
Authenticator is one standard `otpauth://totp` QR code per account, SHA1 / 6 digits / 30 s only
(the app ignores other parameters).

## 8. QR codes

Encoded by the core as SVG: black modules on a white background, a 4-module quiet zone; the
webview shows them through an `<img>` data URL, so nothing in the SVG can run. Decoded with rxing
from PNG, JPEG and WebP images (several codes per image, photos at an angle); a picked file's type
is judged from its first bytes, not its name: Lockra magic, `SQLite format 3`, an image, or text.

A login's form adds one account on the spot to link it (board 07-c): `entry_add_clipboard` reads
the clipboard (an image's QR codes, else text with an `otpauth` link) and the desktop shell's
`entry_add_image` an image picked in its open dialog. What they hold is read as the import reads it
(Google Authenticator export codes included), but nothing goes to the preview: exactly one account
the vault can take is added at once and its id answered; none is `import_empty` (`clipboard_empty`
for an empty clipboard), several are `several_accounts`, to go through the import instead.

## 9. Sync

A sync space lives under the prefix the user chose on their storage (an S3-compatible bucket or a
WebDAV folder), in a folder of the computer that a cloud drive's client keeps in sync (the folder
is the place, no prefix), or on a Lockra relay, which keeps each space by its id ("The relay"
below): one snapshot per device, and nothing else.

```
<prefix>/lockra-sync-v1/<space id>/devices/<device tag>.lks
<folder>/lockra-sync-v1/<space id>/devices/<device tag>.lks
<relay>/v1/spaces/<space id>/devices/<device tag>.lks
```

The same space can be reached both ways: a computer through its cloud drive's folder, a phone
through the same drive's WebDAV, its prefix the folder's path in the drive. In a folder, a write
replaces the file atomically (a temporary `<name>.tmp` renamed over it) and keeps no copy beside
it; files of the drive's own (conflicted copies, downloads in progress) are no object of the space
and are not read. The folder's path is kept with every link on its way resolved when it is
chosen; every read and write then opens the folders one at a time from the top of the file system
without following a link, and acts on names inside the last one only. A link anywhere on the way
is refused, a linked object is no object, a link at the temporary name is replaced, and a folder
swapped for a link while a write runs does not move it: nothing outside the folder is reached. The etag of a file is the SHA-256 of its bytes. While the folder itself is
missing (moved, deleted, its drive not connected) a run neither reads nor writes, and the folder is
not made again.

Every object is framed like the container: `magic (8) | header length (u32 LE) | header (JSON, ≤
16 KiB) | ciphertext`, the ciphertext authenticated with every byte before it as associated data.

| Object   | Magic      | Header                                       | Ciphertext                                                                                                                     |
| -------- | ---------- | -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| keyring  | `LKSKEYR1` | `{format: 1, space_id, kdf, nonce}`          | the space's 32-byte data key                                                                                                   |
| snapshot | `LKSDEVS1` | `{format: 1, space_id, tag, nonce, keyring}` | `seq`, `written_at_ms`, the device name, from 0.10 `app_version` and `reader`, the payload; padded to a multiple of 4096 bytes |

- **What a snapshot says of its device.** Since 0.10 the sealed metadata beside the name carries
  `app_version` (the writing app's version, at most 32 characters) and `reader` (what of a payload
  that version reads: 2 since the logins, `Replica::reader`). Absent, they are not written, so a
  snapshot of 0.9 is the same object as before, and 0.9 ignores them. The others keep them with
  the device (`seen`, the same way) and Settings › Sync marks a device below reader 2 as one that
  keeps no passwords. Both count in the digest that decides whether a snapshot is written again
  (absent, the digest is the one 0.9 kept), so a device writes once after an update and the others
  learn its version.
- **One writer per object.** A device writes only its own snapshot. Another device only deletes
  one (removing a device), and a device still in use writes its snapshot again. No object is
  shared: what the space holds is the merge of the snapshots, so no lock and no conditional write
  is needed, and S3 and WebDAV behave alike.
- **Keyrings.** A snapshot's `keyring` (Base64) is a keyring object, the device's own: the data
  key wrapped under HKDF-SHA256(salt = space id, ikm = Argon2id(that device's master password,
  `kdf`) ‖ sync key, info `lockra-sync v1 keyring`). It is sealed when the device creates or joins
  the space and again under a new master password, each time with a fresh salt and nonce; its
  header names no device and no time. A join by the biometric check seals it under a random
  password kept nowhere, at the same cost, and the next unlock with the master password seals it
  again under that password. A keyring asking for more than the container's KDF limits is
  refused before any work. A device joining from an invitation takes the data key it carries,
  once one of the space's snapshots opens under it, and seals its own keyring under its own
  master password. A recovery (the storage settings and the sync key typed in) reads the space's
  snapshots (at most 32) and tries their keyrings with the password typed in; the first that opens
  gives the data key.
- **Snapshots** are encrypted under HKDF(salt = space id, ikm = data key, info
  `lockra-sync v1 snapshot`), XChaCha20-Poly1305.
- **Names.** The space id is derived from the sync key (HKDF, info `lockra-sync v1 space id`, as a
  version-4 UUID). A device tag is the first 16 bytes, in hex, of HMAC-SHA256 under
  HKDF(data key, info `lockra-sync v1 device tag`) of the device's random 64-bit number.
- **Payload.** `{format: 1, entries, tombstones, items, item_tombstones, site_rules,
site_rule_tombstones}` as in the vault (§2), without `last_used_at_ms`, each of the last four
  left out while empty. The format stays 1: Lockra 0.9 reads a snapshot of another format as
  unreadable, but ignores what it does not know in one of its own, so it takes the accounts of a
  snapshot with logins and leaves the logins. What a version reads of a payload is a number of its
  own (`reader` in the state the vault keeps: 1 for 0.9, 2 since logins), and a run whose state
  says less reads every snapshot once more, whatever its etag, keeping the sequence numbers seen
  for rollbacks: a device of 0.9 that took a snapshot in and then updated gets its logins even if
  the device that wrote it never writes again. A device writes its snapshot when its device name, keyring or payload
  changed, under a sequence number no write of it had before: one higher than the last written or
  attempted. S3 and relay writes carry `If-None-Match: *` or `If-Match: <etag>`, which catch a
  copied vault writing under the same name sooner. A snapshot under a new etag is read and merged even at a
  sequence number already seen; one under a lower number than seen is refused. An object larger
  than 16 MiB is never read (`MAX_OBJECT_BYTES`): it is reported as unreadable. An object is read
  as a stream, and no further than the size the storage gave for it.
- **Merge.** Last writer wins per account, on the stamps: a hybrid logical clock
  `(wall_ms, counter, device)` that follows wall time, never goes back on a device and comes after
  every stamp the device has seen. A tombstone at or after an account's stamp removes it; a change
  after the deletion brings it back. An HOTP counter is the exception to "the whole later version
  wins": an account and its tombstone carry the highest counter any of their versions reached, so
  a code once shown never comes back on any device. A login merges part by part (§2, "Logins"):
  the later metadata register, the later version of each field (the loser's hidden value into the
  history), the histories joined. A tombstone removes a login whose every part is stamped at or
  before it; a part written after the deletion brings the login back with what came after it
  only, the parts the deletion covers (metadata, fields, history entries) being dropped on every
  replica, whichever arrived first, the deletion or an older version merged elsewhere. Site rules
  merge whole, as accounts do. Merging is commutative, associative and idempotent.
- **The sync key** is `LKS1-` and 14 groups of four Base32 characters: the 32-byte key and three
  bytes of its SHA-256, so a mistyped character is caught. Case, spaces and dashes do not matter.
  The interface calls it the recovery key: it is shown only on `sync_key_reveal`, after the
  master password or the biometric check, and no invitation shows it.
- **An invitation** is `lockra-invite:1:` and Base64url (no padding) of
  `{storage, sync_key, data_key}`: the storage settings with their credentials (a relay's are its
  address, `{kind: "relay", url}`), the sync key text, and the space's data key (standard Base64
  of its 32 bytes). It hands the space over: the joining device needs no other device's master
  password. The QR code carries it so, and so does the pairing link of a space on a relay, which
  is this text, copied and sent as it is. A space in a folder invites with `{sync_key, data_key}`
  alone: a folder is of no use on another device, and its path is never taken from an invitation
  (one naming a folder is refused); the joining device says how it reaches the space. An
  invitation of Lockra up to 0.8.2 carries no `data_key`; Lockra refuses it
  (`sync_invite_outdated`), and Lockra up to 0.8.2 reads no invitation with one. Lockra up to
  0.7.3 reads only invitations with their storage.
- **A sealed invitation**, the text to send for a space not on a relay, is `lockra-invite:2:` and
  Base64url (no padding) of an
  object framed like the others: magic `LKSINVT2`, the header `{format: 2, kdf, nonce}`, then
  XChaCha20-Poly1305 of the plain invitation's JSON, the header bytes as associated data. The key
  is HKDF-SHA256(Argon2id(code, salt), info `lockra-sync v1 shared invitation`) at the vault's
  cost, with the usual bounds on the parameters. The code is ten characters of Crockford's Base32
  (50 bits, shown as `ABCDE-FGHJK`); case, spaces and dashes do not matter, and O reads as 0, I
  and L as 1.

### The relay

A relay (`docs/relay.md`) keeps a space's snapshots under its id, at an HTTPS address of its own:

```
GET    <relay>/v1/spaces/<space id>/devices/                    the listing
GET    <relay>/v1/spaces/<space id>/devices/<device tag>.lks    a snapshot
PUT    <relay>/v1/spaces/<space id>/devices/<device tag>.lks
DELETE <relay>/v1/spaces/<space id>/devices/<device tag>.lks
GET    <relay>/healthz                                          ok
GET    <relay>/v1/                                              {service: "lockra-relay", api: 1, version}
```

- **Access.** Every request for a space carries `Authorization: Bearer <token>`, the space's access
  token: Base64url (no padding) of the 32 bytes of HKDF-SHA256(salt = the space id's 16 bytes,
  ikm = sync key, info `lockra-relay v1 access token`). The relay keeps the SHA-256 of those 32
  bytes (hex, in the space's `access` file); a space's first write binds it. Another token is refused (403), a
  missing or malformed one asks for one (401). Nothing else of the sync key reaches the relay.
- **Registration.** A closed relay serves only its listed spaces (403 `forbidden` for others). A
  relay with code registration answers a space it does not know with 403 `registration_required`,
  unless the request carries `Lockra-Registration: <code>`: a registration code, 15 random bytes
  shown as 24 letters of RFC 4648 Base32 in six groups of four (`ABCD-EFGH-…`; case, spaces and
  dashes do not matter). A code the relay made, not used and not expired, admits the space with the
  request's token (the request is then served); any other is 403 `registration_code_invalid`. An
  admitted space's requests need no code, and one they carry is ignored. The device sends the code
  of its storage settings (`{kind: "relay", url, registration_code}` in the vault's local part)
  with every request; an invitation's storage never has one.
- **The relay's admission file**, `<data>/lockra-relay-v1/admission.json`, with code registration:
  `{version: 1, spaces: {<space id>: {access, note?, admitted_at, code?}}, codes: {<id>: {hash,
note?, created_at, expires_at}}}`: an admitted space's token hash (hex), the administrator's note,
  when (Unix seconds), and the id of the code that admitted it; a code not used yet as the hex
  SHA-256 of its 15 bytes, its id being the first 8 hex digits of that hash. Only the relay process
  writes it, through a temporary file renamed into place; one it cannot read stops it at start.
- **The listing** is `{"objects": [{"name", "size", "etag"}]}`, the snapshots by name. Its `ETag`
  is the listing's revision, a hash of the names and etags. `If-None-Match: <revision>` answers
  `304` while nothing changed; with `?wait=<seconds>` too, the relay holds the `304` back until a
  snapshot is written or removed, or the wait (at most its `--max-wait`, 30 s by default) is over.
  A space the relay does not keep lists empty.
- **Snapshots.** A read answers the bytes and their `ETag`: the first 16 bytes of their SHA-256 in
  hex, quoted. A write answers the new etag and honours `If-None-Match: *` and `If-Match: <etag>`
  (`412` when they fail): unlike WebDAV, a relay holds a write's condition. Removing nothing is no
  error (`204`). A name that is no device tag and `.lks`, and a space id not written as a lowercase
  UUID, are `404`.
- **Refusals.** `413` for a snapshot larger than the relay keeps; `507` with `{"error": "full",
"limit": "devices" | "space" | "relay" | "spaces"}` at a limit of the space or of the relay; `429`
  with `Retry-After` past a rate; `503` with `Retry-After` while a space being removed was in use.
  Every other error is `{"error": "<code>"}` with its status. Every answer carries
  `Cache-Control: no-store`.

## 10. The browser extension's channel

The extension keeps no vault. It reaches the running app through native messaging: the browser
starts the host (`lockra-desktop` in host mode, the shell's part), which only passes frames between
its stdio and the app's local socket. Noise encrypts end to end between the extension and the app,
so the host and the socket are untrusted pipes (`crates/lockra-browser`, the shell's
`apps/desktop/src-tauri/src/browser/`).

- **Frames.** A 4-byte length, then that many bytes of JSON: the machine's byte order on stdio
  (native messaging), little-endian on the socket. At most 1 MiB either way, what a browser accepts
  from a host; a longer length, or a stream that ends inside a frame, ends the connection.
- **The envelope** is `{"v": 1, "t": "hello" | "hs" | "msg", "d": "<base64>"}`, and `"p": "<pairing
id>"` on the first message of a session's handshake. `hello` carries a plain JSON message, `hs` a
  Noise handshake message, `msg` an encrypted fragment. Another `v`, JSON that does not read, or a
  `d` that is not base64 ends the connection.
- **Before a handshake** (plain, `op`-tagged): the extension's
  `{"op": "hello", "proto_min", "proto_max", "browser", "label", "extension", "version",
"page_access"}` is answered with `{"op": "hello", "proto_min", "proto_max", "proto", "app_version",
"state"}`, `proto` the newest version both speak or `null` (the ranges then tell which side needs
  an update) and `state` one of `no_vault`, `locked`, `unlocked`. `{"op": "unlock_request"}` asks
  Lockra to come up with its unlock screen, at most once every ten seconds
  (`{"op": "unlock", "accepted"}`). The app says `{"op": "state", "state"}` when it locks or
  unlocks, and `{"op": "error", "error"}` for what it cannot answer: `hello_first`, `version`,
  `bad_message`, `locked`, `not_paired`, `busy`, `unavailable`. While locked nothing else is
  answered: the static key is in the vault.
- **Pairing**: `Noise_XX_25519_AESGCM_SHA256`, the prologue `lockra-browser/1`. The extension
  initiates; the app answers with its static key (made the first time, kept in the vault's local
  part, `local.browser.key`). When the third message is in, both sides show six digits: HKDF-SHA256
  of the handshake hash (no salt, info `lockra pairing code`), its first four bytes as a big-endian
  number, modulo a million, with leading zeros. The app pushes `pair_pending`; the request waits in
  Lockra, at most three at a time, for two minutes, until the user compares the codes and approves
  with the master password or the biometric check. Then `{"push": "paired", "pairing": "<id>"}`,
  or `{"push": "pair_failed", "reason": "denied" | "expired" | "busy" | "locked"}`, and the
  connection is back before a handshake. The vault keeps the pairing (`local.browser.paired`: id,
  browser, label, extension id, the extension's public key, when) and never syncs or backs it up.
- **Sessions**: `Noise_KK_25519_AESGCM_SHA256` between the two static keys, the same prologue, new
  ephemeral keys on each connection (forward secrecy). The first message names its pairing (`p`);
  a pairing Lockra does not have, or keys that do not match it, get `not_paired`. Once open, the app
  pushes `{"push": "ready", "pairing"}`.
- **Messages over a channel** are JSON of at most 512 KiB in at most eight Noise messages of at most
  65 535 bytes (eight full fragments carry 524 144 bytes). Each fragment's plaintext begins with a
  flag byte, 1 when more follow, 0 on the last; a ninth fragment, a longer message or another flag
  ends the connection.
- **Requests** are `{"req": <n>, "op": "<op>", …}`; their answers repeat the number:
  `{"req", "ok": true, "result"}` or `{"req", "ok": false, "error": "<refusal>"}`. The ops are
  `status`, `query` (the logins of the best match for a frame, without secrets, and
  `host_matching`: the frame matches by its full host, as the policy or the closest `match_host` or
  `match_domain` rule says, the popup's switch; absent from an older Lockra: false; with
  `"purpose": "save"` and the submitted user name and password, whether to offer saving or
  updating, never for a password that is the code of an account linked to a login offered there:
  the current one, the one before or the one after), `fill` (one login's secrets; `trigger` says
  what asked: `shortcut`, `popup`, `inline_menu`, `page_load`, or `next_step`, the page asking for
  the password within two minutes of a fill the user asked for, once), `otp` (the code of a login
  filled on this site in the last five minutes, for a later step; `submit` in the answer says
  whether to send its form, absent before 0.10: not sent; with `trigger` `popup`, `inline_menu` or
  `shortcut`, from 0.12, the code of a login the user chose that is offered for the frame, in the
  page or a frame of its own site, without a fill before), `copy` (a part on the clipboard, written
  by the app),
  `save`, `update`, `generate` (a password to the page's `passwordrules`, `minlength` and
  `maxlength` or the bundled rules, saved at once as a new login when asked), `copy_generated`
  (from 0.10.1: the sign-up menu's `password` on the clipboard, written by the app as a login's
  is, and only a password of the generator's history made for the frame's site, else
  `not_found`), `site_rule` (never save or never fill the frame's site), `site_matching` (the
  popup's switch, `host`: the frame's site and its subdomains match by the full host, or by the
  site; Lockra removes the `match_host` and `match_domain` rules about the site and about the
  domains of the frame's host within it, then adds one of the asked kind about the site where what
  is left, a rule about a parent of the site or the policy's default, says otherwise; only in the
  page or a frame of its own site, counted as a query, the change synced as one made in Lockra),
  `search`, `delete` (from 0.12: a login offered for the frame, which the user agreed to delete in
  the menu or the popup; never in a frame of another site or on a never-fill site, counted with the
  fills; the deletion syncs as one made in Lockra) and `link_code` (`item_id`: a login offered for
  the frame, with a linked code or not, for the save prompt that took a one-time code; only in the
  page or a frame of its own site, counted as a query, and answered at once, before the user picks
  the code in Lockra: the app tells its interface `browser_link_code_requested` (the login's id and
  title, the browser), comes up as for a pairing or a confirmation, and shows the login on the
  passwords page with its code picker open, never over an open dialog, where only the notice
  shows). Each names
  its frame with `frame_url`,
  `top_url` and `frame_chain` (the ancestors, nearest first), which the extension's background
  script takes from the browser, never from the page. A request this version cannot read gets
  `invalid` when its number can be read. `fill`, `copy` and `delete` may wait for the user (a login
  of the `confirm` level; a frame of another site under the `ask` rule), and other requests are
  answered meanwhile.
- **Refusals**: `unavailable`, `locked`, `not_paired`, `paused`, `busy`, `invalid`, `never_fill`,
  `never_save`, `no_match`, `cross_origin`, `http_downgrade`, `protected`, `denied`, `expired`,
  `rate_limited`, `not_found`.
- **Pushes** besides the pairing ones: `revoked` when the user removes the pairing (the session
  ends). As the vault locks, every session ends and the connection hears `{"op": "state",
"state": "locked"}`.
- **The policy** (`local.browser.policy`, never in `settings.json`): `fill_otp` and `copy_otp`
  (on), `auto_submit`, `confirm_fill`, `on_page_load`, `match_port`, `match_host` (off) and
  `cross_origin_frames` (`never` or `ask`; a value of a later version reads as `never`).
  `match_port` makes the domain rule compare the port as well (the scheme's default port is none).
  `match_host` makes it ask for the page's host and port, as the host rule does and at the host
  rule's level, the equivalent domains counting for nothing; a `match_host` or `match_domain` site
  rule about the page's host or a parent of it decides instead (§2). A change that lets the
  extensions do more asks for the master password or the biometric check; `match_port` and
  `match_host` never do. `settings.json`'s `browser_integration` only says whether the shell
  listens and keeps the browsers' manifests; only the shell changes it.

- **The host's own.** The host answers by itself only what needs no app, on `hello` envelopes:
  `{"op": "state", "state": "not_running"}` when a frame finds no app to go to (its socket cannot
  be reached, or is not in a folder of the user's alone) and as the app's connection ends; the next
  frame connects again. The extension's `{"op": "start_app"}` starts Lockra as the browser's
  (`--from-browser`: in the tray while Lockra runs in the background, and a running Lockra keeps
  its window as it is), and `{"op": "start_app", "show": true}` brings Lockra's window up; neither
  is answered, nor passed on: the extension says `hello` again (every second, for at most 30
  seconds) until Lockra does. The app never sees `start_app`; a `hello` envelope it gets with one
  is a `bad_message`.
- **Host mode.** The browser starts `lockra-desktop` with the extension's origin first
  (`chrome-extension://<id>/`, then on Windows a `--parent-window=` handle) or, Firefox, with the
  manifest's path and the add-on's id. Nothing of the app starts then: no Tauri context, no
  single-instance check, no window; stdout carries the protocol alone, the log goes to stderr.
- **The socket.** Linux: `$XDG_RUNTIME_DIR/lockra/browser.sock`, else
  `<data>/browser/browser.sock`; macOS: `<data>/browser/browser.sock` while its path is at most
  104 bytes, else `$TMPDIR/lockra/browser.sock`. Its folder is created 0700 (an existing one must
  be the user's and is set to 0700); the app takes a connection only from a process of the same
  user (`peer_cred`), and the host connects only to a socket in a folder of the user's alone. A
  socket left by a Lockra that did not end is replaced; anything else in its place is left, and
  nothing listens. Windows: the named pipe `\\.\pipe\lockra-browser-<SHA-256 of the config
folder, hex>`, created as the first instance and refusing remote clients.
- **The manifests** (`dev.lockra.browser`, `"type": "stdio"`, the program's path: the AppImage when
  Lockra runs from one): Chrome, Chromium, Edge and Brave's allow `chrome-extension://<id>/` for
  Edge Add-ons' id (the self-install package's too), the Chrome Web Store's once the extension is
  listed there (Edge installs from the Chrome Web Store too) and, in a debug build, the development
  id; Firefox's allows `lockra@firlab.app`. Linux:
  `~/.config/{google-chrome, chromium, microsoft-edge, BraveSoftware/Brave-Browser}/NativeMessagingHosts/`
  and `~/.mozilla/native-messaging-hosts/`; macOS: the same browsers' folders under
  `~/Library/Application Support/` (`Mozilla/NativeMessagingHosts/` for Firefox); Windows: the
  files in `%LOCALAPPDATA%\Lockra\browser\` (one for the Chromium browsers, one for Firefox),
  pointed at by the default value of `HKCU\Software\{Google\Chrome, Chromium, Microsoft\Edge,
Mozilla}\NativeMessagingHosts\dev.lockra.browser` (Brave reads Chrome's). Written for the
  browsers the user turns the integration on for, written again at each start while it is on,
  removed when it is turned off. A debug build may put the Chromium ones in
  `$LOCKRA_DEV_BROWSER_MANIFEST_DIR/NativeMessagingHosts/` (a test browser's `--user-data-dir`).
- **The extension's folder** (after 0.10.1): the Chromium build of the extension the desktop app
  carries (the self-install package's files, Edge Add-ons' public key), for the browsers to load
  unpacked, in `Lockra/browser/extension` under the local data folder (`%LOCALAPPDATA%` on Windows,
  `~/Library/Application Support` on macOS, `$XDG_DATA_HOME` or `~/.local/share` on Linux). Its
  `lockra-folder.json` is `{"version": "0.10.2", "digest": "sha256:<hex>"}`: the version of the
  copy and the SHA-256 over every file sorted by its path, each as the path's length (u64
  little-endian), the path, the content's length and the content. It is written last, into a new
  copy beside the folder (`extension.staging`), which then takes the folder's place
  (`extension.old` meanwhile, removed after). The app rewrites the folder when the digest differs
  from its own copy's; the extension reads only `version`, from its own package, and reloads itself
  when it differs from the manifest it runs (`update.reload_tried` in its `storage.local` keeps it to
  one try per version). An extension of a later build (below) also reads the copy's
  `lockra-build.json`, `{"build": "<id>"}`, and reloads itself when the copy says another build than
  its own, its one try then kept per version and build; it reads both files as it starts, as a page
  of its own opens, and each time Lockra answers it again (a new version, or Lockra back after it was
  gone: Lockra puts its copy in the folder before it listens).
- **The extension's build.** Its background, content script and pages are built from one tree and
  carry one build id (`apps/extension/scripts/build-id.mjs`): the first 12 hex digits of the
  SHA-256 over `lockra <version>` and a zero byte, then each source file it is built from (its own,
  tests left out, and those of the workspace packages it bundles), in the script's fixed order, as
  its path from `apps/extension`, a zero byte, its content and a zero byte. The background's views
  for an inline frame (`{"kind": "view", "view", "build"}`) and its state for the popup and the
  options page (`build`) carry its id. A frame or page that gets another id, or none (a background
  from before the id), runs beside an older background the browser still has after the folder
  changed under it: the frame shows the reload in place of the view, the popup and the options page
  a note, each with `runtime.reload()`, unless the background says it waits to take the folder's
  copy (`update: "waiting"`, once the pages close). The id never leaves the browser.

`crates/lockra-browser/tests/vectors/` holds every message as these types write it
(`protocol.json`) and both handshakes with fixed keys, their hashes, the code and channel messages
as snow makes them (`noise.json`): the extension's implementation is checked against them, and they
are compared byte for byte (`UPDATE_BROWSER_FIXTURES=1 cargo test -p lockra-browser --test protocol`
after a deliberate change).

## 11. Logins from other apps

Where password management is on (`UiState.features.passwords`), the import reads the logins of
browsers, password managers and KeePass beside the accounts (`crates/lockra-transfer`: `csv_logins`,
`bitwarden`, `onepux`, `kdbx`; `crates/lockra-core/src/import.rs`). Where it is off, none of these
formats is recognised: such a file is read as it was before (unrecognised, or a text whose lines
are no `otpauth://` URIs). The chosen logins also go out to a KeePass database or Bitwarden's CSV
("Logins exported", at the end).

### What a file is

A file's kind comes from its bytes, these before the `otpauth://` text (Safari's and 1Password's
CSV carry `otpauth://` links of their own):

| Kind                   | Recognised by                                                                                   |
| ---------------------- | ----------------------------------------------------------------------------------------------- |
| KeePass database       | `03 D9 A2 9A 67 FB 4B B5` (KDBX 4 only)                                                         |
| 1Password 1PUX         | a zip (`PK\x03\x04`) holding `export.data`; another zip is unrecognised                         |
| Bitwarden JSON         | an object whose `encrypted` is a boolean, beside `items` (or, encrypted, `data`)                |
| Chrome and Edge CSV    | `name,url,username,password[,note]`                                                             |
| Firefox CSV            | `url,username,password,httpRealm,formActionOrigin,…`                                            |
| Safari / Passwords CSV | `Title,URL,Username,Password,Notes,OTPAuth` (older: `Title,Url,Username,Password`)              |
| Bitwarden CSV          | `folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,…` (or `collections`) |
| 1Password CSV          | `Title,Url,Username,Password,OTPAuth,Favorite,Archived,Tags,Notes`                              |
| LastPass CSV           | `url,username,password,totp,extra,name,grouping,fav`                                            |

Header names compare in any case; a CSV whose header is none of these goes on to be read as text.
The input is limited to 64 MiB a file, what a compressed file expands to as well (1PUX's
`export.data`, a KeePass payload), and a zip to 10 000 entries: past them the file is unreadable.

### What a login becomes

| Source's                                     | Lockra's login                                                                                                                                                                                        |
| -------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| name, title                                  | `title` (without one, Chrome's and Firefox's take the URL's host)                                                                                                                                     |
| user name                                    | the `username` field                                                                                                                                                                                  |
| password                                     | the `password` field, hidden                                                                                                                                                                          |
| URLs                                         | `urls`, at most 32, each `domain` unless the source says: Bitwarden's URI match `0` domain, `1` host, `2` starts with, `3` exact, `4` regex, `5` never                                                |
| notes (LastPass's `extra`)                   | `notes`                                                                                                                                                                                               |
| folder, collection, grouping, KeePass groups | `group`: a nested group as its path (`Work/Servers`), the root group left out; 1PUX names the vault when the export has several                                                                       |
| favourite                                    | `favorite`                                                                                                                                                                                            |
| tags                                         | `tags` (separated by commas, semicolons or tabs), at most 32                                                                                                                                          |
| custom fields                                | custom fields; hidden ones (Bitwarden's hidden, 1Password's concealed and passwords, KeePass's protected strings) stay hidden; Bitwarden's linked fields, holding no value of their own, are left out |
| Bitwarden's re-prompt                        | `protection: confirm`                                                                                                                                                                                 |
| the app                                      | `source`: `chrome`, `firefox`, `safari`, `bitwarden`, `1password`, `lastpass`, `kdbx`                                                                                                                 |

A login is a set of new writes (§2): each part gets a fresh write id and a stamp of the commit.
One past what a login holds (a password or a field's value over 4 KiB, notes over 64 KiB, more
than 64 fields) is listed as left out, `too_large`, with its code.

Left out and listed, for a later Lockra: payment cards and bank accounts (`card_item`), identities,
passports and licences (`identity_item`), secure notes (`note_item`), other kinds such as SSH keys
and documents (`other_item`), and an encrypted Bitwarden export as a whole (`encrypted_export`).
An item of a shape the reader does not know is `other_item`, not a reason to refuse the file.

### Codes

A login's code is a candidate of its own, right after the login: an `otpauth://` URI (named after
the login where the URI names nobody), or a bare Base32 secret (Bitwarden, LastPass, 1Password)
read as TOTP, SHA1, six digits, 30 s. KeePass keeps one in `otp` (a URI, or KeeOtp's
`key=…&size=…&step=…&otpHashMode=…`), in KeePass 2's `TimeOtp-Secret[-Base32|-Hex|-Base64]` with
`TimeOtp-Length`, `-Period`, `-Algorithm` (and `HmacOtp-*` with `HmacOtp-Counter` for HOTP), or in
the older `TOTP Seed` and `TOTP Settings` (`30;6`; `30;S`, Steam's, is not taken). A code that does
not read is listed as unsupported with its reason; the login links nothing then. An imported code
is an account of `origin: uri`, never an origin of its own: devices of 0.9 do not read one.

The commit takes the codes first and maps each candidate to the account it ends as: a new one, the
vault's account of the same secret, the account an earlier candidate of the same secret ends as,
the account that replaces one of the same name; a code skipped, kept beside an account of its name
with another secret, or unsupported ends as none. Then each login is
linked through that map: a code that ends as none links nothing, and the source's own id never
stays. Two logins of one code link the same account.

### Logins in the preview

A login is compared by its site, its user name and its password. Its site is the host of its
first URL that is a web address (`WebUrl`), else its first URL as written, else its title (a
KeePass entry without a URL); user names compare trimmed. Its code is compared as it would be
mapped, with the codes' default choices: a code the import would add is a new account.

| Status           | When                                                                                                                 | Default |
| ---------------- | -------------------------------------------------------------------------------------------------------------------- | ------- |
| `new`            | none of the below                                                                                                    | add     |
| `login_exists`   | the vault has the same site, user name and password, and its code is the same or the import has none for it          | skip    |
| `duplicate`      | an earlier login of this import has the same site, user name and password                                            | skip    |
| `login_conflict` | the vault's login of the same site and user name has another password (`password`) or another code, or none (`code`) | replace |

_Replace_ writes the import's password into the vault's login (the one it replaces goes into the
history) or links the import's code (the account linked before stays in the code list); a code
the import maps to no account leaves the login's link as it is. _Add_ keeps both. The commit
compares the logins with the vault as the preview showed it: two logins of one site in one import
are two logins, not one replacing the other.

A backup's login (a merge, or a backup opened in the import) is compared by its id first: the same
id and the same content (everything but the link, the stamps, the write ids, the history and the
times) is `login_exists` when its code maps to the account it links here, `login_conflict`
(`code`) otherwise; the same id and other content is `login_conflict` (`changed`), and replacing
takes the backup's version back as a restore that replaces does (§4, write ids kept, new stamps).
A login whose id this vault does not hold compares as any other; a new one gets a new id and new
stamps, as a backup's account does, so that no deletion of this vault or of its space removes it.

### KeePass databases

A KDBX 4 database waits in the import for its password, as a backup does (`ImportView.awaiting`,
`kind: kdbx`), and for its key file if it takes one: the shell's `import_pick_key_file` opens the
system's file dialog (on the phone, its file picker) and the core keeps the key the file brings,
32 bytes, and its name for the preview; `import_key_file_clear` forgets it. The composite key is
SHA-256 of SHA-256(password) (left out when the password is empty and a key file is given) and the
key file's key: an XML key file of version 1.0 (Base64) or 2.0 (hex, its four-byte `Hash` checked),
32 bytes as they are, 64 hex digits, or any other file's SHA-256. An empty file, or an XML key file
that does not read, is refused at once (`import_key_file_invalid`). A wrong key is `wrong_password`
without a key file, `import_key_file_invalid` with one (the two cannot be told apart).

Lockra reads the database itself: the ciphers AES-256-CBC, Twofish-CBC and ChaCha20; gzip or none;
the inner streams ChaCha20 and Salsa20; the key derivations AES-KDF, Argon2d and Argon2id (its
lanes in parallel). Before any work, the header is checked and the key derivation bounded: Argon2
at most 1 GiB, 4 096 passes, 64 lanes and 64 GiB of memory times passes, the AES-KDF at most 2^30
rounds; a database past them, of KDBX 3.1 or earlier, or of a cipher it does not know, is said to
be unreadable as the file is added. The header's SHA-256 and HMAC, then every block's HMAC, are
checked; the payload expands to at most 64 MiB; the XML is read as events, its protected values
unmasked in document order (the entries' histories included). The entries of the recycle bin
(`Meta/RecycleBinUUID`, while `RecycleBinEnabled`) are left out; `KP2A_URL*` strings are more URLs,
in the order of their number (`KP2A_URL`, `KP2A_URL_1`, `KP2A_URL_2`, …).

### Logins exported

The chosen logins export to a file (`export_items_file`, a shell command), once the user proved to
be there: the master password, or the biometric check that unlocks the vault. A login of a later
Lockra's kind is not exported. The file is made in Rust before the save dialog (the phone's file
picker) opens and written where the user chose, readable by its owner only, a file it replaces
too; the webview hears its name. Where password management is off the command answers
`feature_unavailable`.

| Lockra's login                        | KeePass database (`lockra-logins.kdbx`)                                     | Bitwarden's CSV (`lockra-logins.csv`)           |
| ------------------------------------- | --------------------------------------------------------------------------- | ----------------------------------------------- |
| `title`                               | `Title`                                                                     | `name`                                          |
| the user name, else the email address | `UserName`                                                                  | `login_username`                                |
| the email address beside a user name  | a string `Email`                                                            | a field `Email`                                 |
| the password                          | `Password`, protected                                                       | `login_password`                                |
| `urls`                                | the first in `URL`, the others in `KP2A_URL`, `KP2A_URL_1`, …               | `login_uri`, joined by commas                   |
| `notes`                               | `Notes`                                                                     | `notes`                                         |
| `group`                               | the entry's group, nested by `/` (`Dev/Web`), under the root group `Lockra` | `folder`                                        |
| `tags`                                | `Tags`                                                                      | none                                            |
| custom fields                         | strings of their names, the hidden ones protected                           | `fields`, one `name: value` a line, none hidden |
| `favorite`                            | none                                                                        | `favorite` (`1`)                                |
| `protection: confirm`                 | none                                                                        | `reprompt` (`1`)                                |
| the linked account's code             | `otp`, protected: its `otpauth://` URI, as KeePassXC keeps it               | `login_totp`: its `otpauth://` URI              |

Not exported: how each URL is compared (the import takes them back as `domain`), the history, the
times, the colour, the mark and the autofill choices. In a KeePass entry a custom field named as
one of KeePass's own strings (`Title`, `otp`, …) or as another field of the login takes a number
(`Title (2)`), one whose name begins as the strings Lockra reads as more URLs or a code
(`KP2A_URL…`, `TimeOtp-…`, `HmacOtp-…`) takes a `_` in front, and one without a name is `Field`.

The KeePass database is KDBX 4.1 as the keepass crate writes it: AES-256, gzip, ChaCha20 for the
protected values, the key derived with Argon2id at the vault's own cost (64 MiB, three passes, one
lane), under a password of its own, typed twice in the interface and at least eight characters
(`password_too_short`). The CSV is plain text, Bitwarden's export of a vault: the command asks that
the user confirmed it is (`acknowledged`, else `export_acknowledge_needed`), before the proof. Its
rows end with CRLF, a cell is quoted where it holds a comma, a quote, a line break or spaces at
either end. Both come back through the import (above) with their codes linked, but for what their
format has no place for.
