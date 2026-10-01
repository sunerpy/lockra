# Roadmap

This page describes what Lockra covers today and what it deliberately leaves out.

## Today

Lockra covers what an authenticator on a computer needs: codes for every standard account, moving
accounts from and to Google Authenticator and Microsoft Authenticator, otpauth links and lists,
encrypted and automatic backups, protection for the vault, the clipboard and the screen, and signed
in-app updates with a one-line install for every platform. No new
features are scheduled; suggestions are welcome in the
[issue tracker](https://github.com/sunerpy/lockra/issues).

## Deliberately left out

- **Syncing between devices.** Lockra keeps the vault on one computer and syncs nothing; move
  accounts with an export or a backup.
- **Phone apps.** Lockra exports to the authenticator on your phone instead.
- **A browser extension or filling in codes.** Copy a code and paste it.
- **Website icons.** Accounts show their initial; fetching icons would mean going online.
- **Non-standard codes** such as Steam Guard.
- **Importing from Microsoft Authenticator on an iPhone.** There is no known way to read its
  accounts.
