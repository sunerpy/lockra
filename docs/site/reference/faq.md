# Questions and troubleshooting

This page answers common questions and explains what to do when something does not work.

## I forgot my master password

It cannot be recovered, by anyone: the vault is encrypted with it. If you have a backup made under
a password you remember, choose **Forgot the master password?** on the unlock screen, confirm the
reset, then restore the backup. Otherwise the accounts must be set up again at each service, using
the recovery codes the services gave you.

## The codes are rejected by a service

Codes depend on the time. Check that the computer's clock is set automatically; a clock a minute
off produces codes the service no longer accepts. If only one account fails, compare its settings
(digits, period, algorithm) with the service's setup page, in **Edit…** and in the import preview.

## Google Authenticator's export screen cannot be captured

The Android app blocks screenshots of its export codes. Photograph the screen with another phone or
a camera and import the photos; a sharp, straight-on picture reads best. On an iPhone, screenshots
work.

## The preview says a code of the Google export is missing

A large export spreads over several codes, numbered on the phone. Import the pictures of all of
them; the preview counts what it has seen and names the missing ones.

## A Microsoft Authenticator account is listed as unsupported

Work and school accounts keep no secret on the phone that another app can use; newer versions of
the app may also encrypt the others. Set those accounts up again at the service with Lockra as the
authenticator.

## Remember on this device is greyed out

No system keychain is available. On Linux, install and start a Secret Service keychain such as
GNOME Keyring or KWallet, then restart Lockra.

## The automatic backup failed

The Backup page shows the error and when it happened. Usually the folder is on a drive that is not
connected, or Lockra may no longer write to it. Reconnect the drive or choose another folder; the
next change, or **Run automatic backup now**, writes a new backup.

## Does Lockra work offline?

Always: codes are calculated on the computer from each account's secret and the time, with no
network. Lockra goes online for updates (when you check, or at start after you turn on automatic
updates) and, from version 0.4.0, for sync, once you set it up on storage of your own
([Sync between devices](/backup/sync)). Without the network, sync waits and the codes work as
before.

## Where are my accounts stored?

In one encrypted file on the computer. See [Privacy](/privacy) and
[Updates, uninstalling and your data](/guide/updates#where-the-files-are).
