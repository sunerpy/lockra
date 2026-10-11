# Privacy

This page lists what Lockra stores, where, and what it sends: update checks and downloads, and,
once you turn on sync, encrypted files to storage of your own (from version 0.4.0) or to a Lockra
relay (from version 0.8.1).

## What Lockra stores

| What                                              | Where                                                | How                                 |
| ------------------------------------------------- | ---------------------------------------------------- | ----------------------------------- |
| Your accounts                                     | `vault.lockra` in Lockra's data folder               | encrypted with your master password |
| Settings (theme, language, timers, backup folder) | `settings.json` in Lockra's settings folder          | plain text, no secrets              |
| A key for **Remember on this device**, if on      | the system keychain                                  | protected by your sign-in           |
| Backups                                           | where you save them, and the automatic backup folder | encrypted                           |
| Sync settings and credentials, if sync is on      | inside `vault.lockra`                                | encrypted with your master password |
| The sync space, if sync is on                     | the relay, bucket or folder you set up               | encrypted on the device             |

[Updates, uninstalling and your data](/guide/updates#where-the-files-are) lists the folders for each
system.

## What Lockra sends

Update checks and downloads, and the sync you set up. When you choose **Check for updates**, Lockra asks GitHub for
the newest version, and downloads it when you choose **Update now**. With **Settings › General ›
Automatic updates** on, it also asks 10 seconds after each start and downloads a newer version in
the background. Nothing from your accounts, your vault or your settings goes with these requests; GitHub sees the request and the address it came from, as for any download
([Updating](/guide/updates#updating)).

With sync on (from version 0.4.0), each device writes encrypted files to the S3-compatible bucket
or WebDAV folder you set up and reads the other devices' files from there, and contacts nothing else for it. The
storage sees encrypted files, their sizes in steps of 4 KiB, the number of devices and when they
write; never an account, a secret or a device name ([Sync between devices](/backup/sync)).

On a Lockra relay (from version 0.8.1), the devices write and read the same encrypted files there
instead; what the built-in relay receives and keeps is below.

There is no Lockra account, no crash report and no telemetry. Fonts, images and icons ship inside
the app. Accounts leave the device only when you export them, save a backup to a place that
synchronises, or sync them, encrypted, through your storage or a relay.

## The built-in relay

<StatusTag status="available" /> Available from version 0.8.1.

When you choose **Lockra's built-in relay** for sync, each device sends its encrypted files to the
relay Lockra runs and reads the other devices' files from there. The relay is run by Lockra's
developer, on a server in AWS's Seoul region (South Korea).

- **What it receives**: the encrypted files, as the devices made them; an identifier of the space
  and of each device, computed from the space's keys, from which a device cannot be recognised in
  another space; the value the devices identify themselves with, of which it keeps only a
  fingerprint; and the network address each request comes from.
- **What it cannot see**: your accounts, their secrets, the device names, the recovery key and the
  master passwords. The files stay encrypted on the relay; neither the relay nor Lockra's developer
  can open them.
- **How long**: a space is deleted with its files 400 days after a device last reached it.
  Removing a device in **Settings › Sync** deletes its file at once.
- **Network addresses**: kept in the relay's memory only, to limit the requests from each address,
  and never written to disk. The relay logs no request, and the load balancer in front of it keeps
  no access logs.
- **No sharing**: nothing the relay holds is shared or sold. AWS hosts the server.

## On Android

<StatusTag status="available" /> Available from version 0.7.0.

The Android app keeps the vault and its settings in its private folder, which Android's backup and
device transfer leave out, and the key for **Unlock with fingerprint** in the phone's secure
hardware. It sends what a computer sends, with one difference: it checks for updates only when you
tap **Check for updates**, and downloads nothing itself, since a newer release's page opens in
your browser.

## The clipboard and the screen

Copied codes go to the system clipboard, where other apps can read them until Lockra clears it.
Secrets and export codes appear on screen only after the master password (or, on the phone, the
fingerprint); see
[How Lockra protects your accounts](/security/).

## Contact

Lockra is developed by sunerpy, who also publishes it on Google Play. For a question about privacy,
open an issue on [GitHub](https://github.com/sunerpy/lockra/issues) or write to
nkuzhangshn@gmail.com.
