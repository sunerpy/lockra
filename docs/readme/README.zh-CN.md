<div align="center">

<img src="../../apps/desktop/src-tauri/icons/128x128.png" alt="Lockra" width="96" />

# Lockra

### 两步验证码保存在你的电脑和手机上，加密，多端同步。

[![CI](https://github.com/sunerpy/lockra/actions/workflows/ci.yml/badge.svg)](https://github.com/sunerpy/lockra/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/sunerpy/lockra)](https://github.com/sunerpy/lockra/releases)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue)](../../LICENSE)

[亮点](#亮点) · [特性](#特性) · [截图](#截图) · [安装](#安装) · [快速开始](#快速开始) · [文档](https://firlab.app/lockra/zh/) · [开发](#开发)

[English](../../README.md) · [**简体中文**](./README.zh-CN.md)

</div>

---

Lockra 是一个 TOTP/HOTP 验证器，支持 Windows、macOS、Linux 和 Android。账号保存在每台设备的加密文件中，
Lockra 只为自身的更新以及你开启的设备间端到端加密同步联网。它可以从 Google 身份验证器和 Microsoft Authenticator
导入账号，也可以导出给它们，并把加密备份写入你选择的文件夹。

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../site/public/screens/devices-zh-dark.webp" />
  <img src="../site/public/screens/devices-zh-light.webp" alt="电脑和手机上的 Lockra，显示相同的账号和相同的验证码" />
</picture>

## 亮点

- **多端支持**：同一个应用覆盖 Windows、macOS、Linux（x64 与 ARM64）和 Android 8 及以上，界面设计一致，
  提供八套主题和中英文界面。
- **多端同步**：经内置的 Lockra 中继（无需任何设置）、自建中继、S3 兼容存储桶、WebDAV 文件夹，或在电脑之间
  经网盘同步文件夹，端到端加密地同步，每台设备上的账号保持一致。内容在离开设备之前就已加密，中继或存储里只有密文。
- **扫码配对**：手机扫描电脑上的二维码即可加入，也可以粘贴配对链接或邀请码；所有设备都丢失时，可以用恢复密钥
  找回同步空间。
- **无需账号**：无需注册，没有遥测，也没有能读取验证码的服务器。账号可以从 Google 身份验证器或 Microsoft
  Authenticator 迁入，也可以迁回去。

## 特性

- **验证码**：TOTP（SHA1、SHA256、SHA512，6–8 位，任意周期）和 HOTP。点击一行即复制；30 秒后，如果剪贴板中
  仍是这个验证码就会清空；验证码在最后 5 秒同时显示下一个。支持搜索、分组、收藏和隐藏验证码。
- **导入**：Google 身份验证器「转移账号」生成的二维码（照片或截图，可一次选多张）、已 root 的 Android 手机上
  Microsoft Authenticator 的数据库、`otpauth://` 链接与列表，以及 Lockra 备份。保存之前先给出预览，逐条标明
  新增、已存在、同名不同密钥或不支持，并说明原因。
- **导出**：给 Google 身份验证器的迁移二维码、给 Microsoft Authenticator 的每个账号一张标准二维码，或者明文列表。
  每种方式都要再次输入主密码，二维码旁边显示当前验证码，方便与手机核对。
- **备份**：随时手动备份为加密的 `.lockrabackup` 文件；也可以在每次修改几秒后自动备份到指定文件夹（OneDrive、
  Google Drive 或 iCloud 的同步文件夹均可），只保留最近几份。恢复时可以合并，也可以整体替换。
- **同步**：在电脑和 Android 应用之间端到端加密同步，可以通过 S3 兼容存储桶、WebDAV 文件夹、网盘客户端同步的
  文件夹，或从 0.8.1 起通过 Lockra 中继（[见下文](#同步中继)）。手机扫描电脑上的二维码即可加入。保存同步空间
  的一方只能看到加密的文件（[同步指南](https://firlab.app/lockra/zh/backup/sync)）。
- **安全**：Argon2id 与 XChaCha20-Poly1305，可选用系统钥匙串解锁，自动锁定，显示密钥时阻止截屏（Windows 和
  macOS）。
- **更新**：有新版本时，标题栏会提示，并可打开更新对话框（更新内容、带速度的下载进度、重启）；开启自动更新后，
  Lockra 在启动时下载新版本，重启时完成安装。只安装带有 Lockra 签名的安装包，并通过 Lockra 原来的安装方式
  （deb、rpm、AppImage、Windows 安装程序或 macOS 应用）完成安装。
- **界面**：八套主题（默认「澄空」，跟随系统的浅色与深色）、八种强调色，列表或卡片视图，中英文界面，
  命令面板（`Ctrl K`），全程可用键盘操作。

## 截图

Android 应用：验证码页、添加账号和同步。

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="../site/public/screens/phones-zh-dark.webp" />
  <img src="../site/public/screens/phones-zh-light.webp" alt="Android 应用的三个页面：验证码页、添加账号和同步" />
</picture>

电脑和手机上的更多页面，见[文档](https://firlab.app/lockra/zh/)。

## 安装

1. **一键安装**：脚本会为当前电脑选择对应的安装包，用该版本的 `SHA256SUMS` 校验后安装
   （[安装指南](https://firlab.app/lockra/zh/guide/install#一键安装)）。

   ```bash
   # Linux 和 macOS
   curl -fsSL https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.sh | sh
   ```

   ```powershell
   # Windows（PowerShell）
   irm https://raw.githubusercontent.com/sunerpy/lockra/main/scripts/install.ps1 | iex
   ```

2. **下载安装包**：在[发布页](https://github.com/sunerpy/lockra/releases)选择 Windows 安装程序（x64 与 ARM64）、
   适用于 Apple 芯片或 Intel Mac 的 dmg、适用于 Linux（x64 与 ARM64）的 deb、rpm、AppImage，或适用于 Android
   手机的 APK（ARM64，Android 8 及以上；[Android](https://firlab.app/lockra/zh/guide/install#android)）。每个版本都附带
   `SHA256SUMS` 和构建证明：

   ```bash
   sha256sum -c SHA256SUMS --ignore-missing
   gh attestation verify Lockra_*_amd64.deb --repo sunerpy/lockra
   ```

   安装包目前尚未签名，首次启动时 Windows SmartScreen 和 macOS Gatekeeper 会提示确认
   （[平台说明](https://firlab.app/lockra/zh/reference/platforms)）。

3. **从源码构建**（见[开发](#开发)）。

从 0.2.0 起，Lockra 可以自行更新：「设置 › 通用 › 检查更新」，或标题栏中的提示（[更新说明](https://firlab.app/lockra/zh/guide/updates)）。

## 同步中继

从 0.8.1 起，没有自己存储的设备可以通过 Lockra 中继同步。内置中继无需任何设置。自建中继时，每个版本都提供
Linux 版 `lockra-relay`（x64 与 ARM64）及其 systemd 服务；`deploy/relay/` 中还有一个 Docker Compose 文件，
在中继前面运行 Caddy 并自动配置 HTTPS：

```bash
RELAY_HOST=relay.example.com docker compose -f deploy/relay/compose.yaml up -d
```

中继只保存设备加密后的快照：加密快照的密钥和同步密钥都不会到达中继；设备向中继出示由同步密钥派生的访问令牌，
中继只保存它的哈希。中继能看到设备何时、从哪些地址同步（[自建指南](https://firlab.app/lockra/zh/backup/relay)、
[docs/relay.md](../relay.md)（英文）、[安全模型](../security.md#the-relay)（英文））。

## 快速开始

1. 启动 Lockra，设置主密码。主密码无法找回，请妥善保管。
2. 在「导入」页导入账号：Google 身份验证器导出二维码的照片、粘贴或从剪贴板读取的链接，或者一个备份。
3. 点击账号即可复制验证码。在「备份」页开启自动备份。

[文档](https://firlab.app/lockra/zh/)详细介绍了每一步、在手机与电脑之间迁移账号、备份、安全和常见问题。

## 开发

需要 Rust 1.98（见 `rust-toolchain.toml`）、Node ≥ 20.19 和 pnpm 9；Linux 上还需要 webkit2gtk 4.1。
`make check` 另外需要 Python ≥ 3.9、cargo-llvm-cov、cargo-deny、actionlint 和 shellcheck，缺少其中
任何一个时，会在开始检查前列出。

```bash
pnpm install --frozen-lockfile
pnpm --filter @lockra/desktop tauri dev   # 带热重载的桌面应用
pnpm dev                                  # 只运行界面，后端是内存中的模拟核心
make check                                # 全部检查
make help                                 # 其余命令
```

开发流程见 [CONTRIBUTING.md](../../CONTRIBUTING.md)，代码约定见 [AGENTS.md](../../AGENTS.md)，代码结构见
[docs/architecture.md](../architecture.md)，文件格式见 [docs/formats.md](../formats.md)，安全模型见
[docs/security.md](../security.md)，发布流程见 [docs/release.md](../release.md)。

## 许可证

[Apache-2.0](../../LICENSE)。界面设计源自 [Voltip](https://github.com/sunerpy/voltip)；随附字体采用 SIL Open Font
License 1.1（[NOTICE](../../NOTICE)）。
