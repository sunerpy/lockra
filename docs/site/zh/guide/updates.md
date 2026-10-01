# 更新、卸载与数据

本页介绍如何升级到新版本、Lockra 把文件保存在哪里，以及如何彻底卸载。

## 更新

Lockra 本身不检查更新，因为它从不联网。请关注[发布页](https://github.com/sunerpy/lockra/releases)（在 GitHub 上选择 Watch › Custom › Releases），然后直接安装新的安装包覆盖旧版本。保险库和设置会保留。

更新前，请确认有一份较新的备份：「备份 › 保存备份…」。

## 文件位置

|        | Windows                                      | macOS                                                            | Linux                                            |
| ------ | -------------------------------------------- | ---------------------------------------------------------------- | ------------------------------------------------ |
| 保险库 | `%APPDATA%\dev.lockra.desktop\vault.lockra`  | `~/Library/Application Support/dev.lockra.desktop/vault.lockra`  | `~/.local/share/dev.lockra.desktop/vault.lockra` |
| 设置   | `%APPDATA%\dev.lockra.desktop\settings.json` | `~/Library/Application Support/dev.lockra.desktop/settings.json` | `~/.config/dev.lockra.desktop/settings.json`     |

「设置 › 关于」显示当前电脑上的数据目录。Lockra 在保险库旁边保留上一个版本 `vault.lockra.prev`，以及替换式恢复之前保存的副本（`pre-restore-<日期>.lockrabackup`）。设置文件不包含任何密钥。

## 迁移到另一台电脑

保存一份备份，把文件复制到另一台电脑，在那里安装 Lockra，并在欢迎页选择「从备份恢复」。账号随之迁入，备份的密码成为主密码。

## 卸载

按照系统的常规方式卸载 Lockra。保险库和设置仍保留在上述文件夹中，以后重新安装时会再次读取；删除这些文件夹即可彻底清除。如果使用过「在本机记住」，请在卸载前关闭它，以便同时删除系统钥匙串中的密钥。
