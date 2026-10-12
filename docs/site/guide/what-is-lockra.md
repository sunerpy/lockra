# What is Lockra

This page explains what Lockra is for and what sets it apart from other authenticators.

Lockra is a two-factor authenticator for your computers and your Android phone. When a website asks
for a six-digit code after your password, Lockra shows it, and one click or tap copies it. It works with every service that
offers an authenticator app: the codes follow the open TOTP and HOTP standards, the same ones
Google Authenticator and Microsoft Authenticator use.

## Where your accounts are kept

Lockra keeps all your accounts in one file on each device, encrypted with a key derived from a
master password that only you know. Lockra goes online for its own updates
([Updating](/guide/updates#updating)) and, from version 0.4.0 if you turn it on, to sync your
devices through storage of your own or, from version 0.8.1, a Lockra relay ([Sync between devices](/backup/sync)): there is no account to create and nothing is
reported back. The codes are calculated on the device from the secret each service gave you and
the current time.

## Moving accounts in and out

Most people already have their accounts in a phone app. Lockra reads the export codes of Google
Authenticator from photos or screenshots, reads Microsoft Authenticator's database from a rooted
Android phone, and accepts otpauth links and QR images from any other app. It can also move the
accounts back to those apps.

## On the phone

From version 0.7.0 Lockra also runs on Android phones with Android 8 or later: the same vault as on
the computer, QR codes scanned with the camera or read from screenshots, unlocking with a
fingerprint, and sync with your computers, which a phone joins by scanning a QR code
([Sync between devices](/backup/sync#adding-a-phone)). An app for iPhone and iPad is planned
([Roadmap](/roadmap#planned)).

## Backups

A lost or broken device would take the accounts with it. On a computer, Lockra writes encrypted
backups to a folder you choose, automatically after each change; on the phone, you save them
through the system's file picker. A backup restores on any device with Lockra and the password it
was made under.

## What Lockra does not do

Lockra has no sync service that can read your accounts (a relay keeps encrypted files only), no
browser extension, and does not fill in codes for you. These are deliberate: each would mean
sending your accounts where they can be read, or reaching into other apps. See [Roadmap](/roadmap)
for the full list and what is planned.

## Next steps

- [Install Lockra](/guide/install)
- [Quick start](/guide/quick-start)
