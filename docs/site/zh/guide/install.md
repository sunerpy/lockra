# 安装

本页介绍如何在 Windows、macOS 和 Linux 上下载并安装 Lockra，以及如何确认下载的文件未被篡改。

## 一键安装

安装脚本会为当前电脑选择对应的安装包，用该版本的 `SHA256SUMS` 校验，校验不一致时不安装任何内容。再次运行同一条命令即可安装更新的版本。

在 Linux 和 macOS 的终端中运行：

```bash
curl -fsSL https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.sh | sh
```

在 Windows 的 PowerShell 中运行：

```powershell
irm https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.ps1 | iex
```

| 系统                                             | 脚本安装的内容                                            |
| ------------------------------------------------ | --------------------------------------------------------- |
| 使用 apt 的 Linux（Debian、Ubuntu）              | `.deb`，通过 apt 安装                                     |
| 使用 dnf、zypper 或 yum 的 Linux（Fedora、SUSE） | `.rpm`                                                    |
| 其他 Linux                                       | AppImage，放在 `~/.local/bin`，并添加到应用程序菜单       |
| macOS                                            | dmg 中的应用，放在 `/Applications`（或 `~/Applications`） |
| Windows                                          | 安装程序，仅为当前用户安装，无需管理员权限                |

在 Linux 上安装 `.deb` 或 `.rpm` 时，系统会要求输入密码。在 macOS 上，应用打开时不会出现下文所述的“来自身份不明的开发者”提示：脚本会清除浏览器下载时附带的标记。脚本读取三个设置：`LOCKRA_VERSION=0.2.0` 安装指定版本而不是最新版本，`LOCKRA_PACKAGE=appimage`（或 `deb`、`rpm`）选择其他 Linux 安装包，`LOCKRA_INSTALL_DIR` 指定 AppImage 或应用的其他目录。在 PowerShell 中，请在运行命令前设置 `$env:LOCKRA_VERSION = "0.2.0"`。

## 下载

每个版本都发布在[发布页](https://github.com/sunerpy/lockra/releases)，包含各平台的安装包：

| 平台                        | 安装包                                                                                       |
| --------------------------- | -------------------------------------------------------------------------------------------- |
| x64 上的 Windows 10 和 11   | `Lockra_<版本>_x64-setup.exe`（安装程序）或 `Lockra_<版本>_x64_en-US.msi`                    |
| ARM 上的 Windows 11         | `Lockra_<版本>_arm64-setup.exe`                                                              |
| macOS 11 及以上，Apple 芯片 | `Lockra_<版本>_aarch64.dmg`                                                                  |
| macOS 11 及以上，Intel      | `Lockra_<版本>_x64.dmg`                                                                      |
| x64 上的 Linux              | `Lockra_<版本>_amd64.deb`、`Lockra-<版本>-1.x86_64.rpm` 或 `Lockra_<版本>_amd64.AppImage`    |
| ARM64 上的 Linux            | `Lockra_<版本>_arm64.deb`、`Lockra-<版本>-1.aarch64.rpm` 或 `Lockra_<版本>_aarch64.AppImage` |

## 校验下载的文件

每个版本都在 `SHA256SUMS` 中列出每个文件的 SHA-256 校验和，每个文件还附有构建证明，用于证明它是由该版本的源码经发布流程构建的。

```bash
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify Lockra_*_amd64.deb --repo sunerpy/lockra
```

## Windows

运行安装程序。它只为当前用户安装 Lockra，无需管理员权限。Lockra 使用 Microsoft Edge WebView2，Windows 10 和 11 已自带；如果电脑上缺少，安装程序会自动下载。

安装程序目前尚未签名，因此 Microsoft Defender SmartScreen 可能显示「Windows 已保护你的电脑」。选择「更多信息」，确认发布者和文件名与你下载的一致，然后选择「仍要运行」。

## macOS

打开 dmg，把 Lockra 拖到「应用程序」文件夹。应用目前尚未使用 Apple 开发者 ID 签名，因此首次打开时 macOS 会拒绝。打开「系统设置 › 隐私与安全性」，找到有关 Lockra 的提示并选择「仍要打开」。此后可以正常打开。

## Linux

安装适合你所用发行版的安装包，例如：

```bash
sudo apt install ./Lockra_*_amd64.deb      # Debian、Ubuntu
sudo dnf install ./Lockra-*-1.x86_64.rpm    # Fedora
```

AppImage 无需安装：添加可执行权限后直接运行。Lockra 需要 WebKitGTK 4.1，deb 和 rpm 安装包会自动安装它。要使用「在本机记住」，桌面环境需要提供 Secret Service 钥匙串，例如 GNOME 钥匙圈或 KWallet。

## Android

<StatusTag status="available" /> 自 0.7.0 版起提供。

自 0.7.0 版起，每个版本还会附带 `Lockra_<version>_android_arm64.apk`，适用于 Android 8 及以上的 ARM64 手机。在手机上从[发布页](https://github.com/sunerpy/lockra/releases)下载并打开它；首次安装时，Android 会询问是否允许打开它的浏览器或文件管理器安装应用。发布页上更新版本的 APK 可以直接覆盖安装，保险库会保留。

与其他下载一样，可以用 `SHA256SUMS` 和 `gh attestation verify Lockra_*_android_arm64.apk --repo sunerpy/lockra` 校验 APK。APK 使用 Lockra 的 Android 密钥签名：使用 Android SDK 的构建工具运行 `apksigner verify --print-certs Lockra_*_android_arm64.apk`，可以看到证书的 SHA-256：`5ac2ccffe00d12e80d13dbfc23425cd3b89eec77cd5d21adda4fdac1cf4dcc28`。Android 不会用其他密钥签名的 APK 更新已安装的应用。

## 后续步骤

[快速开始](/zh/guide/quick-start)介绍如何创建保险库并导入第一批账号。
