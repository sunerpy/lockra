# Microsoft Authenticator

This page explains what can be moved from Microsoft Authenticator into Lockra, and how to add
Lockra's accounts to Microsoft Authenticator.

## Moving accounts into Lockra

Microsoft Authenticator has no export: its cloud backup restores only into Microsoft
Authenticator. The accounts can be read only from the app's database, and only on an Android phone
with root access.

1. On the rooted phone, copy both files from
   `/data/data/com.azure.authenticator/databases/`: `PhoneFactor` and `PhoneFactor-wal`. The second
   one holds recent changes; without it, accounts added lately are missing.
2. Move the files to the computer.
3. Open **Import**, choose **Choose PhoneFactor files…** under Microsoft Authenticator and select
   both files.
4. Check the preview and choose **Import**.

Lockra reads copies of the files and leaves the originals untouched.

### What comes across

| Account                                                       | Result                                                                                                                             |
| ------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Other services added with a QR code (GitHub, Google, Amazon…) | imported: 6 digits, 30 seconds                                                                                                     |
| Personal Microsoft account                                    | imported: 8 digits, 30 seconds                                                                                                     |
| Work or school account                                        | not imported: Microsoft keeps no secret on the phone that another app could use; set up the account again in the new authenticator |

Newer versions of Microsoft Authenticator may keep the secrets encrypted on the phone. Lockra then
lists those accounts as unsupported, with the reason; set them up again at each service instead.

There is no known way to read the accounts from Microsoft Authenticator on an iPhone.

## Adding accounts to Microsoft Authenticator

Microsoft Authenticator imports one account per QR code.

1. Open **Export** and choose **Microsoft Authenticator**.
2. Tick the accounts. Microsoft Authenticator takes only accounts with SHA1, 6 digits and a
   30-second period; the others are greyed out, with the reason.
3. Enter the master password and choose **Show QR codes**. On a phone with fingerprint unlock
   on, leave the master password empty and verify your fingerprint instead.
4. On the phone, tap **+ › Other account (Google, Facebook, etc.)** and scan the first code; use
   **Next** in Lockra for the following ones.

Check that the code the phone shows matches the one Lockra shows beside each QR code.
