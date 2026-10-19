# Roadmap

This page describes what Lockra covers today, what is planned, and what it deliberately leaves out.

## Today

Lockra covers what an authenticator needs on your computers and your phone: codes for every standard
account, moving accounts from and to Google Authenticator and Microsoft Authenticator, otpauth links
and lists, encrypted and automatic backups, end-to-end encrypted [sync between
devices](/backup/sync) through storage of your own or [a Lockra relay you run](/backup/relay), protection for the vault (with Touch ID or Windows Hello where the
computer has it), the clipboard and the screen, and signed in-app updates with a one-line install
for every platform. From version 0.7.0 there is an [Android app](/reference/platforms#android) too:
the same vault on the phone, scanning QR codes with the camera or from screenshots, unlocking with a
fingerprint, and syncing with your other devices. Suggestions are welcome in the [issue
tracker](https://github.com/sunerpy/lockra/issues).

### Passwords

<StatusTag status="available" /> Available from version 0.10.0.

Lockra also keeps passwords, in the same vault as the codes ([Passwords](/passwords/)):

- logins with their user name and email address, sites, notes, custom fields, tags and groups, a
  linked code, a password history and a protection level of their own, on the computer and on the
  phone;
- a password generator, for random passwords and passphrases, that follows the password rules of
  known sites;
- importing from Chrome, Edge, Firefox, Safari, Bitwarden, 1Password, LastPass and KeePass, and
  exporting to KeePass or a plain CSV file ([Importing and exporting logins](/passwords/import-export));
- a [browser extension](/passwords/browser) for Chrome, Edge and Firefox that fills logins and their
  codes, and offers to save and update passwords, with Lockra running in the background;
- a key combination of your own that brings up the command palette over any app.

## Planned

### iPhone and iPad

<StatusTag status="planned" />

An app for iPhone and iPad, with the same vault and the same sync as on the other platforms. There
is no date yet.

### More for passwords

<StatusTag status="planned" />

The next steps for passwords, with no date yet:

- **More kinds of items**: payment cards, identities, secure notes, API credentials, Wi-Fi networks
  (with a QR code to join), software licences and ID documents.
- **A security report**, worked out on the device: weak, reused, old and expiring passwords, sites
  still on HTTP, duplicates, sites that offer two-factor sign-in where no code is linked yet, and
  sites that accept passkeys. A check of your passwords against known breaches, through Have I Been
  Pwned's anonymous range search, only if you turn it on: it would be the first connection Lockra
  makes besides updates and sync.
- **A trash and better order**: deleted logins kept for 30 days, an archive, earlier versions of a
  login, your own order of the list, and notes in Markdown.
- **Filling on Android**: logins offered to other apps and the browser on the phone.
- **More**: email aliases from SimpleLogin, addy.io, Firefox Relay, Fastmail or DuckDuckGo, opening
  a login's site (and filling it) from Lockra, website icons if you want them, a printable emergency
  kit, starting with your computer, importing encrypted Bitwarden exports, and the quick search
  shortcut on Wayland without setting it up in the desktop.

### Later

<StatusTag status="planned" />

- Passkeys, in the browser, on Android and on Windows 11, with the new standard for moving them
  between apps.
- Unlocking with a hardware security key.
- Typing a login into any app on the desktop.
- Safari, and browsers installed as a Snap or a Flatpak.
- Attachments.
- Sharing logins with other people, in shared groups or with a link that works once.
- A command line and an SSH agent.
- Several vaults, and a travel mode.

## Deliberately left out

- **A sync account, or a server that can read your accounts.** Sync needs no account, and what
  keeps the space, your storage or a relay, holds only encrypted files.
- **Non-standard codes** such as Steam Guard.
- **Importing from Microsoft Authenticator on an iPhone.** There is no known way to read its
  accounts.
