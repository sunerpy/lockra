# 开发者

本页面向希望构建、修改 Lockra 或了解其工作原理的读者。

## 源代码

代码托管在 GitHub 的 [sunerpy/lockra](https://github.com/sunerpy/lockra)，采用 Apache License 2.0 许可。Lockra 由 Rust 核心、Tauri 2 外壳和 React 界面组成：

| 部分                             | 内容                                                      |
| -------------------------------- | --------------------------------------------------------- |
| `crates/lockra-otp`              | HOTP 与 TOTP（RFC 4226 和 6238）、Base32、otpauth 链接    |
| `crates/lockra-vault`            | 保险库和备份共用的加密容器                                |
| `crates/lockra-transfer`         | Google 迁移二维码、Microsoft 数据库、otpauth 列表、二维码 |
| `crates/lockra-core`             | 应用核心：保险库会话、导入、导出、备份、设置              |
| `crates/lockra-bridge`           | 界面与核心之间的契约                                      |
| `apps/desktop`                   | Tauri 外壳与 React 应用                                   |
| `apps/mobile`                    | 开发中的 Android 应用：Tauri 外壳与 React 应用            |
| `packages/ui`、`packages/shared` | 设计系统；契约、翻译和测试用后端                          |

## 构建

需要 Rust 1.98、Node 20.19 及以上与 pnpm 9；在 Linux 上还需要 WebKitGTK 4.1。`make check` 另外需要
Python 3.9 及以上、cargo-llvm-cov、cargo-deny、actionlint 和 shellcheck。

```bash
pnpm install --frozen-lockfile
pnpm --filter @lockra/desktop tauri dev   # 带热重载的应用
make check                                # CI 运行的全部检查
make help                                 # 其余命令
```

仓库中的 `CONTRIBUTING.md` 和 `AGENTS.md` 说明了开发流程和规则。

## 设计文档

以下文档介绍 Lockra 的实现方式，以英文撰写，与代码保存在一起。

- [架构](/zh/dev/architecture)：组成部分、IPC 契约与测试
- [文件格式](/zh/dev/formats)：保险库、备份、设置，以及每种导入和导出格式
- [安全模型](/zh/dev/security)：保护的对象、保护方式与残余风险
- [发布](/zh/dev/release)：发布流程与仓库设置

## 本站

本站的页面以中英文保存在仓库的 `docs/site/` 目录中；对应用的修改会在同一个拉取请求中更新相应页面。
