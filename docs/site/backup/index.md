# Backups and restore

This page explains how to back up the vault, how automatic backups work, and how to restore a
backup.

<ScreenFigure src="/screens/backup-en-light.webp" dark="/screens/backup-en-dark.webp" width="1440" height="900" alt="The Backup page with saving a backup now, automatic backups into a folder, and restoring a backup." />

A backup is a `.lockrabackup` file encrypted the same way as the vault. It can be kept anywhere,
including a cloud drive, because it opens only with its password.

## Saving a backup now

Open **Backup** and choose **Save backup…**, then a place for the file. By default the backup is
encrypted under the master password. Turn on **Use a separate backup password** to choose another
one, for example for a copy you hand to someone you trust.

## Automatic backups

Turn on **Automatic backup** and choose a folder. From then on, three seconds after the last change
Lockra writes `lockra-auto-<date>-<time>.lockrabackup` there and deletes the oldest automatic
backups beyond the number you keep (10 by default; 3 to 50). Only files with that name are ever
deleted; anything else in the folder is left alone.

A folder that a cloud service synchronises (OneDrive, Google Drive, iCloud Drive, Dropbox) carries
the backups to your other devices without Lockra going online. To keep the accounts themselves the
same on several devices, use [sync](/backup/sync).

If the folder cannot be written, for example because a drive is disconnected, the Backup page says
so and Lockra tries again at the next change. **Run automatic backup now** writes one at once.

### After changing the master password

Automatic backups are encrypted under the master password at the time they were written. After you
change it, earlier backups still open with the old password. Keep the old password until newer
backups exist.

## Restoring a backup

On the Backup page, choose **Choose a backup file…** under **Restore a backup** and enter the
backup's password. Then choose how:

- **Merge (confirm each)**: the backup's accounts go into the import preview, where you choose what
  to add, as with any import.
- **Replace the current accounts**: the vault's accounts are replaced by the backup's. Lockra first
  saves the current vault as `pre-restore-<date>-<time>.lockrabackup` in its data folder, under the
  master password.

On the welcome screen of a new installation, **Restore a backup** turns a backup into the vault,
and the backup's password becomes the master password.
