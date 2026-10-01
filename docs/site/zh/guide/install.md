# 安装

本页介绍如何在 Windows、macOS 和 Linux 上下载并安装 Lockra，以及如何确认下载的文件未被篡改。

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
gh attestation verify Lockra_0.1.0_amd64.deb --repo sunerpy/lockra
```

## Windows

运行安装程序。它只为当前用户安装 Lockra，无需管理员权限。Lockra 使用 Microsoft Edge WebView2，Windows 10 和 11 已自带；如果电脑上缺少，安装程序会自动下载。

安装程序目前尚未签名，因此 Microsoft Defender SmartScreen 可能显示「Windows 已保护你的电脑」。选择「更多信息」，确认发布者和文件名与你下载的一致，然后选择「仍要运行」。

## macOS

打开 dmg，把 Lockra 拖到「应用程序」文件夹。应用目前尚未使用 Apple 开发者 ID 签名，因此首次打开时 macOS 会拒绝。打开「系统设置 › 隐私与安全性」，找到有关 Lockra 的提示并选择「仍要打开」。此后可以正常打开。

## Linux

安装适合你所用发行版的安装包，例如：

```bash
sudo apt install ./Lockra_0.1.0_amd64.deb      # Debian、Ubuntu
sudo dnf install ./Lockra-0.1.0-1.x86_64.rpm    # Fedora
```

AppImage 无需安装：添加可执行权限后直接运行。Lockra 需要 WebKitGTK 4.1，deb 和 rpm 安装包会自动安装它。要使用「在本机记住」，桌面环境需要提供 Secret Service 钥匙串，例如 GNOME 钥匙圈或 KWallet。

## 后续步骤

[快速开始](/zh/guide/quick-start)介绍如何创建保险库并导入第一批账号。
