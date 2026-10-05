# Sync between devices

This page explains how to keep the same accounts on several devices through storage of your own,
how a new device joins, and how Lockra keeps that storage from reading your accounts.

<StatusTag status="available" /> Sync is available from version 0.4.0.

Sync is off until you set it up. Lockra runs no server and has no account: your devices meet in an
S3-compatible bucket or a WebDAV folder that you choose, and everything they put there is encrypted
before it leaves the device.

## What you need

- **S3-compatible storage**: a bucket at AWS S3, Cloudflare R2, Backblaze B2, Alibaba Cloud OSS,
  MinIO or another S3-compatible service, with an access key that may list, read, write and delete
  in it. Or **WebDAV**: a folder at Nextcloud, Jianguoyun, a Synology NAS or another WebDAV
  service, with a user name and password (an app password where the service offers one).
- An address that starts with `https://`. Lockra refuses plain `http://`, except for an address
  on the same computer.

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

## Over the local network

<StatusTag status="building" /> In development, not released yet.

On the same network, devices can sync directly through one computer, with no storage. That computer is the hub: the devices paired with it exchange encrypted data through it, and a change shows on the others within seconds. With a storage as well, both are used: away from the network, the devices sync through the storage as before.

### Turning it on

On the computer that is to be the hub, open **Settings › Sync**:

- **Not syncing yet**: under **Sync over the local network**, choose **Turn on local network sync**, check the device name and enter the master password. Lockra starts a sync space over the local network only, and reminds you to keep its sync key.
- **Already syncing through a storage**: under **Local network**, choose **Turn on for this computer**, then verify or enter the master password.

The first time, Windows asks whether Lockra may use the network: allow private networks.

### Pairing a device

1. On the hub, choose **Pair a device…** and verify or enter the master password. Lockra shows a QR code and a pairing code, which work once and for 2 minutes.
2. On the device to pair:
   - **A phone**: on a new install, choose **Sync from a computer** on the welcome screen; with a vault, open **Settings › Sync** and choose **Sync from a computer** or **Connect to a computer**. Enter this phone's master password (on a new install, choose a new one and type it twice), then scan the QR code on the computer. The camera hands the pairing code to Lockra without showing it on the phone; **Paste the code** works too.
   - **Another computer**: open **Settings › Sync**. Not syncing yet, paste the pairing code under **Join an existing sync**; already syncing, choose **Connect to another computer…** under **Local network** and paste it there. Then enter this device's master password; on a computer with no vault yet, choose a new master password.
3. Both screens show the same 6-digit code. Once it matches on the hub, choose **Allow**; if the codes differ, choose **Refuse**.

Until you allow it, the device gets nothing of the sync space. A paired device does not need the hub's master password: it keeps the sync space's key encrypted under its own.

### When the hub is locked or away

Lockra has to keep running on the hub. On Windows and macOS, once local network sync is on, closing the window leaves Lockra in the system tray (**Keep running after the window closes**, which **Local network** turns off); to quit, choose **Quit Lockra** in the tray icon's menu. On Linux, closing the window quits. While locked, the hub still takes in the paired devices' changes and merges them at the unlock. When the hub is off or on another network, the paired devices show their local network sync as offline and try again later; those with a storage go on syncing through it.

### Managing the pairing

- **Paired devices** lists the devices paired with the hub. **Unpair** stops one from reaching the hub and deletes its sync data there; the accounts it already has are not taken back.
- A sync space over the local network only can take a storage under **Add a storage**: its settings, or another device's invitation of the same space. With both, **Remove storage** on the storage's row keeps the local network only.
- **Stop syncing over the local network** ends this computer's part as the hub, and the paired devices have to pair again. In a space without a storage, this device's sync is turned off with it, and its accounts stay.

The hub accepts connections from this computer's and the local network's addresses only, and every paired device connects with a key of its own over an encrypted channel. On public Wi-Fi, stop syncing over the local network.

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

When the same account was changed on two devices, the later change wins on every device. A
deletion removes the account on every device, unless the account was changed after the deletion.
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

| Message                                                                                               | What to do                                                                                                                                     |
| ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| The storage refused access; check the access key or password                                          | Check the access key or the password, and that it may list, read, write and delete in the bucket or folder.                                    |
| The storage could not be reached                                                                      | Check the network and the address. A proxy configured in the system is used.                                                                   |
| The storage answered with an error; check that the bucket or folder exists                            | Check the bucket, the region and **Path-style access**.                                                                                        |
| There is no sync space for this sync key on that storage                                              | Check the address, the bucket and **Folder (optional)**: they must be the same as on the other devices.                                        |
| The master password or the sync key is wrong                                                          | Enter the master password of a device in the space (above) and check the sync key.                                                             |
| The storage must be reached over HTTPS (plain HTTP only to this computer)                             | Use the service's `https://` address.                                                                                                          |
| The sync data of a device is older than before and was refused. The storage may have been rolled back | The storage served an older file of that device. The device writes its file again on its next change; if the message stays, remove the device. |
