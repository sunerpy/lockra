# 路线图

本页介绍 Lockra 目前提供的功能、正在开发的功能，以及有意不做的功能。

## 当前版本

Lockra 提供电脑上的验证器所需的功能：支持所有标准账号的验证码，在 Lockra 与 Google 身份验证器、Microsoft Authenticator 之间迁移账号，otpauth 链接与列表，加密备份与自动备份，通过你自己的存储或 Lockra 中继（内置中继，或[你自建的中继](/zh/backup/relay)）进行端到端加密的[多设备同步](/zh/backup/sync)，对保险库（电脑支持时可用 Touch ID 或 Windows Hello 验证）、剪贴板和屏幕的保护，以及经过签名校验的应用内更新和各平台的一键安装。自 0.7.0 版起还提供 [Android 应用](/zh/reference/platforms#android)：在手机上使用同一个保险库，用相机或截图扫描二维码，用指纹解锁，并与其他设备同步。欢迎在[问题跟踪](https://github.com/sunerpy/lockra/issues)中提出建议。

## 有意不做的功能

- **同步账户，或能读取账号的服务器。** 同步无需注册账户；无论同步空间保存在你的存储还是中继上，那里都只有加密的文件。
- **浏览器扩展或自动填写验证码。** 请复制验证码后粘贴。
- **网站图标。** 账号显示首字母；获取图标需要联网。
- **非标准验证码**，例如 Steam 令牌。
- **从 iPhone 上的 Microsoft Authenticator 导入。** 目前没有已知的方法可以读取其中的账号。
