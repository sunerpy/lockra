# Importing and exporting logins

This page explains how to bring logins from a browser or another password manager into Lockra,
and how to export them for another app.

<StatusTag status="available" /> Available from version 0.10.0.

## What Lockra reads

Export your passwords from the other app first, then import the file it writes:

| App                       | The file                                                   |
| ------------------------- | ---------------------------------------------------------- |
| Chrome, Edge              | the CSV file of the browser's password export              |
| Firefox                   | the CSV file of its password export                        |
| Safari, the Passwords app | the CSV file of its password export, codes included        |
| Bitwarden                 | a CSV export, or a JSON export that is not encrypted       |
| 1Password                 | a 1PUX export, or a CSV export                             |
| LastPass                  | a CSV export                                               |
| KeePass, KeePassXC        | a KDBX 4 database, with its password, its key file or both |

Lockra recognises each file by its content, whatever its name. An encrypted Bitwarden export is
listed as unsupported, with the advice to export unencrypted JSON instead, and a KeePass database
older than KDBX 4 cannot be read.

## Importing

On a computer, open **Import** and, under **Browsers and password managers**, choose **Choose an
export…**, or drop the file on the window; **Add login › Import from another app…** on the
Passwords page leads to the same place. On the phone, choose **Add › Import from files**. Nothing
is saved until you confirm the preview.

A KeePass database first asks for its password ("… needs the KeePass database's password").
**Choose a key file…** adds its key file; for a database protected by a key file alone, leave the
password empty. Then choose **Open database**. A wrong password says "Wrong password", and with a
key file, "The key file is not valid, or does not go with this password".

### The preview

The preview lists every login and every code it found, with its type (**Login** or **Code**) and,
for a login, whether a code comes with it. Each login is compared with the vault by its site, user
name and password:

| Status                        | When                                                                                       | By default                                                  |
| ----------------------------- | ------------------------------------------------------------------------------------------ | ----------------------------------------------------------- |
| **New**                       | nothing like it is in the vault                                                            | added                                                       |
| **Already here**              | the vault has the same site, user name and password, and the same code or none in the file | skipped                                                     |
| **Duplicate**                 | an earlier login of the same file is the same                                              | skipped                                                     |
| **Differs from this vault's** | the vault has the same site and user name with another password or another code            | replaced; a replaced password goes into the login's history |

**Add**, **Skip** or **Replace existing** changes what happens to each one; **Add** keeps both
logins. Codes are compared as accounts are ([Already in the vault](/accounts/add#already-in-the-vault)).

### What comes along

- The title, the user name, the password, the notes, and the sites, each matched by domain unless
  the export says otherwise (Bitwarden's own matching is kept).
- The folder, collection or KeePass group, as the login's group (a nested one as a path, such as
  `Work/Servers`), the favorites, the tags, and the custom fields, the hidden ones still hidden.
- Bitwarden's master password re-prompt, as **Verify first** ([Protection levels](/passwords/#protection-levels)).
- A login's code, as an account on the codes page linked to the login. When the vault already has
  an account with the same secret, the login is linked to it instead.

The detail pane names the app a login came from under **Source**. Payment cards, identities, secure
notes and other kinds of items are listed as unsupported, with the reason, and so is a login larger
than Lockra keeps (a value over 4 KiB, notes over 64 KiB, or more than 64 fields). KeePass entries
in the recycle bin are left out.

The file you imported holds every password in plain text, except a KeePass database: delete it once
the import is done.

## Exporting

Open **Export** (on the phone, **Settings › Export**) and choose **Logins** under **What to
export**. Every login is ticked under **Choose logins**; untick those to leave out. Then choose
**Export as**:

- **KeePass database**: an encrypted KDBX 4 file that KeePassXC, KeePass, Strongbox and others open.
  Choose its password, at least 8 characters, in **Database password** and **Repeat it**. The codes
  go into each entry's `otp` field.
- **Plain CSV file**: a table in Bitwarden's format, which most password managers import. It is
  **not encrypted**: anyone who gets the file reads every password in it. Lockra saves it only after
  you turn on "I know this file is not encrypted and anyone can read the passwords in it".

Enter the master password (on a phone with fingerprint unlock on, leave it empty to use the
fingerprint), then choose **Save database…** or **Save CSV file…** and a place for the file. On
Linux and macOS, the file can be read by your user account only. Delete a CSV file once the other
app has imported it.

Not exported: how each site matches (an export imported back matches by domain), the password
history, the dates, the colours and the autofill choices; a KeePass database also leaves out the
favorites and **Verify first**, and a CSV file the tags. A login written by a newer version of
Lockra is not exported.

To keep your logins safe rather than move them, use a [backup](/backup/): it holds the logins too,
encrypted.
