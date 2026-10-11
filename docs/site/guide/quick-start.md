# Quick start

This page takes you from a fresh install to copying your first code in a few minutes.

## Create the vault

When Lockra starts for the first time it asks for a master password. The master password encrypts
all your accounts and **cannot be recovered**: if you forget it, nobody can open the vault, not even
with a backup made under it. Choose a sentence that only you know and that you can type reliably,
at least eight characters long, and enter it twice. The strength bar under the field helps.

If you already have a Lockra backup, choose **Restore a backup** instead: the backup's password
becomes the master password.

## Bring in your accounts

Open **Import** in the sidebar and choose the source:

- **Google Authenticator**: export the accounts on the phone, photograph the QR codes with another
  device, and drop the photos on the window. See [Google Authenticator](/transfer/google).
- **Microsoft Authenticator**: see [Microsoft Authenticator](/transfer/microsoft); it needs a rooted
  Android phone.
- **A website's setup page**: when a site shows a QR code to set up an authenticator, take a
  screenshot of it, or copy its otpauth link, and choose **Import from the clipboard**.

Lockra lists what it found before saving anything. Check the list and choose **Import**.

You can also add one account by hand: **Add › Enter manually** on the codes page, or `Ctrl N`.

## Copy a code

The **Codes** page lists your accounts with their current code and a ring that shows the time left.
Click an account to copy its code, then paste it into the site. Lockra clears the clipboard 30
seconds later if it still holds the code.

`Ctrl K` opens the command palette: type part of an account's name and press Enter to copy its
code without touching the mouse.

## Turn on automatic backups

Open **Backup**, turn on **Automatic backup** and choose a folder, ideally one that a cloud drive
synchronises. From now on an encrypted backup is written there a few seconds after every change.
See [Backups and restore](/backup/).

## Lock when you leave

Lockra locks itself after five idle minutes. `Ctrl L` locks at once; the master password unlocks
again.
