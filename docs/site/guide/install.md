# Install

This page explains how to download and install Lockra on Windows, macOS and Linux, and how to
check that a download is genuine.

## Download

Every release is on the [releases page](https://github.com/sunerpy/lockra/releases), with the
packages for each platform:

| Platform                         | Package                                                                                               |
| -------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Windows 10 and 11 on x64         | `Lockra_<version>_x64-setup.exe` (installer) or `Lockra_<version>_x64_en-US.msi`                      |
| Windows 11 on ARM                | `Lockra_<version>_arm64-setup.exe`                                                                    |
| macOS 11 or later, Apple silicon | `Lockra_<version>_aarch64.dmg`                                                                        |
| macOS 11 or later, Intel         | `Lockra_<version>_x64.dmg`                                                                            |
| Linux on x64                     | `Lockra_<version>_amd64.deb`, `Lockra-<version>-1.x86_64.rpm` or `Lockra_<version>_amd64.AppImage`    |
| Linux on ARM64                   | `Lockra_<version>_arm64.deb`, `Lockra-<version>-1.aarch64.rpm` or `Lockra_<version>_aarch64.AppImage` |

## Check the download

Each release lists the SHA-256 checksum of every file in `SHA256SUMS`, and each file carries a
build attestation that proves it was built from the release's source by its release workflow.

```bash
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify Lockra_*_amd64.deb --repo sunerpy/lockra
```

## Windows

Run the installer. It installs Lockra for your user account only, without administrator rights.
Lockra uses Microsoft Edge WebView2, which Windows 10 and 11 include; on a computer where it is
missing, the installer downloads it.

The installer is not code-signed yet, so Microsoft Defender SmartScreen may show "Windows protected
your PC". Choose **More info**, check that the publisher and the file name are the ones you
downloaded, then **Run anyway**.

## macOS

Open the dmg and drag Lockra to Applications. The app is not signed with an Apple Developer ID yet,
so the first time macOS refuses to open it. Open **System Settings › Privacy & Security**, find the
message about Lockra and choose **Open Anyway**. Later starts open normally.

## Linux

Install the package for your distribution, for example:

```bash
sudo apt install ./Lockra_*_amd64.deb      # Debian, Ubuntu
sudo dnf install ./Lockra-*-1.x86_64.rpm    # Fedora
```

The AppImage runs without installing: make it executable and start it. Lockra needs WebKitGTK 4.1,
which the deb and rpm packages pull in. To use **Remember on this device**, the desktop needs a
Secret Service keychain such as GNOME Keyring or KWallet.

## Next steps

[Quick start](/guide/quick-start) creates the vault and brings in your first accounts.
