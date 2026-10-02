# Roadmap

This page describes what Lockra covers today, what is planned, and what it deliberately leaves out.

## Today

Lockra covers what an authenticator on a computer needs: codes for every standard account, moving
accounts from and to Google Authenticator and Microsoft Authenticator, otpauth links and lists,
encrypted and automatic backups, end-to-end encrypted sync between your devices through storage of
your own, protection for the vault, the clipboard and the screen, and signed in-app updates with a
one-line install for every platform. Suggestions are welcome in the
[issue tracker](https://github.com/sunerpy/lockra/issues).

## Planned

- **An Android app** <StatusTag status="planned" />: the same vault on the phone, scanning QR codes
  with the camera or from screenshots, unlocking with a fingerprint, and syncing with your other
  devices.

## Deliberately left out

- **A sync server.** Devices sync through storage you choose; Lockra runs none and keeps no
  account.
- **A browser extension or filling in codes.** Copy a code and paste it.
- **Website icons.** Accounts show their initial; fetching icons would mean going online.
- **Non-standard codes** such as Steam Guard.
- **Importing from Microsoft Authenticator on an iPhone.** There is no known way to read its
  accounts.
