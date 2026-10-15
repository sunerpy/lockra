# Questions and troubleshooting

This page answers common questions and explains what to do when something does not work.

## I forgot my master password

It cannot be recovered, by anyone: the vault is encrypted with it. If you have a backup made under
a password you remember, choose **Forgot the master password?** on the unlock screen, confirm the
reset, then restore the backup. Otherwise the accounts must be set up again at each service, using
the recovery codes the services gave you.

## The codes are rejected by a service

Codes depend on the time. Check that the device's clock is set automatically; a clock a minute
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

Always: codes are calculated on the device from each account's secret and the time, with no
network. Lockra goes online for updates (when you check, or at start after you turn on automatic
updates) and, from version 0.4.0, for sync, once you set it up on storage of your own or a relay
([Sync between devices](/backup/sync)). Without the network, sync waits and the codes work as
before.

## Where are my accounts stored?

In one encrypted file on each device, with your logins. See [Privacy](/privacy) and
[Updates, uninstalling and your data](/guide/updates#where-the-files-are).

## The browser extension says Lockra is not running

The extension needs Lockra running on the same computer. Choose **Start Lockra** in its popup, and
turn on **Settings › Browser › Keep running after the window closes**, so that closing Lockra's
window leaves it in the tray ([Running in the background](/passwords/browser#running-in-the-background)).

## The browser extension cannot find Lockra

The popup says "Lockra not found" when the browser has no connection file for Lockra: turn on
**Settings › Browser › Browser integration**. For a browser installed after that, choose **Write**
in its row under **Browsers**. A browser installed as a Snap or a Flatpak cannot reach Lockra
([Platform notes](/reference/platforms#browser-extension)).

## The fill shortcut does nothing

Check the shortcut on the browser's own page: `chrome://extensions/shortcuts` in Chrome,
`edge://extensions/shortcuts` in Edge, **Manage Extension Shortcuts** in Firefox's add-ons manager.
A browser may leave `Ctrl Shift L` unassigned, for example when another extension uses it. Lockra
fills only a login whose site matches the page; when none does, the popup opens and says so.

## Should I turn on filling as pages load?

It is off by default for a reason. A login matched by its domain is offered on every subdomain of
the site, and with filling as pages load, a malicious script on any of them can read the password
without a click from you. Lockra then fills only under the conditions in
[Filling as pages load](/passwords/browser#filling-as-pages-load). Leave it off unless you trust
every page of the sites you keep logins for; the shortcut fills with one key instead.

## Can the phone fill in passwords?

No. On the phone, open a login and copy its password, then paste it; the browser extension works on
computers only. Filling on Android is planned ([Roadmap](/roadmap#more-for-passwords)).

## Is there an iPhone app?

Not yet. Lockra runs on Windows, macOS, Linux and Android phones; an app for iPhone and iPad is
planned ([Roadmap](/roadmap#planned)). Until then, Lockra on a computer can show export codes for
Google Authenticator or Microsoft Authenticator on the iPhone
([Moving accounts to Google Authenticator](/transfer/google#moving-accounts-to-google-authenticator)).
