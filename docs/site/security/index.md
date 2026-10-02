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

On a Mac with Touch ID, or a Windows PC with Windows Hello, turn on **Settings › Security › Ask for
Touch ID to unlock** (**Ask for Windows Hello to unlock** on Windows) after **Remember on this
device**. The unlock screen then shows **Unlock with Touch ID** (or **Unlock with Windows Hello**):
your fingerprint, or Windows Hello's fingerprint, face or PIN, is checked before the remembered key
opens the vault. Turning it on checks once that it works; turning it off asks for the master
password. The master password always unlocks.

<StatusTag status="building" /> From version 0.6.0 the switch is called **Unlock with Touch ID**
(**Unlock with Windows Hello**) and is offered before **Remember on this device** is on: turning it
on checks once and turns **Remember on this device** on with it. On a computer that has Touch ID or
Windows Hello but has not turned it on, the unlock screen says where to find the switch.

On Windows, if Windows Hello becomes unavailable at the moment you unlock (for example while another
app shows its own Windows Hello prompt), Windows asks for your Windows sign-in password instead.

The check guards against someone using your unlocked computer. A program running as you could
still read the remembered key from the keychain, as without the check.

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

Sync (from version 0.4.0) is off until you set it up on storage of your own. Everything a device writes there is
encrypted on the device, and the key that encrypts it opens only with both the master password of
a device in the space and the sync key. Each device writes only its own file, so devices never
overwrite each other's changes; a file that was altered or put back to an older version is
refused. The storage's
credentials and the sync key are kept in the encrypted vault and never in a backup.
[Sync between devices](/backup/sync#what-the-storage-can-see) explains what the storage can see.

## What Lockra cannot protect against

- Malware running as your user on the computer can read what you type, including the master
  password.
- With **Remember on this device** on, the vault is as safe as your account on the computer.
- A forgotten master password cannot be recovered, by anyone.
- With sync on, someone who has both the storage's contents and the sync key (a photographed
  invitation, for example) can try master passwords on their own computer, against every device's;
  a long master password on every device is the defence. Whoever can write to the storage can
  delete the space, which stops sync but not your vaults. Removing a device does not revoke it.

For the full design, see the [security model](/dev/security).
