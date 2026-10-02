# Sync between devices

This page explains how to keep the same accounts on several devices through storage of your own,
how a new device joins, and how Lockra keeps that storage from reading your accounts.

Sync is off until you set it up. Lockra runs no server and has no account: your devices meet in an
S3-compatible bucket or a WebDAV folder that you choose, and everything they put there is encrypted
before it leaves the device. Sync is part of Lockra from version 0.4.0.

## What you need

- **S3-compatible storage**: a bucket at AWS S3, Cloudflare R2, Backblaze B2, Alibaba Cloud OSS,
  MinIO or another S3-compatible service, with an access key that may list, read, write and delete
  in it. Or **WebDAV**: a folder at Nextcloud, Jianguoyun, a Synology NAS or another WebDAV
  service, with a user name and password (an app password where the service offers one).
- An address that starts with `https://`. Lockra refuses plain `http://`, except for an address
  on the same computer.

## Starting to sync

1. On the first device, open **Settings › Sync** and choose **Start syncing**.
2. Choose **S3-compatible** or **WebDAV** and fill in the storage. For S3: **Endpoint**, **Region**
   (`auto` where the service has none), **Bucket**, **Access key ID** and **Secret access key**;
   turn on **Path-style access** for a self-hosted service such as MinIO. **Folder (optional)** is
   where Lockra keeps its files in the bucket or WebDAV folder (`lockra` by default).
3. Check **This device's name**, enter the master password, and choose **Start syncing**.
4. Lockra shows the **sync key**: `LKS1-` and 14 groups of four characters. Write it down or keep
   it in a password manager, apart from the master password. Adding a device without another one
   at hand, or recovering when every device is lost, takes both. You can see it again under
   **Invite another device**.

The credentials are kept in the encrypted vault and never shown again; **Settings › Sync** shows
the storage without them.

## Adding a device

On a device that syncs, open **Settings › Sync**, choose **Invite another device** and enter the
master password. Lockra shows a QR code and the invitation as text, for two minutes. The
invitation holds the storage's credentials and the sync key, so use it on your own devices only.

On the new device:

- **No vault yet**: on the welcome screen, choose **Join sync…**, paste the invitation, enter a
  name for the device and **The sync space's master password**, and choose **Join**. Lockra
  creates the vault under that master password and the accounts arrive.
- **A vault already**: open **Settings › Sync › Join an existing sync**. The vault's accounts join
  the space, and the vault keeps its own master password.

Without another device, choose **Sync key** instead of **Invitation** and fill in the storage
settings and the sync key.

The space's master password is the one of the device that created the space; after a master
password change on any device of the space, it is the newest one. A change of the master password
reaches the space on the next sync.

## When devices sync

While the vault is unlocked, a device syncs when you unlock it, three seconds after a change,
every five minutes, and when you choose **Sync now**. While the vault is locked, nothing goes out.
The status line shows **Synced** and when, or why the last run failed; the next run tries again.

When the same account was changed on two devices, the later change wins on every device. A
deletion removes the account on every device, unless the account was changed after the deletion.
The order of recently used accounts stays on each device.

## Devices

**Devices** lists every device of the space and when it last wrote. To remove a lost or retired
device, choose its remove button: its data is deleted from the storage. A device that is still in
use appears again on its next sync. **Rename** changes this device's name for the others.

## Changing the storage settings or turning sync off

**Change storage settings** takes a new access key, password or address, and the master password.
The space must already be at the new place: move its folder first.

**Turn off sync on this device** stops sync here. The accounts stay on the device; the space and
the other devices go on. You can join again later with an invitation or the sync key.

## What the storage can see

- **Only encrypted files.** The accounts, their secrets, the device names and the times are
  encrypted on the device with XChaCha20-Poly1305 before they are written. Each device writes one
  file, padded to steps of 4 KiB, so its size says little about the number of accounts. The file
  names show only how many devices there are.
- **Two secrets open the space.** The key that encrypts the files is itself encrypted with both
  the master password (through Argon2id, 64 MiB of memory and three passes) and the sync key.
  Someone with the storage's contents opens nothing without the sync key, however good their guess
  of the master password; with the sync key, every guess still costs a full Argon2id run.
- **Changes are detected.** A file that was altered, moved from another device or space, or put
  back to an older version is refused, and **Settings › Sync** names the device. Your accounts stay
  as they are.
- **Deleting is not prevented.** Whoever can write to the storage can delete the space. That stops
  sync, not your vaults: every device keeps its accounts.
- **Backups leave sync out.** The storage settings, their credentials and the sync key are kept in
  the vault file only. A backup does not contain them; a device restored from a backup joins the
  space again.

## When something goes wrong

| Message                                                                                               | What to do                                                                                                                                     |
| ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| The storage refused access; check the access key or password                                          | Check the access key or the password, and that it may list, read, write and delete in the bucket or folder.                                    |
| The storage could not be reached                                                                      | Check the network and the address. A proxy configured in the system is used.                                                                   |
| The storage answered with an error; check that the bucket or folder exists                            | Check the bucket, the region and **Path-style access**.                                                                                        |
| There is no sync space for this sync key on that storage                                              | Check the address, the bucket and **Folder (optional)**: they must be the same as on the other devices.                                        |
| The master password or the sync key is wrong                                                          | Enter the space's master password (above) and check the sync key.                                                                              |
| The storage must be reached over HTTPS (plain HTTP only to this computer)                             | Use the service's `https://` address.                                                                                                          |
| The sync data of a device is older than before and was refused. The storage may have been rolled back | The storage served an older file of that device. The device writes its file again on its next change; if the message stays, remove the device. |
