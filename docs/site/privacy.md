# Privacy

This page lists what Lockra stores, where, and what it sends: nothing.

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

Nothing. Lockra makes no network connections: there is no account, no sync, no update check, no
crash report and no telemetry. Fonts, images and icons ship inside the app. Accounts leave the
computer only when you export them or save a backup to a place that synchronises.

## The clipboard and the screen

Copied codes go to the system clipboard, where other apps can read them until Lockra clears it.
Secrets and export codes appear on screen only after the master password; see
[How Lockra protects your accounts](/security/).
