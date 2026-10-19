# Privacy

This page lists what Lockra stores, where, and what it sends: update checks and downloads, and,
once you turn on sync, encrypted files to storage of your own (from version 0.4.0) or to a Lockra
relay (from version 0.8.1).

## What Lockra stores

| What                                                                        | Where                                                                                                                                       | How                                 |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------- |
| Your accounts and logins                                                    | `vault.lockra` in Lockra's data folder                                                                                                      | encrypted with your master password |
| Settings (theme, language, timers, backup folder)                           | `settings.json` in Lockra's settings folder                                                                                                 | plain text, no secrets              |
| A key for **Remember on this device**, if on                                | the system keychain                                                                                                                         | protected by your sign-in           |
| Backups                                                                     | where you save them, and the automatic backup folder                                                                                        | encrypted                           |
| Sync settings and credentials, if sync is on                                | inside `vault.lockra`                                                                                                                       | encrypted with your master password |
| The sync space, if sync is on                                               | the relay, bucket or folder you set up                                                                                                      | encrypted on the device             |
| The generator's history, the paired browser extensions and what they may do | inside `vault.lockra`, on this device only                                                                                                  | encrypted with your master password |
| A connection file for each browser, if browser integration is on            | each browser's folder for native-messaging hosts; on Windows, `%LOCALAPPDATA%\Lockra\browser\` and a registry entry under your user account | plain text, no secrets              |

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

On a Lockra relay you run (from version 0.8.1), the devices write and read the same encrypted files
there instead; what that relay receives and keeps is below.

There is no Lockra account, no crash report and no telemetry. Fonts, images and icons ship inside
the app. Accounts and logins leave the device only when you export them, save a backup to a place
that synchronises, or sync them, encrypted, through your storage or a relay.

## The browser extension

<StatusTag status="available" /> Available from version 0.10.0.

The [browser extension](/passwords/browser) sends nothing over the network and collects nothing.
It talks only to Lockra on the same computer, through the browser's native messaging, over a
connection encrypted between the two. In your browser profile it keeps its own key and the pairing
with Lockra; while you use it, the site's logins as it last listed them (titles and user names, no
password) stay in the browser's memory until Lockra locks. A login's password reaches it only when
you fill or copy it.

Lockra keeps a list of what the extensions did, with the time, the browser and the site, in memory
only: it is cleared as the vault locks. Turning browser integration off removes the connection
files.

## A relay you run

<StatusTag status="available" /> Available from version 0.8.1.

When you configure a Lockra relay, each device sends its encrypted files to the server at the HTTPS
address you entered and reads the other devices' files from there.

- **What it receives**: the encrypted files, as the devices made them; opaque identifiers of the
  space and its devices; the access token, of which `lockra-relay` keeps only a fingerprint; and the
  network address each request comes from.
- **What it cannot see**: your accounts, their secrets, device names, recovery key and master
  passwords. The files stay encrypted on the relay and its operator cannot open them.
- **Data at rest**: taking the server's disk yields encrypted files, identifiers and the access-token
  fingerprint. `lockra-relay` writes no network address or request log to its data directory.
- **Retention and network logs**: the relay removes an idle space after its configured period. It
  keeps addresses in memory for rate limits; the reverse proxy or load balancer logs whatever its
  operator configures.
- <StatusTag status="available" /> **Registration code** (from version 0.15.0): for a relay that serves registered sync
  spaces only, the code you enter goes to the relay with the requests; the relay keeps only its
  hash and takes it once. The code stays encrypted with the storage settings on the device you
  entered it on, and goes to no other device.

## On Android

<StatusTag status="available" /> Available from version 0.7.0.

The Android app keeps the vault and its settings in its private folder, which Android's backup and
device transfer leave out, and the key for **Unlock with fingerprint** in the phone's secure
hardware. It sends what a computer sends, with one difference: it checks for updates only when you
tap **Check for updates**, and downloads nothing itself, since a newer release's page opens in
your browser.

## The clipboard and the screen

Copied codes and passwords go to the system clipboard, where other apps can read them until Lockra
clears it. Secrets and export codes appear on screen only after the master password (or, on the
phone, the fingerprint), and so do the passwords of logins set to **Verify first**; see
[How Lockra protects your accounts](/security/).

## Contact

Lockra is developed by sunerpy, who also publishes it on Google Play. For a question about privacy,
open an issue on [GitHub](https://github.com/sunerpy/lockra/issues) or write to
nkuzhangshn@gmail.com.
