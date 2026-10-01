# 更新、卸载与数据

本页介绍如何升级到新版本、Lockra 把文件保存在哪里，以及如何彻底卸载。

## 更新

从 0.2.0 版本起，Lockra 可以自行更新。打开「设置 › 关于」，选择「检查更新」。有新版本时，这里会显示更新内容；「下载并安装」会下载新版本、校验签名并安装，随后 Lockra 自动重启。保险库和设置会保留。更新前，请确认有一份较新的备份：「备份 › 保存备份…」。

更新的安装方式取决于 Lockra 的安装方式：

| 安装方式         | 更新过程                                 |
| ---------------- | ---------------------------------------- |
| `.deb` 或 `.rpm` | 安装新的安装包；系统会要求输入管理员密码 |
| AppImage         | 替换 AppImage 文件，然后 Lockra 重启     |
| Windows 安装程序 | 安装程序关闭 Lockra，安装完成后重新打开  |
| macOS 应用       | 替换应用，然后 Lockra 重启               |

只有带有 Lockra 签名、且签名对应发布所声明版本的安装包才会被安装，被改动过的或较旧的安装包都会被拒绝。Lockra 0.1.x 没有应用内更新：请用[安装脚本](/zh/guide/install#一键安装)或安装包把 0.2.0 或更高版本覆盖安装一次。不是通过安装包安装的副本（例如从源码构建的副本）无法自行更新，「设置 › 关于」中会说明这一点。

也可以随时使用安装脚本更新，或直接安装新的安装包覆盖旧版本。

### 自动检查

「设置 › 关于 › 自动检查更新」默认关闭。开启后，Lockra 在启动 10 秒后检查一次，之后每天检查一次，发现新版本时会提示。Lockra 不会自行下载或安装新版本。检查失败时（例如没有网络），下一次检查会在一小时内进行。

### 检查时发送的内容

检查更新时，Lockra 向发布 Lockra 的 GitHub 请求一个标明最新版本的小文件；安装时，从同一个发布下载安装包。这两个请求都不包含账号、保险库或设置中的任何内容。与任何下载一样，GitHub 能看到请求及其来源地址。不检查更新时，Lockra 不建立任何网络连接。

## 文件位置

|        | Windows                                      | macOS                                                            | Linux                                            |
| ------ | -------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------ |
| 保险库 | `%APPDATA%\dev.lockra.desktop\vault.lockra`  | `~/Library/Application Support/dev.lockra.desktop/vault.lockra`  | `~/.local/share/dev.lockra.desktop/vault.lockra` |
| 设置   | `%APPDATA%\dev.lockra.desktop\settings.json` | `~/Library/Application Support/dev.lockra.desktop/settings.json` | `~/.config/dev.lockra.desktop/settings.json`     |

「设置 › 关于」显示当前电脑上的数据目录。Lockra 在保险库旁边保留上一个版本 `vault.lockra.prev`，以及替换式恢复之前保存的副本（`pre-restore-<日期>.lockrabackup`）。设置文件不包含任何密钥。

## 迁移到另一台电脑

保存一份备份，把文件复制到另一台电脑，在那里安装 Lockra，并在欢迎页选择「从备份恢复」。账号随之迁入，备份的密码成为主密码。

## 卸载

按照系统的常规方式卸载 Lockra：

| 安装方式   | 卸载方法                                                                            |
| ---------- | ----------------------------------------------------------------------------------- |
| `.deb`     | `sudo apt remove lockra`                                                            |
| `.rpm`     | `sudo dnf remove lockra`（或 `sudo zypper remove lockra`）                          |
| AppImage   | 删除 `~/.local/bin/Lockra.AppImage` 和 `~/.local/share/applications/lockra.desktop` |
| macOS 应用 | 把 Lockra 从“应用程序”文件夹移到废纸篓                                              |
| Windows    | 「设置 › 应用 › 已安装的应用 › Lockra › 卸载」                                      |

保险库和设置仍保留在上述文件夹中，以后重新安装时会再次读取；删除这些文件夹即可彻底清除。如果使用过「在本机记住」，请在卸载前关闭它，以便同时删除系统钥匙串中的密钥。
