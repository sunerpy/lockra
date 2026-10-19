# 路线图

本页介绍 Lockra 目前提供的功能、计划中的功能，以及有意不做的功能。

## 当前版本

Lockra 提供电脑和手机上的验证器所需的功能：支持所有标准账号的验证码，在 Lockra 与 Google 身份验证器、Microsoft Authenticator 之间迁移账号，otpauth 链接与列表，加密备份与自动备份，通过你自己的存储或[你运行的 Lockra 中继](/zh/backup/relay)进行端到端加密的[多设备同步](/zh/backup/sync)，对保险库（电脑支持时可用 Touch ID 或 Windows Hello 验证）、剪贴板和屏幕的保护，以及经过签名校验的应用内更新和各平台的一键安装。自 0.7.0 版起还提供 [Android 应用](/zh/reference/platforms#android)：在手机上使用同一个保险库，用相机或截图扫描二维码，用指纹解锁，并与其他设备同步。欢迎在[问题跟踪](https://github.com/sunerpy/lockra/issues)中提出建议。

### 密码

<StatusTag status="available" /> 自 0.10.0 版起提供。

Lockra 还可以在与验证码相同的保险库中保存密码（[密码](/zh/passwords/)）：

- 登录项：包含用户名和邮箱、网址、备注、自定义字段、标签和分组、关联的验证码、密码历史，以及每个登录项自己的保护级别，电脑和手机上都可以使用；
- 密码生成器：生成随机密码和口令，并遵循已知网站的密码规则；
- 从 Chrome、Edge、Firefox、Safari、Bitwarden、1Password、LastPass 和 KeePass 导入，并导出为 KeePass 数据库或明文 CSV 文件（[导入与导出登录项](/zh/passwords/import-export)）；
- 适用于 Chrome、Edge 和 Firefox 的[浏览器扩展](/zh/passwords/browser)：填入登录项及其验证码，提示保存和更新密码，Lockra 可以在后台运行；
- 由你设定的组合键，可以在任何应用之上打开命令面板。

## 计划中

### iPhone 与 iPad

<StatusTag status="planned" />

与其他平台使用同样的保险库和同步的 iPhone 与 iPad 应用。目前尚无确定的时间。

### 更多密码功能

<StatusTag status="planned" />

密码功能的后续计划，目前尚无确定的时间：

- **更多条目类型**：银行卡、身份信息、安全笔记、API 凭据、Wi-Fi 网络（可显示用于连接的二维码）、软件许可证和证件。
- **安全报告**，在设备上完成：弱密码、重复使用的密码、长期未修改和即将过期的密码，仍在使用 HTTP 的网站，重复的条目，支持两步验证但尚未关联验证码的网站，以及支持通行密钥的网站。通过 Have I Been Pwned 的匿名范围查询检查密码是否出现在已知的泄露中，只在你开启后进行：这将是 Lockra 在更新和同步之外的第一项联网功能。
- **回收站与整理**：删除的登录项保留 30 天、归档、登录项的历史版本、自行排列的列表顺序，以及 Markdown 格式的备注。
- **Android 上的自动填充**：在手机上为其他应用和浏览器提供登录项。
- **其他**：来自 SimpleLogin、addy.io、Firefox Relay、Fastmail 或 DuckDuckGo 的邮箱别名，从 Lockra 打开登录项的网站（并完成填充），可选的网站图标，可打印的应急资料，开机启动，导入加密的 Bitwarden 导出文件，以及无需在桌面环境中设置即可在 Wayland 上使用的全局快捷搜索。

### 更远的计划

<StatusTag status="planned" />

- 通行密钥：在浏览器、Android 和 Windows 11 上使用，并支持在应用之间迁移通行密钥的新标准。
- 用硬件安全密钥解锁。
- 在桌面上把登录项输入到任意应用中。
- Safari，以及以 Snap 或 Flatpak 形式安装的浏览器。
- 附件。
- 与他人共享登录项：通过共享分组，或通过只能使用一次的链接。
- 命令行工具和 SSH 代理。
- 多个保险库，以及旅行模式。

## 有意不做的功能

- **同步账户，或能读取账号的服务器。** 同步无需注册账户；无论同步空间保存在你的存储还是中继上，那里都只有加密的文件。
- **非标准验证码**，例如 Steam 令牌。
- **从 iPhone 上的 Microsoft Authenticator 导入。** 目前没有已知的方法可以读取其中的账号。
