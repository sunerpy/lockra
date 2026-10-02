# What is Lockra

This page explains what Lockra is for and how it differs from the authenticator on your phone.

Lockra is a two-factor authenticator for the computer. When a website asks for a six-digit code
after your password, Lockra shows it, and one click copies it. It works with every service that
offers an authenticator app: the codes follow the open TOTP and HOTP standards, the same ones
Google Authenticator and Microsoft Authenticator use.

## Where your accounts are kept

Lockra keeps all your accounts in one file on your computer, encrypted with a key derived from a
master password that only you know. Lockra goes online for its own updates
([Updating](/guide/updates#updating)) and, from version 0.4.0 if you turn it on, to sync your
devices through storage of your own ([Sync between devices](/backup/sync)): there is no account to create and nothing is
reported back. The codes are calculated on your computer from the secret each service gave you and
the current time.

## Moving accounts in and out

Most people already have their accounts in a phone app. Lockra reads the export codes of Google
Authenticator from photos or screenshots, reads Microsoft Authenticator's database from a rooted
Android phone, and accepts otpauth links and QR images from any other app. It can also move the
accounts back to a phone, so the computer and the phone can hold the same accounts.

## Backups

A lost or broken computer would take the accounts with it, so Lockra writes encrypted backups to a
folder you choose, automatically after each change. A backup restores on any computer with Lockra
and the password it was made under.

## What Lockra does not do

Lockra runs no sync server, has no browser extension, and does not fill in codes for you. These
are deliberate: each would mean sending your accounts somewhere or reaching into other apps. See
[Roadmap](/roadmap) for the full list and what is planned.

## Next steps

- [Install Lockra](/guide/install)
- [Quick start](/guide/quick-start)
