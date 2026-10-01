# Adding an account

This page explains how to add a single account, from a service's setup page or by hand. To move many
accounts at once, see [Moving accounts](/transfer/google).

## From a QR code or a link

When a service turns on two-factor sign-in, it shows a QR code, and usually a link or a secret
under it.

- **The QR code**: take a screenshot of it, then choose **Add › Import from the clipboard** on the
  codes page. Lockra reads the code from the picture.
- **The link** (`otpauth://…`): copy it and choose **Add › Paste an otpauth link…**, or
  **Import from the clipboard**.

Either way the service then asks for the current code to confirm: copy it from Lockra.

## By hand

Some services show only a secret, a string such as `JBSW Y3DP EHPK 3PXP`. Choose **Add › Enter
manually…** (`Ctrl N`), enter the service and the account name and paste the secret; spaces and
upper or lower case do not matter.

Most services use the defaults: a time-based code, 6 digits, a 30-second period and SHA1. When a
service says otherwise, open **Advanced** and choose:

| Setting   | Values                                    |
| --------- | ----------------------------------------- |
| Type      | time-based (TOTP) or counter-based (HOTP) |
| Period    | 1 to 3600 seconds (TOTP)                  |
| Counter   | the starting value (HOTP)                 |
| Digits    | 6, 7 or 8                                 |
| Algorithm | SHA1, SHA256 or SHA512                    |

## Already in the vault

Adding an account whose secret is already in the vault is refused, so the same account never
appears twice. An account with the same service and name but a different secret is added beside
the existing one: services issue a new secret when you set up the authenticator again.
