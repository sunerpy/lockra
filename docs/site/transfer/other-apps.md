# Other apps, links and lists

This page explains how to bring accounts from any other authenticator, and how to export a plain
list.

## otpauth links

Most authenticators, and the setup pages of most services, can give an account as an otpauth link
(`otpauth://totp/…`) or as a QR code that contains one.

- **One link**: copy it and choose **Import from the clipboard** on the Import page or in the
  codes page's **Add** menu.
- **Several links**: paste them one per line under **otpauth links or a list** and choose **Read**,
  or choose a `.txt` file with one link per line. Blank lines and lines starting with `#` are
  skipped.
- **A QR code**: take a screenshot and choose **Import from the clipboard**, or save the picture and
  drop it on the window.

Every link and picture goes through the import preview, which lists what was found before anything
is saved. A line that is not a valid link is listed with its line number.

## Lockra backups

A backup can be imported like any other source: choose **Choose a backup file…** under **Lockra
backup**, enter the backup's password, and the accounts appear in the preview. This merges a backup
into the vault account by account; [Backups and restore](/backup/#restoring-a-backup) describes the
other ways to restore.

## Exporting a list

**Export › otpauth list file** writes the chosen accounts to a text file, one link per line, which
most authenticators can import. The file is **not encrypted**: anyone who reads it can generate the
codes. Lockra asks for the master password (or, on a phone with fingerprint unlock on, the
fingerprint) and a confirmation, and the file should be deleted once
it has been imported elsewhere. For keeping accounts safe, use a [backup](/backup/) instead.
