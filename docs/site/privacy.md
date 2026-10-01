# Privacy

This page lists what Lockra stores, where, and what it sends: nothing but an update check.

## What Lockra stores

| What                                              | Where                                                | How                                 |
| ------------------------------------------------- | ---------------------------------------------------- | ----------------------------------- |
| Your accounts                                     | `vault.lockra` in Lockra's data folder               | encrypted with your master password |
| Settings (theme, language, timers, backup folder) | `settings.json` in Lockra's settings folder          | plain text, no secrets              |
| A key for **Remember on this device**, if on      | the system keychain                                  | protected by your sign-in           |
| Backups                                           | where you save them, and the automatic backup folder | encrypted                           |

[Updates, uninstalling and your data](/guide/updates#where-the-files-are) lists the folders for each
system.

## What Lockra sends

Only an update check. When you choose **Settings › About › Check for updates**, or once a day if
you turned automatic checks on, Lockra asks GitHub for the newest version, and downloads it when
you choose to install it. Nothing from your accounts, your vault or your settings goes with these
requests; GitHub sees the request and the address it came from, as for any download
([Updating](/guide/updates#updating)). There is no account, no sync, no crash report and no
telemetry. Fonts, images and icons ship inside the app. Accounts leave the computer only when you
export them or save a backup to a place that synchronises.

## The clipboard and the screen

Copied codes go to the system clipboard, where other apps can read them until Lockra clears it.
Secrets and export codes appear on screen only after the master password; see
[How Lockra protects your accounts](/security/).
