# Microsoft Authenticator

本页介绍哪些账号可以从 Microsoft Authenticator 迁入 Lockra，以及如何把 Lockra 的账号添加到 Microsoft Authenticator。

## 迁入 Lockra

Microsoft Authenticator 没有导出功能：它的云备份只能恢复到 Microsoft Authenticator 中。只能从应用的数据库中读取账号，而且只能在已获得 root 权限的 Android 手机上进行。

1. 在已 root 的手机上，从 `/data/data/com.azure.authenticator/databases/` 复制两个文件：`PhoneFactor` 和 `PhoneFactor-wal`。第二个文件保存最近的更改；缺少它时，最近添加的账号会丢失。
2. 把这两个文件传到电脑上。
3. 打开「导入」，在「Microsoft Authenticator」下选择「选择 PhoneFactor 文件…」，并同时选中两个文件。
4. 核对预览，然后选择「导入」。

Lockra 读取的是文件的副本，原文件不会被改动。

### 可以迁移的账号

| 账号                                                  | 结果                                                                                 |
| ----------------------------------------------------- | ------------------------------------------------------------------------------------ |
| 通过二维码添加的其他服务（GitHub、Google、Amazon 等） | 导入：6 位，30 秒                                                                    |
| 个人 Microsoft 账户                                   | 导入：8 位，30 秒                                                                    |
| 工作或学校账户                                        | 不导入：Microsoft 不在手机上保存其他应用可用的密钥；请在新的验证器中重新设置这个账户 |

较新版本的 Microsoft Authenticator 可能会在手机上加密保存密钥。此时 Lockra 会把这些账号列为不支持，并说明原因；请改为在各个服务中重新设置。

目前没有已知的方法可以读取 iPhone 上 Microsoft Authenticator 中的账号。

## 添加到 Microsoft Authenticator

Microsoft Authenticator 每个二维码导入一个账号。

1. 打开「导出」，选择「Microsoft Authenticator」。
2. 勾选账号。Microsoft Authenticator 只接收 SHA1、6 位、30 秒周期的账号；其他账号显示为灰色，并说明原因。
3. 输入主密码，选择「生成二维码」。手机上开启了指纹解锁时，主密码可以留空，改用指纹验证。
4. 在手机上轻点「+ › 其他账户（Google、Facebook 等）」并扫描第一个二维码；之后在 Lockra 中选择「下一张」继续。

请核对手机上显示的验证码是否与 Lockra 在每个二维码旁显示的一致。
