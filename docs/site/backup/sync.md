# Sync between devices

This page explains how to keep the same accounts on several devices, through storage of your own
or a Lockra relay, how a new device joins, and how Lockra keeps that storage from reading your
accounts.

<StatusTag status="available" /> Sync is available from version 0.4.0.

Sync is off until you set it up, and has no account: your devices meet on a Lockra relay, in an
S3-compatible bucket or a WebDAV folder that you choose, or, on a computer, in a folder your cloud
drive keeps in sync, and everything they put there is encrypted before it leaves the device.

## What you need

- **S3-compatible storage**: a bucket at AWS S3, Cloudflare R2, Backblaze B2, Alibaba Cloud OSS,
  MinIO or another S3-compatible service, with an access key that may list, read, write and delete
  in it. Or **WebDAV**: a folder at Nextcloud, Jianguoyun, a Synology NAS or another WebDAV
  service, with a user name and password (an app password where the service offers one).
- An address that starts with `https://`. Lockra refuses plain `http://`, except for an address
  on the same computer.
- Or, on a computer, a folder that a cloud drive's app keeps in sync, with no credentials at all
  (below, [Through a cloud drive folder](#through-a-cloud-drive-folder)).
- Or nothing at all: Lockra's built-in relay, or a relay of your own (below,
  [Through a Lockra relay](#through-a-lockra-relay)).

### By provider

<StatusTag status="available" /> Available from version 0.7.3.

After the storage type, choose the provider: Lockra fills in the address and the addressing, and
asks only for what the table lists besides the credentials. For a service that is not listed,
choose **Other S3-compatible service** or **Other WebDAV service** and fill in every field as
described below.

| Provider                | You enter                                            | Note                                                                                                                             |
| ----------------------- | ---------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| AWS S3 (global regions) | Region, bucket, access key                           | For Beijing or Ningxia choose AWS S3 (China regions): China and global accounts and keys are separate.                           |
| AWS S3 (China regions)  | `cn-north-1` or `cn-northwest-1`, bucket, access key | The address ends in `.amazonaws.com.cn`.                                                                                         |
| Cloudflare R2           | Account ID, bucket, access key                       | The region is `auto`.                                                                                                            |
| Alibaba Cloud OSS       | Region ID (`cn-hangzhou`), bucket, AccessKey         | Accounts opened after 20 March 2025 cannot reach buckets in mainland China regions through the default address; Hong Kong works. |
| Tencent Cloud COS       | Region (`ap-guangzhou`), bucket with its APPID, key  | A bucket is named like `examplebucket-1250000000`.                                                                               |
| Backblaze B2            | Region (`us-west-004`), bucket, application key      | The region is part of the bucket's endpoint.                                                                                     |
| MinIO                   | Address, region, bucket, access key                  | Path-style access is on.                                                                                                         |
| Jianguoyun              | Account, app password                                | Create the app password under Account info › Security › Third-party apps on the Jianguoyun website.                              |
| Nextcloud               | Server, user name, app password                      | Create the app password under Settings › Security.                                                                               |
| Synology NAS            | Server, user name, password                          | Needs the WebDAV Server package (HTTPS on port 5006 by default); start **Folder** with a shared folder, such as `home/lockra`.   |

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
  name for the device and **The sync space's master password**, the master password of any device
  in the space, and choose **Join**. Lockra creates the vault under that master password and the
  accounts arrive.
- **A vault already**: open **Settings › Sync › Join an existing sync** and enter **This device's
  master password**. If the devices in the space use another master password, also enter one of
  theirs in **The sync space's master password (optional)**. The vault's accounts join the space,
  and the vault keeps its own master password.

Without another device, choose **Sync key** instead of **Invitation** and fill in the storage
settings and the sync key.

Every device in the space keeps its own copy of the space's key, encrypted with its own master
password and the sync key. The master password of any device in the space, with the sync key,
adds a device. After you change the master password on a device, the new one works from that
device's next sync, and the old one no longer opens the space unless another device still uses it.

### Easier inviting and joining

<StatusTag status="available" /> Available from version 0.7.3.

- **Show the invitation with Touch ID, Windows Hello or the fingerprint**: on a device that unlocks with one of them, **Invite another device** accepts it instead of the master password.
- **An invitation you can send**: a 10-character code is shown beside the invitation. Send the invitation to the new device by chat or mail, and tell the code another way, in person or by phone; the new device pastes the invitation and enters the code. Scanning the QR code needs no code.
- **One password to join**: a device with a vault enters only its own master password; Lockra asks for another device's only when the space's devices use another one.
- **Save the sync key**: after setting up, you can save the sync key to a file you choose. Until you save it or choose **I have kept it**, **Settings › Sync** reminds you.

## Adding a phone

<StatusTag status="available" /> Available from version 0.7.0.

The Android app syncs like a computer. Show an invitation on a device of the space (**Settings ›
Sync › Invite another device**), then on the phone choose **Settings › Sync › Join an existing
sync › Scan to join**, or **Join from sync** on the welcome screen of a new install, and scan it.
The invitation goes from the camera to Lockra and is not shown on the phone. Pasting the
invitation, or typing the storage settings and the sync key, works as on a computer.

### Adding a computer from a phone

<StatusTag status="building" /> In development, not released yet.

A computer scans nothing, so when the space started on the phone, the phone sends it the invitation
as text. On the phone, open **Settings › Sync › Invite another device** and choose **Copy the
invitation**: the text goes to the clipboard, kept out of clipboard history, and is cleared after
the time set in **Settings › Security › Clear clipboard**. Send it to the computer by chat or mail,
paste it there in **Join an existing sync** (or **Join sync…** on the welcome screen), and enter the
code the phone shows beside it.

The devices of a space are equal: whichever started it, every change syncs both ways. When the
accounts are on the phone and the computer is new, the computer can as well start syncing and show
its QR code; the phone joins with its own vault, and its accounts join the space and reach the
computer.

## Through a cloud drive folder

<StatusTag status="available" /> Available from version 0.7.4.

On a computer, sync needs no storage credentials at all: choose a folder that a cloud drive's app
keeps in sync, such as a folder of OneDrive, iCloud Drive, Dropbox, Jianguoyun, Nextcloud,
Synology Drive or Syncthing. Lockra only writes encrypted files into it, and the drive's app
carries them to your other devices.

1. In the drive's synced folder, make a folder, for example `Lockra`.
2. On the computer, open **Settings › Sync**, choose **Start syncing**, choose **Cloud drive
   folder** as the storage, then **Choose folder…** and pick that folder.
3. Check **This device's name**, enter the master password, and choose **Start syncing**.

The invitation of such a space holds the sync key alone, not the folder on this computer. Once a
new device has pasted or scanned it, Lockra asks how that device reaches the same folder:

- **Another computer**: choose **Cloud drive folder** and pick the same drive's folder on that
  computer.
- **A phone**: choose **WebDAV**, enter the drive's WebDAV address and, in **Folder (optional)**,
  the folder's path in the drive, then scan the invitation again. For example, the folder
  `我的坚果云/Lockra` that Jianguoyun's app syncs is the WebDAV address
  `https://dav.jianguoyun.com/dav/` with the folder `我的坚果云/Lockra`.

OneDrive, iCloud Drive and Dropbox offer no official WebDAV: with them, only computers can join.
For a phone as well, use Jianguoyun, Nextcloud or Synology, or an S3-compatible bucket or WebDAV
instead.

Reading a folder on this computer needs no network: once the drive's app puts another device's
changes into the folder, Lockra syncs about a second later; besides, it looks at the folder every
15 seconds while it is in front, and every minute behind other windows and after a failed run.
When another device's changes reach the folder is up to the drive's app. When the drive's app is not
running, or the folder was moved or deleted, **Settings › Sync** says the sync folder is missing,
and Lockra does not make it again; if the folder moved, choose it again in **Change storage
settings**.

Versions up to 0.7.3 do not know cloud drive folders: opening the same vault file with one of them
keeps the accounts, and shows sync as off.

## Through a Lockra relay

<StatusTag status="available" /> Available from version 0.8.1.

A relay is a server that keeps a sync space's encrypted files, for devices that have no storage of
their own. Lockra runs one, the built-in relay, and you can run your own
([Running your own relay](/backup/relay)). To the devices it is one more kind of storage: they
write the same encrypted files there as anywhere else, and the relay can open none of them.

1. On the first device, open **Settings › Sync** and choose **Start syncing**. **Storage** starts
   on **Lockra relay**, with **Lockra's built-in relay** as the provider and nothing to fill in.
   For a relay of your own, choose **A relay of your own** and enter its **Relay address**, which
   starts with `https://`.
2. Check **This device's name**, enter the master password, and choose **Start syncing**.
3. Keep the sync key Lockra shows, as for any storage.

The invitation of such a space holds the relay's address and the sync key. To add a phone, show the
invitation on the computer (**Invite another device**) and scan its QR code on the phone
(**Settings › Sync › Join an existing sync › Scan to join**, or **Join from sync** on the welcome
screen): the phone needs no storage settings, only a master password. Pasting the
invitation and entering the code shown beside it works too, on a phone or a computer. Without
another device at hand, choose **Sync key**, keep **Lockra relay**, and enter the sync key.

While the vault is unlocked, a device keeps one request waiting at the relay, which answers it when
another device writes: the change arrives about a second later. Besides, a device syncs at the
intervals of any storage ([When devices sync](#when-devices-sync)).

**Change storage settings** also moves a space onto a relay. Unlike other storage, the relay need
not hold the space yet: this device writes it there on its next sync. Then change the storage
settings on the other devices the same way.

What the relay can see:

- **What any storage sees.** Encrypted files, their sizes in steps of 4 KiB, how many devices a
  space has and when they write, and the network addresses the devices connect from. Never an
  account, a secret, a device name, the sync key or a master password
  ([What the storage can see](#what-the-storage-can-see)).
- **Only the space's devices change it.** The devices identify themselves with a value made from
  the sync key, from which the sync key cannot be recovered; the relay keeps only a fingerprint of
  that value and refuses every request without it. Someone who learns where a space lives on the
  relay can neither read nor change it.
- **The relay can be unavailable.** It can also delete a space, like any storage: the devices keep
  every account, and write the space again on their next sync. The built-in relay is a single
  server; for a space that must not depend on it, run a relay of your own or use storage of your
  own.

The built-in relay keeps a space for 400 days after a device last reached it, holds at most 64
devices and 32 MiB per space, and limits the requests each network address makes. When it is full,
a run fails with "The storage answered with an error"; when it asks a device to slow down, with
"The storage could not be reached"; the next run tries again. What the built-in relay keeps is on
the [Privacy](/privacy#the-built-in-relay) page.

## When devices sync

While the vault is unlocked, a device syncs when you unlock it, three seconds after a change, at
intervals (below), and when you choose **Sync now**. While the vault is locked, nothing goes out.
The status line shows **Synced** and when, or why the last run failed; the next run tries again.

### Syncing sooner

<StatusTag status="available" /> Available from version 0.7.3.

- **More often in front**: while Lockra is in front, a device syncs every minute; on a computer,
  behind other windows, every five minutes. Back in Lockra, a computer syncs once the last run is
  30 seconds old. After a failed run, the next one comes five minutes later, or choose **Sync
  now**.
- **What a run brought**: when a run adds, changes or deletes accounts on this device, Lockra
  says so and names the devices the changes came from, for example "Synced from Pixel 8: 2 added,
  1 changed".

Accounts added on several devices at the same time are all kept, on every device. When the same
account was changed on two devices, the later change wins on every device. A deletion removes the
account on every device, unless the account was changed after the deletion. The same account added
apart on two devices (its QR code scanned on both) shows twice: delete one, and the deletion syncs.
The order of recently used accounts stays on each device.

## Devices

**Devices** lists every device of the space and when it last wrote. To remove a lost or retired
device, choose its remove button: its data, with its copy of the space's key, is deleted from the
storage. A device that is still in use appears again on its next sync. **Rename** changes this device's name for the others.

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
- **Two secrets open the space.** On each device, the key that encrypts the files is itself
  encrypted with both that device's master password (through Argon2id, 64 MiB of memory and three
  passes) and the sync key. Someone with the storage's contents opens nothing without the sync key,
  however good their guess of a master password; with the sync key, every guess still costs a full
  Argon2id run. Give every device a strong master password.
- **Devices never overwrite each other.** Each device writes only its own file, so devices that
  sync at the same moment keep each other's changes, on S3 and WebDAV alike.
- **Changes are detected.** A file that was altered, moved from another device or space, or put
  back to an older version is refused, and **Settings › Sync** names the device. Your accounts stay
  as they are.
- **Deleting is not prevented.** Whoever can write to the storage can delete the space. That stops
  sync, not your vaults: every device keeps its accounts.
- **Removing a device does not revoke it.** A removed device still has the space's key. To shut
  out a lost device, or someone who has the sync key and an old master password, turn off sync on
  every device, start a new sync space and add the devices to it.
- **Backups leave sync out.** The storage settings, their credentials and the sync key are kept in
  the vault file only. A backup does not contain them; a device restored from a backup joins the
  space again.

## When something goes wrong

| Message                                                                                                  | What to do                                                                                                                                     |
| -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| The storage refused access; check the access key or password                                             | Check the access key or the password, and that it may list, read, write and delete in the bucket or folder.                                    |
| The storage could not be reached                                                                         | Check the network and the address. A proxy configured in the system is used.                                                                   |
| The storage answered with an error; check that the bucket or folder exists                               | Check the bucket, the region and **Path-style access**.                                                                                        |
| There is no sync space for this sync key on that storage                                                 | Check the address, the bucket and **Folder (optional)**: they must be the same as on the other devices.                                        |
| The master password or the sync key is wrong                                                             | Enter the master password of a device in the space (above) and check the sync key.                                                             |
| The storage must be reached over HTTPS (plain HTTP only to this computer)                                | Use the service's `https://` address.                                                                                                          |
| The sync data of a device is older than before and was refused. The storage may have been rolled back    | The storage served an older file of that device. The device writes its file again on its next change; if the message stays, remove the device. |
| The sync folder is missing: the cloud drive's app may not be running, or the folder was moved or deleted | Start the drive's app and check the folder is where it was; if it moved, choose it again in **Change storage settings**.                       |
