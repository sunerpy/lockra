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

## What Lockra cannot protect against

- Malware running as your user on the computer can read what you type, including the master
  password.
- With **Remember on this device** on, the vault is as safe as your account on the computer.
- A forgotten master password cannot be recovered, by anyone.

For the full design, see the [security model](/dev/security).
