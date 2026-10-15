# The browser extension

This page explains how to fill passwords and codes in Chrome, Edge and Firefox with the Lockra
extension: installing and pairing it, filling and saving, and the settings that decide what it may
do.

<StatusTag status="available" /> Available from version 0.10.0, on Windows, macOS and Linux. The
extension itself comes from your browser's store: see
[Installing the extension](#installing-the-extension) for where each store stands.

The extension keeps no passwords of its own. It asks Lockra on the same computer, over a connection
encrypted between the two, and only while Lockra runs and is unlocked; Lockra decides whether a page
may be filled, and with which login. The extension locks and unlocks with Lockra and never asks for
the master password.

## What you need

- Lockra 0.10.0 or later on Windows, macOS or Linux, running on the same computer as the browser.
- Chrome or Microsoft Edge 133 or later, or Firefox 140 or later. Lockra also sets up Chromium and
  Brave, which install the extension from the Chrome Web Store. Browsers installed as a Snap or a
  Flatpak are not supported ([Platform notes](/reference/platforms#browser-extension)).
- Logins in the vault ([Passwords](/passwords/)).

## Installing the extension

<StatusTag status="building" />

| Browser                 | Store                  | Status                          |
| ----------------------- | ---------------------- | ------------------------------- |
| Chrome, Chromium, Brave | Chrome Web Store       | <StatusTag status="building" /> |
| Microsoft Edge          | Microsoft Edge Add-ons | <StatusTag status="building" /> |
| Firefox                 | Firefox Add-ons        | <StatusTag status="building" /> |

The extension is not in the stores yet, and Lockra connects only to the extension the stores
publish. Edge can also install the Chrome Web Store's extension.

## Turning on browser integration

1. In Lockra, open **Settings › Browser** and turn on **Browser integration**. Lockra writes a
   connection file (a native-messaging manifest) for each browser it finds on this computer, so the
   extension can reach it; on Windows, a registry entry under your user account points to each
   file.
2. Lockra offers to keep running after its window closes, since the extension works only while
   Lockra runs: **Keep running** turns that on ([Running in the background](#running-in-the-background)).

**Browsers** then lists each browser and how it stands: **Connected** while a paired extension is
connected, **Manifest written** when one has paired before, and **Extension not installed** when
none has paired yet; Lockra cannot see whether the extension is installed, only whether it has
paired. A browser installed after you turned the integration on shows **Write**: choose it to write
that browser's connection file.

Turning **Browser integration** off removes the connection files and disconnects every extension.

## Pairing the extension with Lockra

1. Click Lockra's icon in the browser's toolbar and choose **Connect**.
2. The extension and Lockra each show a six-digit code; Lockra's window comes to the front with
   "Chrome asks to connect" (or the browser you use). Check that the two codes are the same.
3. Approve in Lockra with the master password, or with Touch ID or Windows Hello where Lockra
   unlocks with it. If the codes differ, choose **Deny**.

A request expires after two minutes, and at most three wait at once. Each browser, and each of its
profiles, pairs on its own. **Paired extensions** in **Settings › Browser** lists them, with when
each was paired and last used; **Unpair** disconnects one at once, and it has to pair again to be
used. The extension's own settings page can unpair as well.

While Lockra is locked, the popup shows **Unlock in Lockra**, which brings Lockra's unlock screen
to the front. While Lockra is not running, it shows **Start Lockra**.

## Filling a sign-in form

Lockra offers the logins that match the page most closely ([Sites and matching](/passwords/#sites-and-matching)),
favorites first, then the ones used most recently.

### From the popup

Click Lockra's icon in the toolbar, or press `Ctrl Shift Y` (`⌘ Shift Y` on macOS), to open the
popup. It lists the site's logins; **Fill** fills one into the page, and ⋯ copies its user name,
password or code. **Search all logins…** finds any login the extension may see: those that match
the page can be filled, the others only copied.

### With the shortcut

Click into the sign-in form and press `Ctrl Shift L` (`⌘ Shift L` on macOS): the first login for the
site is filled into the form. Press it again within 10 seconds to fill the next account instead.
When there is nothing to fill, or Lockra is locked or not paired, the popup opens and says why. The
browser's own shortcut settings change both keys (in Chrome `chrome://extensions/shortcuts`, in
Edge `edge://extensions/shortcuts`, in Firefox the add-ons manager's **Manage Extension
Shortcuts**).

### From the menu by the field

With [site access](#site-access), Lockra's menu opens under a sign-in field when you click into it:
the site's logins, chosen with the arrow keys and Enter, or a click; Esc closes it. Lockra's mark at
the end of the field opens it again.

Whichever way you fill, only the visible fields of the form you are in are filled, and the custom
fields of the login that name a page field. A form is submitted after filling only if you turned
that on.

### When a fill asks first

- **A login set to Verify first** waits for you in Lockra: its window comes to the front with the
  site and the login, and fills after the master password, Touch ID or Windows Hello. A request
  nobody answers is denied after 60 seconds.
- **Confirm before filling** (below) asks in the page before the menu or the shortcut fills:
  "Fill …?", then **Fill**.
- **An HTTP page** for a login saved with an `https://` address says "This page is not encrypted
  (HTTP)": a password filled there may be seen by others on the same network. **Fill anyway** fills
  it.
- **A form in a frame of another site** is never filled by default. With **Ask first**, Lockra shows
  the frame's site and the page's, and fills only after **Fill anyway**.

## The code on the next page

When a login has a linked code, the extension fills the code after the password: into the code
field of the same page, or of the next page of the same site within five minutes, when a sign-in
asks for it in a second step. Lockra recognises fields marked for one-time codes, fields named like
one, and a code split into one box per digit. The code also goes to the clipboard, cleared after the
time set in **Settings › Security › Clear clipboard**. **Fill in codes** and **Copy codes** turn
either off ([What the extension may do](#what-the-extension-may-do)). The next page needs site
access.

## Saving and updating passwords

With site access, the extension notices a form you submit:

- A user name and password Lockra does not have bring up **Save to Lockra?** at the top right of
  the page, with the title, the user name, the password and a group to check. **Save** adds the
  login for this site, **Not now** forgets it, and **Never for this site** adds the site to **Never
  offer to save**.
- A known user name with a new password brings up **Update the password in Lockra?**. **Update**
  changes the login's password, and the old one goes into its history.
- A sign-in in two steps, the user name first and the password on the next page, is put together:
  the extension keeps the user name in memory for five minutes.
- On a sign-up form, the menu by the password field offers **Use a generated password**, made to
  the site's password rules when the page or Lockra knows them. **Use** fills the password fields
  and adds the login to Lockra at once.

Nothing is offered on a site of the **Never offer to save** list, or for a login that is saved
already.

## Site access

Without access to the sites you visit, the extension runs in a page only when you use it: the
popup and the shortcut work, nothing else. **Turn on in web pages** in the popup, or **Grant
access** on the extension's settings page, asks the browser for that access. With it, the extension
adds the menu by sign-in fields, the save and update prompts, the generated passwords of sign-up
forms, the code on the next page and, if you turn it on, filling as pages load.

The browser's own extension settings, or **Remove access** on the extension's settings page, take
it back. **Settings › Browser** in Lockra notes a paired extension that has no site access.

## What the extension may do

**Settings › Browser › Autofill** decides what every paired extension may do on this computer:

| Setting                                    | Default    | What it does                                              |
| ------------------------------------------ | ---------- | --------------------------------------------------------- |
| **Fill in codes**                          | On         | Fill a login's linked code into the page's code field     |
| **Copy codes**                             | On         | Put that code on the clipboard as well                    |
| **Submit after filling**                   | Off        | Submit the form once it is filled                         |
| **Confirm before filling**                 | Off        | Confirm in the page each time before filling              |
| **Sign-in forms in frames of other sites** | Never fill | **Never fill**, or **Ask first** in Lockra                |
| **Fill as pages load**                     | Off        | Fill matching pages as they open, without a click (below) |

A change that lets the extension do more asks for the master password, Touch ID or Windows Hello:
turning on **Fill in codes**, **Submit after filling** or **Fill as pages load**, turning off
**Confirm before filling**, or choosing **Ask first**. These settings are kept in the vault, on this
device only. Each login can set **Fill as the page loads**, **Submit after filling** and **Fill the
code** for itself ([Protection levels](/passwords/#protection-levels)).

### Filling as pages load

**Fill as pages load** fills a login into a matching page as it opens, without a click. Turning it
on first shows its risk: with the default matching by domain, a single malicious script on any
subdomain of the site can read the filled password without your knowing, and with this on it needs
no click from you. Lockra then fills only when all of these hold:

- the form is in the page itself, not in a frame;
- the page uses HTTPS, or the scheme the login was saved with;
- exactly one login matches most closely;
- the login is set to **Fill directly** and has not turned this off for itself;
- the site is not on the **Never fill** list;
- the fields are visible.

Even then, codes are never filled and forms never submitted. It needs site access, and only
**Continue…** and the master password, Touch ID or Windows Hello turn it on.

## Site lists

**Site lists** in **Settings › Browser** holds two lists: **Never offer to save**, where the
extension offers no save or update prompt, and **Never fill**, where it fills nothing. **Add a
site…** adds one; a site covers its subdomains. The lists are kept in the vault and sync to your
other devices.

## Recent activity

**Recent activity** lists what the extensions did and what Lockra refused: pairings, fills, codes,
copies, saves and updates, with the time, the browser and the site. It is kept in memory only, at
most 100 entries, and cleared as Lockra locks; **Clear** empties it.

An extension that asks too often is paused: more than 10 passwords or codes in a minute, more than
120 requests in a minute, or the passwords of more than 20 sites in ten minutes. While paused it
gets no password, and **Settings › Browser** shows "… is paused" with **Resume**: resume it once you
are sure the requests were yours.

## Running in the background

**Settings › Browser › Keep running after the window closes** keeps Lockra running in the tray when
you close its window, so the extension can still reach it. The tray icon is there whenever Lockra
runs ([The tray icon](/guide/quick-start#the-tray-icon)): a click on it opens the window, and on
macOS so does Lockra's icon in the Dock; its menu has **Open Lockra**, **Quick search**
([Quick search from anywhere](/passwords/#quick-search-from-anywhere)), **Lock**, **Settings…**,
**Check for updates…** and **Quit Lockra**. A pairing or a fill that waits for you brings the
window up by itself. With the setting off, closing the window quits Lockra, as before.

While Lockra runs in the background, the vault stays unlocked until it locks by itself after the
idle time in **Settings › Security**, or until you lock it from the tray. Some Linux desktops show
no tray icons, GNOME among them unless an AppIndicator extension is installed: there nothing shows
that Lockra still runs, and starting Lockra again brings its window back.

**Start Lockra** in the extension starts Lockra in the tray while this setting is on, and with its
window otherwise.

## What the extension can and cannot see

- **No vault.** The extension keeps no logins. A login's password reaches it only when you fill or
  copy it, and for filling only from a login that matches the page. It never sees a login set to
  **Not for the extension**, not even in its search.
- **What stays in the browser.** Its own key and the pairing, in your browser profile; while you use
  it, the site's logins as it last listed them (titles and user names, no password), in the
  browser's memory until Lockra locks.
- **No network.** It talks only to Lockra on the same computer, through the browser's native
  messaging, encrypted end to end; it sends nothing anywhere else and collects nothing.
- **The page cannot pretend.** Lockra checks the addresses the browser reports for the page and for
  the frame the form is in, never what the page says about itself, and a page cannot click Lockra's
  menu for you: a click counts only when the menu has been visible, uncovered and still for at least
  300 milliseconds.
- **No say in the rules.** The extension cannot unlock Lockra, and cannot let itself do more:
  only **Settings › Browser** in Lockra can.

What it cannot protect against is in [How Lockra protects your accounts](/security/#what-lockra-cannot-protect-against).
