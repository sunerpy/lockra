# How Lockra protects your accounts

This page explains how Lockra keeps your accounts safe, what each security setting does, and what
it cannot protect against.

## The master password

All accounts are kept in one file, encrypted with a key that Argon2id derives from your master
password using 64 MiB of memory and three passes, so trying passwords one after another is slow.
The file is encrypted with XChaCha20-Poly1305; if any part of it is changed, even one byte, Lockra
refuses to open it instead of reading altered data.

After three wrong passwords in a row, each further attempt has to wait, starting at one second and
doubling up to 30 seconds.

The master password cannot be recovered. If it is lost, **Forgot the master password?** on the
unlock screen resets the vault after you type RESET to confirm: Lockra starts over with an empty
vault and keeps the old file under a new name, which still cannot be opened without its password.

To change it, open **Settings › Security › Change master password**. Earlier backups keep opening
with the old password.

## Remember on this device

**Settings › Security › Remember on this device** keeps a key in the system keychain (Credential
Manager on Windows, Keychain on macOS, the Secret Service on Linux), so the vault unlocks without
the master password on this computer. Anyone who can sign in to your account on the computer can
then open the vault, so use it only on a computer that only you use.

Turning it off asks for the master password, deletes the key from the keychain and re-encrypts the
vault, so the deleted key opens nothing afterwards.

## Touch ID and Windows Hello

<StatusTag status="available" /> Available from version 0.5.0.

On a Mac with Touch ID, or a Windows PC with Windows Hello, turn on **Settings › Security › Unlock
with Touch ID** (**Unlock with Windows Hello** on Windows). Turning it on checks once that it works
and turns **Remember on this device** on with it. The unlock screen then shows **Unlock with Touch
ID** (or **Unlock with Windows Hello**): your fingerprint, or Windows Hello's fingerprint, face or
PIN, is checked before the remembered key opens the vault. Until it is on, the unlock screen of a
computer that has Touch ID or Windows Hello says where to find the switch. Turning it off asks for
the master password and leaves **Remember on this device** on. The master password always unlocks.

On Windows, if Windows Hello becomes unavailable at the moment you unlock (for example while another
app shows its own Windows Hello prompt), Windows asks for your Windows sign-in password instead.

The check guards against someone using your unlocked computer. A program running as you could
still read the remembered key from the keychain, as without the check.

## Default unlock

<StatusTag status="available" /> Available from version 0.7.0.

Once **Unlock with Touch ID** (or **Unlock with Windows Hello**) is on, **Settings › Security ›
Default unlock** chooses what the unlock screen does first. With **Touch ID** (**Windows Hello** on
Windows), the default, its button comes first and the unlock screen asks for the check by itself:
when Lockra starts, when it locks by itself while its window is in front, and when you come back to
its window while it is locked. After you lock it yourself, or cancel the check, it asks again once
you have left the window and come back. It never asks while Lockra is in the background. With
**Master password**, you type the password first and the check waits for its button.

## Fingerprint on Android

<StatusTag status="available" /> Available from version 0.7.0.

On the phone, **Settings › Security › Unlock with fingerprint** keeps the vault's key in the
phone's secure hardware, usable only for ten seconds after a passed fingerprint check. Turning it
on checks once; turning it off asks for the master password and deletes the key. A fingerprint
enrolled afterwards makes the key unusable: the master password unlocks, and the switch can be
turned on again. With **Default unlock** on **Fingerprint**, opening Lockra or coming back to it
asks for the fingerprint by itself. Lockra is always kept out of screenshots on the phone, and
leaving the app locks the vault.

### Fingerprint by default

<StatusTag status="available" /> Available from version 0.7.1.

On a phone with an enrolled fingerprint, **Unlock with fingerprint** on the welcome page is on when
you create a vault, restore one from a backup or join a sync space: one fingerprint check once the
vault is open turns it on. Turning that switch off leaves it off, and Lockra does not ask again. An
existing vault unlocked with the master password asks once whether to turn it on; after **Not now**
this phone does not ask again, and the switch stays in **Settings › Security**.

## Locking

Lockra locks after five minutes without a key press or click (**Settings › Security › Auto-lock**:
1 to 60 minutes, or never), and at once with `Ctrl L` or **Lock** in the sidebar. Locking removes
the decrypted accounts from memory and closes any open import or export.

## The clipboard

A copied code is cleared from the clipboard after 30 seconds (**Settings › Security › Clear
clipboard**: 10 to 90 seconds, or never), and only if the clipboard still holds it. On Windows the
code is kept out of the clipboard history, the cloud clipboard and clipboard monitors; on macOS and
Linux it is marked to stay out of clipboard history, which clipboard managers that support the mark
respect.

## Secrets on screen

Showing an account's secret (**Show secret…**) and showing export codes both ask for the master
password again, and both hide after two minutes. While they are shown, Windows and macOS exclude
the window from screenshots and screen recording. Linux has no such control: the reveal view says
so, so mind screen sharing.

**Settings › Security › Hide codes** shows dots instead of codes until you point at an account.

## Sync

Sync (from version 0.4.0) is off until you set it up, on storage of your own or a Lockra relay. Everything a device writes there is
encrypted on the device, and from the storage the key that encrypts it opens only with both the
master password of a device in the space and the recovery key. An invitation carries that key
itself, so a device that has one joins without a password: Lockra shows it only after the master
password or Touch ID, Windows Hello or the fingerprint, and hides it after two minutes. Each device
writes only its own file, so devices never overwrite each other's changes; a file that was altered
or put back to an older version is refused. The storage's credentials and the recovery key are kept
in the encrypted vault and never in a backup.
[Sync between devices](/backup/sync#what-the-storage-can-see) explains what the storage can see.

### Through a Lockra relay

<StatusTag status="available" /> Available from version 0.8.1.

A Lockra relay, the built-in one or your own, is storage like any other and is trusted with no
more: it keeps the same encrypted files, and neither the key that encrypts them nor the recovery
key reaches it. The devices identify themselves to it with a value made from the recovery key, from
which the recovery key cannot be recovered, so nobody else can read or change the space there. The relay sees
when and from which network addresses the devices sync, and it can delete the space or be
unavailable; the devices keep every account either way
([Through a Lockra relay](/backup/sync#through-a-lockra-relay)).

## What Lockra cannot protect against

- Malware running as your user on the computer can read what you type, including the master
  password.
- With **Remember on this device** on, the vault is as safe as your account on the computer.
- A forgotten master password cannot be recovered, by anyone.
- With sync on, someone who photographs an invitation, or gets its text and its code, joins the
  space. Someone who has both the storage's contents and the recovery key can try master passwords
  on their own computer, against every device's; a long master password on every device is the
  defence. Whoever can write to the storage can
  delete the space, which stops sync but not your vaults. Removing a device does not revoke it.

For the full design, see the [security model](/dev/security).
