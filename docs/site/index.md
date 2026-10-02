---
layout: home
title: Lockra — an encrypted two-factor authenticator for Windows, macOS and Linux
titleTemplate: false
description: Lockra keeps your two-factor codes in one encrypted file on your computer. Move accounts from and to Google Authenticator and Microsoft Authenticator, keep encrypted backups in a folder you choose, and sync your devices through storage of your own.

hero:
  name: Lockra
  text: Two-factor codes that stay on your devices.
  tagline: An encrypted authenticator for Windows, macOS and Linux. Move your accounts from Google Authenticator or Microsoft Authenticator, keep encrypted backups where you choose, and sync your devices through storage of your own if you want to.
  actions:
    - theme: brand
      text: Download
      link: /guide/install
    - theme: alt
      text: Quick start
      link: /guide/quick-start
    - theme: alt
      text: GitHub
      link: https://github.com/sunerpy/lockra

home:
  facts:
    - term: Runs on
      text: Windows 10 and 11, macOS 11 or later (Apple silicon and Intel), and Linux, on x64 and ARM64.
    - term: Your accounts
      text: In one encrypted file on each device. Lockra connects to GitHub for updates and, once you turn on sync, to your own storage; to nothing else.

  visual:
    home:
      light: /screens/codes-en-light.webp
      dark: /screens/codes-en-dark.webp
      width: 1440
      height: 900
      alt: The Lockra codes page with a list of accounts, each with its current code and the time it has left.

  index:
    title: What Lockra does
    intro: Everything in the current release, with a link to the page that explains it.
    groups:
      - name: Codes
        items:
          - title: One click copies a code
            body: Click an account, or select it with the arrow keys and press Enter. The clipboard is cleared 30 seconds later if it still holds the code.
            status: available
            link: /accounts/codes
          - title: The next code in time
            body: In a code's last five seconds the next one appears beside it, so you never type a code that is about to change.
            status: available
            link: /accounts/codes#reading-a-code
          - title: Search, groups and pinned accounts
            body: Type to find an account, file accounts into groups, and pin the ones you use most to the top. Ctrl K finds any account and copies its code.
            status: available
            link: /accounts/codes#finding-an-account
          - title: Folding groups and account colours
            body: Fold groups one by one or all at once, give each account a colour or its own avatar text, and pin or edit from the row or a right-click.
            status: building
            link: /accounts/codes#folding-groups
          - title: Every standard account
            body: Time-based (TOTP) and counter-based (HOTP) codes, SHA1, SHA256 or SHA512, 6 to 8 digits, any period.
            status: available
            link: /accounts/add
      - name: Moving accounts
        items:
          - title: From Google Authenticator
            body: Photograph or screenshot its export codes and drop the pictures on the window. Lockra says which codes of a batch are still missing.
            status: available
            link: /transfer/google
          - title: From Microsoft Authenticator
            body: The app has no export, but Lockra reads its database from a rooted Android phone. Work and school accounts cannot be moved.
            status: available
            link: /transfer/microsoft
          - title: Links, lists and QR images
            body: Paste otpauth links, read them from the clipboard, or choose a list or a picture of any authenticator's QR code.
            status: available
            link: /transfer/other-apps
          - title: Back to a phone
            body: Migration codes for Google Authenticator, one code per account for Microsoft Authenticator, with the current codes beside them to check the phone.
            status: available
            link: /transfer/google#moving-accounts-to-google-authenticator
      - name: Backups
        items:
          - title: Encrypted backup files
            body: Save the whole vault to a .lockrabackup file under the master password or a separate backup password.
            status: available
            link: /backup/
          - title: Automatic backups
            body: A few seconds after every change, an encrypted backup goes to a folder you choose. Lockra keeps the newest few.
            status: available
            link: /backup/#automatic-backups
          - title: Restore, merged or in full
            body: Merge a backup's accounts one by one, or replace them all. Before replacing, Lockra keeps a copy of the current vault.
            status: available
            link: /backup/#restoring-a-backup
          - title: Sync between your devices
            body: End-to-end encrypted, through an S3-compatible bucket or a WebDAV folder of your own. Lockra runs no server, and the storage sees only encrypted files.
            status: available
            link: /backup/sync
      - name: Protection
        items:
          - title: A master password
            body: The vault is encrypted with a key derived from your master password. Repeated wrong passwords slow down further attempts.
            status: available
            link: /security/
          - title: Remember on this device
            body: Optionally keep a key in the system keychain so that unlocking needs no password on this computer.
            status: available
            link: /security/#remember-on-this-device
          - title: Touch ID and Windows Hello
            body: Ask for your fingerprint, or Windows Hello, before the remembered key unlocks.
            status: building
            link: /security/#touch-id-and-windows-hello
          - title: Locks itself
            body: After five idle minutes by default, and at once with Ctrl L.
            status: available
            link: /security/#locking
          - title: Secrets out of sight
            body: Showing a secret or an export code asks for the master password again and hides it after two minutes. On Windows and macOS, screenshots show the window black meanwhile.
            status: available
            link: /security/#secrets-on-screen
          - title: Signed updates
            body: The title bar tells you when a new version is out, or downloads it at start if you turn on automatic updates, and Lockra installs only a package that carries its signature.
            status: available
            link: /guide/updates#updating

  steps:
    title: Move your accounts in four steps
    items:
      - title: Create the vault
        body: Start Lockra and choose a master password. It encrypts everything and cannot be recovered.
      - title: Export from the phone
        body: In Google Authenticator, open Transfer accounts and Export accounts, and take photos of the codes with another device.
      - title: Import the photos
        body: Drop the pictures on Lockra's window. A preview lists every account; confirm it and they are saved.
      - title: Copy a code
        keys: [Ctrl, K]
        body: Find an account and press Enter, or click it in the list. The code is on the clipboard.

  transfer:
    columns: [App, Into Lockra, Back to the app]
    rows:
      - name: Google Authenticator
        into: Photos or screenshots of its export codes
        out: Migration codes, up to 10 accounts each
        note: 30-second or counter-based, 6 or 8 digits
      - name: Microsoft Authenticator
        into: Its database from a rooted Android phone
        out: One standard code per account
        note: SHA1, 6 digits, 30 seconds
      - name: Other authenticators
        into: otpauth links, lists and QR images
        out: A plain list of otpauth links
      - name: Lockra
        into: A backup file and its password
        out: A backup file
    caption: Before anything is saved, the preview marks each account as new, already in the vault, sharing a name with a different account, or unsupported, with the reason.

  backup:
    items:
      - title: Encrypted like the vault
        body: A backup file can sit in any folder or cloud drive. It opens only with the password it was made under.
      - title: Automatic
        body: Three seconds after the last change, into the folder you chose, keeping the newest 10 by default.
      - title: Safe to restore
        body: Merge goes through the import preview; replace first saves a copy of the current vault.

  security:
    items:
      - title: Key derivation
        body: Argon2id with 64 MiB of memory and three passes turns the master password into a key.
      - title: Encryption
        body: XChaCha20-Poly1305 encrypts the accounts; any change to the file, even one byte, is detected.
      - title: The clipboard
        body: Copied codes are marked to stay out of clipboard history and are cleared after 30 seconds.
      - title: Your OS account
        body: With Remember on this device, the vault opens for anyone who can sign in to this computer.

  platforms:
    title: Platforms
    intro: Lockra is the same app on all three systems. The platform notes list what differs between them.
    columns: [Platform, Packages, Remember on this device, Blocks screenshots of secrets]
    rows:
      - name: Windows 10 and 11
        status: available
        cells:
          - Installer or MSI for x64, installer for ARM64
          - Credential Manager
          - "Yes"
      - name: macOS 11 or later
        status: available
        cells:
          - A dmg for Apple silicon and one for Intel
          - Keychain
          - "Yes"
      - name: Linux
        status: available
        cells:
          - .deb, .rpm or AppImage, x64 and ARM64
          - Secret Service (GNOME Keyring, KWallet)
          - "No"
    note: The packages are not code-signed yet, so Windows and macOS ask before the first start.

  privacy:
    title: What leaves your computer
    intro: Nothing, unless you move it yourself or turn on sync.
    label: Where it is
    items:
      - name: Your accounts
        value: In one file on this computer
        detail: Encrypted with a key derived from your master password. Lockra never sends it anywhere.
      - name: Backups
        value: Where you save them
        detail: Encrypted the same way. Automatic backups go only to the folder you chose; if that folder syncs, the encrypted file syncs with it.
      - name: Sync
        value: Your own storage, if you turn it on
        detail: Each device writes encrypted files to the bucket or WebDAV folder you set up. The storage cannot read an account, a secret or a device name.
      - name: The network
        value: Updates and your sync
        detail: Lockra asks GitHub for the newest version when you check, or at start if you turn on automatic updates; nothing from your accounts goes with it. There is no Lockra account, server or telemetry, and fonts and images ship with the app.

  scope:
    title: Deliberately left out
    intro: Lockra does one thing, on your devices and your storage.
    items:
      - A sync server or an account. Devices sync through storage you choose.
      - A browser extension or filling in codes. Copy a code and paste it.
      - Website icons. Accounts show their initial; fetching icons would mean going online.
      - Non-standard codes such as Steam Guard.
---

<HomeIndex />

<HomeSteps />

<SplitBlock proof="transfer">

## Bring your accounts, and take them back

Google Authenticator exports its accounts as QR codes that Lockra reads from photos or screenshots, several at once. Microsoft Authenticator has no export; on a rooted Android phone, Lockra reads its database instead. Any other authenticator can hand over otpauth links or QR codes.

Moving the other way, Lockra shows migration codes for Google Authenticator and one code per account for Microsoft Authenticator, with each account's current code beside them so you can check the phone.

[Google Authenticator](/transfer/google) · [Microsoft Authenticator](/transfer/microsoft) · [Other apps](/transfer/other-apps)

</SplitBlock>

<SplitBlock proof="backup" flip>

## Backups you never have to remember

Turn on automatic backups once and choose a folder. A few seconds after each change Lockra writes an encrypted copy there and keeps only the newest few. A folder of OneDrive, Google Drive or iCloud carries the copies to your other devices without Lockra going online, and sync keeps the accounts themselves the same on every device, through storage of your own.

Restoring merges a backup into your accounts through the same preview as an import, or replaces them after saving a copy of the vault you had.

[Backups and restore](/backup/) · [Sync between devices](/backup/sync)

</SplitBlock>

<SplitBlock proof="security">

## Built to keep secrets

The vault is one encrypted file. Without the master password it is unreadable, and changing a single byte of it is detected. Lockra locks itself when idle, clears the codes it copied, and shows a secret only after you enter the master password again.

[How Lockra protects your accounts](/security/) · [Privacy](/privacy)

</SplitBlock>

<HomePlatforms />

<HomePrivacy />

## Install

Packages for every platform, with checksums and build attestations, are on the [releases page](https://github.com/sunerpy/lockra/releases). The [install guide](/guide/install) covers each platform, including the first start of an unsigned app.

<HomeScope />
