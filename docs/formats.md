# Lockra file and transfer formats

What Lockra writes and reads. The code is the authority: `crates/lockra-vault` (container),
`crates/lockra-core` (entries, settings, backups), `crates/lockra-transfer` (Google, Microsoft,
otpauth lists, QR codes), `crates/lockra-otp` (codes and URIs).

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
| `slots`         | the data key wrapped once per way in: `{kind: "password" \| "device", nonce, wrapped_dek}`                                                                          |
| `payload_nonce` | 24 bytes                                                                                                                                                            |

Binary fields are Base64. Cryptography:

- The **payload** (the entries, JSON) is encrypted with XChaCha20-Poly1305 under a random 32-byte
  data key (DEK). Its associated data is **every byte before it**: the magic, the length and the
  whole header, slots included. Changing any header byte, including a slot, fails authentication.
- A **password slot** wraps the DEK with a key derived from the password by Argon2id (the header's
  `kdf`). A **device slot** wraps it with a random 32-byte key kept in the OS keychain
  (service `dev.lockra.desktop`, account = `vault_id`). A slot's associated data is
  `lockra/slot/v1 | vault_id (16 bytes) | kind (1 = password, 2 = device)`.
- Every write rewrites the whole file: a fresh payload nonce, the current header as associated
  data. There is no in-place slot edit.

Slot operations:

| Operation                             | DEK         | Slots afterwards                                                                                                                      |
| ------------------------------------- | ----------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| create                                | new         | password                                                                                                                              |
| remember on this device               | kept        | password + device (new device key into the keychain)                                                                                  |
| stop remembering (needs the password) | **rotated** | password; the keychain entry is deleted                                                                                               |
| change the master password            | **rotated** | password (new salt) + device, if the keychain still returns the device key; otherwise the device slot is dropped and the user is told |

Errors: a password slot that does not open is `WrongPassword`; a payload that fails
authentication, a truncated file or a malformed header is `Corrupted`; a future `format` is
`UnsupportedVersion`; a foreign magic is `NotLockra`.

Writes are atomic: `vault.lockra.tmp` is written and synced, the current file is kept as
`vault.lockra.prev`, the new one is renamed into place, and the directory is synced (Unix). Files
are created `0600` on Unix.

## 2. The vault

`vault.lockra` in the app data directory (`~/.local/share/dev.lockra.desktop`,
`%APPDATA%\dev.lockra.desktop`, `~/Library/Application Support/dev.lockra.desktop`). Its payload
holds the entries:

| Field                                               |                                                                          |
| --------------------------------------------------- | ------------------------------------------------------------------------ |
| `id`                                                | UUID                                                                     |
| `kind`                                              | `{type: "totp", period}` (1–3600 s) or `{type: "hotp", counter}`         |
| `algorithm`                                         | `sha1`, `sha256`, `sha512`                                               |
| `digits`                                            | 6–8                                                                      |
| `secret`                                            | bytes (never sent to the webview except by an explicit reveal or export) |
| `issuer`, `account`, `group`, `favorite`            |                                                                          |
| `origin`                                            | `manual`, `uri`, `google`, `microsoft`, `backup`                         |
| `created_at_ms`, `updated_at_ms`, `last_used_at_ms` |                                                                          |

Duplicates: the same secret and parameters is **the same account** (an import skips it); the same
issuer and account with a different secret is a **conflict** (both are kept by default, or the
import replaces the existing one). Duplicates inside one import are collapsed first.

A forgotten password: _reset_ renames the vault to `vault.lockra.reset-<ms>` (and `.prev` along
with it) and starts over; nothing is deleted.

## 3. Settings

`settings.json` in the app config directory, plain JSON without secrets: theme, follow the system
theme, accent, density, font size (12–18 px), reduce motion, locale (`system`, `zh-cn`, `en`),
auto-lock minutes (0 = never; default 5), clipboard clearing seconds (0 = never; default 30), hide
codes, code order (`name`, `added`, `recent`), automatic backup `{enabled, dir, keep}` (keep
3–50, default 10) and automatic updates (`auto_update`, default off; 0.2.0 wrote it as
`auto_check_updates`, which is still read). Unknown or missing fields take their defaults.

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

Automatic backups go to the chosen folder 3 seconds after the last change, named
`lockra-auto-YYYYMMDD-HHMMSS.lockrabackup` (UTC; `-2`, `-3`, … on a clash). Beyond `keep`, the
oldest files **of that pattern** are deleted; nothing else in the folder is touched. A folder that
cannot be written is reported in the app and tried again at the next change.

Restoring into a vault: _merge_ puts the backup's accounts into the import preview; _replace_
first writes the current vault as `pre-restore-YYYYMMDD-HHMMSS.lockrabackup` in the data
directory. Restoring on the welcome screen makes the backup the vault and its password the master
password. A vault file (`.lockra`) can be restored the same way.

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
