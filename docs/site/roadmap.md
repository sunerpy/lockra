# Roadmap

This page describes what Lockra covers today, what is being built, and what it deliberately leaves
out.

## Today

Lockra covers what an authenticator on a computer needs: codes for every standard account, moving
accounts from and to Google Authenticator and Microsoft Authenticator, otpauth links and lists,
encrypted and automatic backups, end-to-end encrypted [sync between devices](/backup/sync) through
storage of your own or a Lockra relay (the built-in one, or
[one you run yourself](/backup/relay)), protection for the vault (with Touch ID or Windows Hello where the computer
has it), the clipboard and the screen, and signed in-app updates with a one-line install for every
platform. From version 0.7.0 there is an [Android app](/reference/platforms#android) too: the same
vault on the phone, scanning QR codes with the camera or from screenshots, unlocking with a
fingerprint, and syncing with your other devices. Suggestions are welcome in the
[issue tracker](https://github.com/sunerpy/lockra/issues).

## Deliberately left out

- **A sync account, or a server that can read your accounts.** Sync needs no account, and what
  keeps the space, your storage or a relay, holds only encrypted files.
- **A browser extension or filling in codes.** Copy a code and paste it.
- **Website icons.** Accounts show their initial; fetching icons would mean going online.
- **Non-standard codes** such as Steam Guard.
- **Importing from Microsoft Authenticator on an iPhone.** There is no known way to read its
  accounts.
