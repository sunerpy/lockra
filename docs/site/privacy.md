# Privacy

This page lists what Lockra stores, where, and what it sends: update checks and downloads, and,
once you turn on sync, encrypted files to storage of your own.

## What Lockra stores

| What                                              | Where                                                | How                                 |
| ------------------------------------------------- | ---------------------------------------------------- | ----------------------------------- |
| Your accounts                                     | `vault.lockra` in Lockra's data folder               | encrypted with your master password |
| Settings (theme, language, timers, backup folder) | `settings.json` in Lockra's settings folder          | plain text, no secrets              |
| A key for **Remember on this device**, if on      | the system keychain                                  | protected by your sign-in           |
| Backups                                           | where you save them, and the automatic backup folder | encrypted                           |
| Sync settings and credentials, if sync is on      | inside `vault.lockra`                                | encrypted with your master password |
| The sync space, if sync is on                     | the bucket or WebDAV folder you set up               | encrypted on the device             |

[Updates, uninstalling and your data](/guide/updates#where-the-files-are) lists the folders for each
system.

## What Lockra sends

Update checks and downloads, and the sync you set up. When you choose **Check for updates**, Lockra asks GitHub for
the newest version, and downloads it when you choose **Update now**. With **Settings › General ›
Automatic updates** on, it also asks 10 seconds after each start and downloads a newer version in
the background. Nothing from your accounts, your vault or your settings goes with these requests; GitHub sees the request and the address it came from, as for any download
([Updating](/guide/updates#updating)).

With sync on, each device writes encrypted files to the S3-compatible bucket or WebDAV folder you
set up and reads the other devices' files from there, and contacts nothing else for it. The
storage sees encrypted files, their sizes in steps of 4 KiB, the number of devices and when they
write; never an account, a secret or a device name ([Sync between devices](/backup/sync)).

There is no Lockra account, no Lockra server, no crash report and no telemetry. Fonts, images and
icons ship inside the app. Accounts leave the device only when you export them, save a backup to a
place that synchronises, or sync them, encrypted, through your storage.

## The clipboard and the screen

Copied codes go to the system clipboard, where other apps can read them until Lockra clears it.
Secrets and export codes appear on screen only after the master password; see
[How Lockra protects your accounts](/security/).
