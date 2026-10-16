---
layout: home
title: Lockra — 适用于 Windows、macOS、Linux 和 Android 的加密验证器与密码管理器
titleTemplate: false
description: Lockra 把两步验证码和密码保存在电脑和 Android 手机上的加密文件中，经端到端加密同步，每台设备上的内容保持一致。可以在 Lockra 与 Google 身份验证器、Microsoft Authenticator 之间迁移账号，用浏览器扩展填入登录项，并把加密备份写入你选择的文件夹。

hero:
  name: Lockra
  text: 两步验证码和密码，留在你自己的设备上。
  tagline: 适用于 Windows、macOS、Linux 和 Android 的加密验证器与密码管理器，浏览器扩展替你填入登录项及其验证码。端到端加密同步让每台设备上的账号保持一致；可以从 Google 身份验证器或 Microsoft Authenticator 迁入账号，并把加密备份放在你选择的位置。
  actions:
    - theme: brand
      text: 下载
      link: /zh/guide/install
    - theme: alt
      text: 快速开始
      link: /zh/guide/quick-start
    - theme: alt
      text: GitHub
      link: https://github.com/sunerpy/lockra

home:
  facts:
    - term: 支持平台
      text: Windows 10 和 11、macOS 11 及以上（Apple 芯片与 Intel）、Linux，均支持 x64 和 ARM64；以及 Android 8 及以上的 ARM64 手机。
    - term: 多端同步
      text: 每台设备上的账号保持一致，经内置中继或你自己的存储端到端加密同步。
    - term: 你的账号
      text: 验证码和密码保存在每台设备的一个加密文件中。Lockra 为更新连接 GitHub，开启同步后还会连接你选择的中继或存储，除此之外不连接任何地方。

  visual:
    home:
      light: /screens/codes-zh-light.webp
      dark: /screens/codes-zh-dark.webp
      width: 1440
      height: 900
      alt: Lockra 的验证码页，列出各个账号及其当前验证码和剩余时间。

  index:
    title: Lockra 的功能
    intro: 当前版本的全部功能，每一项都链接到详细说明的页面。
    groups:
      - name: 设备与同步
        items:
          - title: Windows、macOS、Linux 和 Android
            body: 电脑和手机上是同一个应用：Windows、macOS 和 Linux 的安装包支持 x64 与 ARM64，Android 8 及以上使用 APK 安装。
            status: available
            link: /zh/guide/install
          - title: 多设备同步
            body: 经 Lockra 中继、S3 兼容存储桶、WebDAV 文件夹，或在电脑之间经网盘同步文件夹进行端到端加密同步。保存同步空间的一方只能看到加密的文件。
            status: available
            link: /zh/backup/sync
          - title: 经 Lockra 中继同步
            body: 无需自备存储。内置中继或你自建的中继保存加密文件，无法打开其中任何一个。
            status: available
            link: /zh/backup/sync#经-lockra-中继同步
          - title: 手机扫码加入
            body: 在同步空间中的设备上显示邀请，用手机扫描即可加入，也可以粘贴配对链接。所有设备都丢失时，可以用恢复密钥找回同步空间。
            status: available
            link: /zh/backup/sync#添加手机
      - name: 验证码
        items:
          - title: 点击即可复制
            body: 点击一个账号，或用方向键选中后按 Enter。30 秒后，如果剪贴板中仍是这个验证码，就会自动清空。
            status: available
            link: /zh/accounts/codes
          - title: 提前显示下一个验证码
            body: 验证码在最后 5 秒内会同时显示下一个，不会输入一个即将失效的验证码。
            status: available
            link: /zh/accounts/codes#读取验证码
          - title: 搜索、分组与收藏
            body: 输入关键字查找账号，把账号归入分组，收藏的账号排在最前。按 Ctrl K 可以查找任意账号并复制验证码。
            status: available
            link: /zh/accounts/codes#查找账号
          - title: 分组折叠与账号颜色
            body: 分组可以逐个或一次全部折叠，每个账号可以设置颜色或自己的头像文字，在行内或右键菜单中即可收藏和编辑。
            status: available
            link: /zh/accounts/codes#按分组折叠
          - title: 支持所有标准账号
            body: 基于时间（TOTP）和基于计数器（HOTP）的验证码，SHA1、SHA256 或 SHA512，6 至 8 位，任意周期。
            status: available
            link: /zh/accounts/add
      - name: 密码
        items:
          - title: 登录项保存在同一个保险库中
            body: 用户名、密码、网址、备注和自定义字段，以及与每个登录项关联的验证码账号，电脑和手机上都可以使用。
            status: available
            link: /zh/passwords/
          - title: 每个登录项的保护级别
            body: 直接填充，先用主密码或系统验证确认，或者不交给浏览器扩展。
            status: available
            link: /zh/passwords/#保护级别
          - title: 密码生成器
            body: 生成随机密码和口令，遵循已知网站的密码规则，并在设备上保留生成历史。
            status: available
            link: /zh/passwords/#密码生成器
          - title: 从浏览器和密码管理器迁入
            body: 从 Chrome、Edge、Firefox、Safari、Bitwarden、1Password、LastPass 和 KeePass 导入，并可导出为 KeePass 数据库或 CSV 文件。
            status: available
            link: /zh/passwords/import-export
          - title: 浏览器扩展
            body: 在 Windows、macOS 和 Linux 上填入登录项及其验证码，分几页的登录也只需填一次，并提示保存新密码。Chrome、Edge 和 Brave 从 Lockra 的发布页加载扩展；Firefox 版要等商店页面上线。
            status: available
            link: /zh/passwords/browser
          - title: 全局快捷搜索
            body: 在任何应用中按下你设定的组合键，唤出 Lockra 的命令面板，复制用户名、密码或验证码。
            status: available
            link: /zh/passwords/#全局快捷搜索
      - name: 迁移账号
        items:
          - title: 从 Google 身份验证器迁入
            body: 拍下或截取它导出的二维码，把图片拖到窗口中。同一批中还缺哪几张二维码，Lockra 会一一提示。
            status: available
            link: /zh/transfer/google
          - title: 从 Microsoft Authenticator 迁入
            body: 这个应用没有导出功能，但 Lockra 可以读取已 root 的 Android 手机上它的数据库。工作或学校账号无法迁移。
            status: available
            link: /zh/transfer/microsoft
          - title: 链接、列表与二维码图片
            body: 粘贴 otpauth 链接、从剪贴板读取，或选择任意验证器的二维码图片或链接列表。
            status: available
            link: /zh/transfer/other-apps
          - title: 迁回手机
            body: 生成给 Google 身份验证器的迁移二维码、给 Microsoft Authenticator 的每个账号一张二维码，旁边显示当前验证码，方便与手机核对。
            status: available
            link: /zh/transfer/google#迁移到-google-身份验证器
      - name: 备份
        items:
          - title: 加密的备份文件
            body: 把整个保险库保存为 .lockrabackup 文件，使用主密码或单独的备份密码加密。
            status: available
            link: /zh/backup/
          - title: 自动备份
            body: 每次修改几秒后，加密备份会写入你选择的文件夹，并只保留最近几份。
            status: available
            link: /zh/backup/#自动备份
          - title: 合并或整体恢复
            body: 逐条合并备份中的账号，或者整体替换。替换之前，Lockra 会先保存当前保险库的副本。
            status: available
            link: /zh/backup/#从备份恢复
      - name: 安全
        items:
          - title: 主密码
            body: 保险库使用由主密码派生的密钥加密。连续输错密码后，再次尝试需要等待更长时间。
            status: available
            link: /zh/security/
          - title: 在本机记住
            body: 可以选择把密钥保存在系统钥匙串中，在这台电脑上解锁时无需输入主密码。
            status: available
            link: /zh/security/#在本机记住
          - title: Touch ID 与 Windows Hello
            body: 用本机记住的密钥解锁前，先验证指纹或 Windows Hello。
            status: available
            link: /zh/security/#touch-id-与-windows-hello
          - title: 自动锁定
            body: 默认无操作 5 分钟后锁定，也可以按 Ctrl L 立即锁定。
            status: available
            link: /zh/security/#锁定
          - title: 密钥不留在屏幕上
            body: 显示密钥或导出二维码前需要再次输入主密码（手机上也可以验证指纹），2 分钟后自动隐藏。在 Windows 和 macOS 上，此时截屏只会得到黑色窗口。
            status: available
            link: /zh/security/#屏幕上的密钥
          - title: 经过签名校验的更新
            body: 有新版本时，标题栏会提示；开启自动更新后，Lockra 在启动时下载新版本，并且只安装带有 Lockra 签名的安装包。
            status: available
            link: /zh/guide/updates#更新

  steps:
    title: 四步迁移你的账号
    items:
      - title: 创建保险库
        body: 启动 Lockra，设置主密码。主密码用于加密全部内容，无法找回。
      - title: 在手机上导出
        body: 在 Google 身份验证器中依次选择「转移账号」和「导出账号」，用另一台设备拍下二维码。
      - title: 导入照片
        body: 把图片拖到 Lockra 窗口中。预览列出每个账号，确认后即保存。
      - title: 复制验证码
        keys: [Ctrl, K]
        body: 查找账号后按 Enter，或在列表中点击它，验证码即复制到剪贴板。

  transfer:
    columns: [应用, 迁入 Lockra, 迁回应用]
    rows:
      - name: Google 身份验证器
        into: 它导出的二维码的照片或截图
        out: 迁移二维码，每张最多 10 个账号
        note: 30 秒周期或基于计数器，6 位或 8 位
      - name: Microsoft Authenticator
        into: 已 root 的 Android 手机上它的数据库
        out: 每个账号一张标准二维码
        note: SHA1，6 位，30 秒
      - name: 其他验证器
        into: otpauth 链接、列表与二维码图片
        out: 明文 otpauth 链接列表
      - name: Lockra
        into: 备份文件及其密码
        out: 备份文件
      - name: 浏览器和密码管理器
        into: 它们导出的密码，以及 KeePass 数据库
        out: KeePass 数据库或 CSV 文件
    caption: 保存之前，预览会逐条标明账号是新增、已在保险库中、与另一个账号同名但密钥不同，还是不支持，并说明原因。

  backup:
    items:
      - title: 与保险库同样加密
        body: 备份文件可以放在任何文件夹或云盘中，只有用创建时的密码才能打开。
      - title: 自动完成
        body: 最后一次修改 3 秒后写入你选择的文件夹，默认保留最近 10 份。
      - title: 放心恢复
        body: 合并会经过导入预览；替换之前会先保存当前保险库的副本。

  security:
    items:
      - title: 密钥派生
        body: Argon2id 使用 64 MiB 内存、3 轮迭代，把主密码转换为密钥。
      - title: 加密
        body: 账号使用 XChaCha20-Poly1305 加密，文件哪怕只改动 1 个字节也会被发现。
      - title: 剪贴板
        body: 复制的验证码会标记为不进入剪贴板历史，并在 30 秒后清空。
      - title: 系统账户
        body: 开启「在本机记住」后，任何能登录这台电脑的人都能打开保险库。

  platforms:
    title: 支持平台
    intro: Lockra 在 Windows、macOS、Linux 和 Android 手机上是同一个应用；iPhone 与 iPad 版在计划中。平台说明列出了它们之间的差异。
    columns: [平台, 安装包, 在本机记住, 显示密钥时阻止截屏]
    rows:
      - name: Windows 10 和 11
        status: available
        cells:
          - x64 安装程序或 MSI，ARM64 安装程序
          - 凭据管理器
          - 是
      - name: macOS 11 及以上
        status: available
        cells:
          - 适用于 Apple 芯片和 Intel 的 dmg 各一个
          - 钥匙串
          - 是
      - name: Linux
        status: available
        cells:
          - .deb、.rpm 或 AppImage，x64 与 ARM64
          - Secret Service（GNOME 钥匙圈、KWallet）
          - 否
      - name: Android 8 及以上
        status: available
        cells:
          - ARM64 的 APK
          - 仅配合指纹解锁
          - 始终阻止
      - name: iPhone 与 iPad
        status: planned
        cells:
          - "—"
          - "—"
          - "—"
    note: 安装包目前尚未签名，因此 Windows 和 macOS 在首次启动前会请你确认。

  privacy:
    title: 哪些内容会离开你的电脑
    intro: 除非你自己迁移或开启同步，否则没有任何内容离开。
    label: 保存位置
    items:
      - name: 你的账号
        value: 本机的一个文件中
        detail: 使用由主密码派生的密钥加密。Lockra 从不把它发送到任何地方。
      - name: 备份
        value: 你保存的位置
        detail: 以同样的方式加密。自动备份只写入你选择的文件夹；如果这个文件夹会同步，加密文件也会随之同步。
      - name: 同步
        value: 中继或你自己的存储（如已开启）
        detail: 每台设备把加密文件写入你设置的 Lockra 中继、存储桶或 WebDAV 文件夹。它们都无法读取任何账号、密钥或设备名称。
      - name: 网络
        value: 更新与你的同步
        detail: 你检查更新时，或开启自动更新后每次启动时，Lockra 向 GitHub 查询最新版本，不发送账号中的任何内容。Lockra 没有用户账户或遥测，内置中继只保存加密的文件，字体和图片都随应用提供。
      - name: 浏览器扩展
        value: 只连接本机的 Lockra
        detail: 它向同一台电脑上的 Lockra 请求，两者之间的连接经过加密；扩展不保存密码，也不经网络发送任何内容。

  scope:
    title: 有意不做的功能
    intro: Lockra 把你的验证码和密码留在你的设备上。
    items:
      - 用户账户，或能读取账号的服务器。同步无需注册账户，中继或存储里只有加密的文件。
      - Steam 令牌等非标准验证码。
---

## 一分钟看 Lockra

在网站上用 Lockra 按规则生成的密码注册，开启这个网站的两步验证，再分几页登录，登录项和验证码自动填入；然后是命令面板（`Ctrl K`）、密码生成器（`Ctrl G`）和主题。录制时使用的是真实的应用和浏览器扩展，网站是虚构的演示站点。

<ScreenVideo src="/screens/tour-zh.mp4" poster="/screens/tour-zh-light.webp" width="1280" height="800" label="一分钟的 Lockra 录屏：用生成的密码在网站上注册，添加它的两步验证码，登录时自动填入登录项和验证码，然后是命令面板、密码生成器和主题。" />

## 密码，在浏览器中自动填入

自 0.10.0 版起，Lockra 把登录项与验证码保存在同一个保险库、同一份备份中，电脑和手机上都可以使用。适用于 Chrome、Edge 和 Firefox 的浏览器扩展会先填入登录项，再填入它的验证码，分几页的登录会逐页接着填；扩展还会提示保存和更新密码，并在注册表单中按网站的规则生成密码。扩展本身不保存密码，而是经两者之间加密的连接向同一台电脑上的 Lockra 请求，每一次填充都由 Lockra 决定。登录项可以从浏览器和密码管理器导入，也可以导出为 KeePass 数据库或 CSV 文件。

<ScreenFigure src="/screens/passwords-zh-light.webp" dark="/screens/passwords-zh-dark.webp" width="1440" height="900" alt="密码页：顶部是搜索框和「添加登录项」，下面是筛选按钮和按分组排列的登录项，右侧的详情栏显示 GitHub 登录项的用户名、以圆点显示的密码和所关联账号的验证码。" />

[密码](/zh/passwords/) · [浏览器扩展](/zh/passwords/browser) · [导入与导出登录项](/zh/passwords/import-export)

## 多端同步

Lockra 可以在 Windows、macOS、Linux 和 Android 上运行，同步让每台设备上的账号、登录项、分组和收藏保持一致。每台设备在共享的内容离开设备之前先完成加密；内置中继、自建中继、S3 兼容存储桶、WebDAV 文件夹，或电脑之间使用的网盘同步文件夹，保存的都只是密文。手机扫描电脑上的二维码即可加入，无需注册任何 Lockra 账号。

<ScreenFigure src="/screens/devices-zh-light.webp" dark="/screens/devices-zh-dark.webp" width="1440" height="900" alt="电脑和手机上的 Lockra，显示相同的账号和相同的验证码。" />

[多设备同步](/zh/backup/sync) · [在 Android 上安装](/zh/guide/install#android)

<HomeIndex />

<HomeSteps />

<SplitBlock proof="transfer">

## 迁入账号，也能迁回去

Google 身份验证器以二维码的形式导出账号，Lockra 可以从照片或截图中读取，一次可以选择多张。Microsoft Authenticator 没有导出功能；在已 root 的 Android 手机上，Lockra 改为读取它的数据库。其他验证器可以提供 otpauth 链接或二维码。

反向迁移时，Lockra 显示给 Google 身份验证器的迁移二维码，以及给 Microsoft Authenticator 的每个账号一张二维码，旁边同时显示各账号的当前验证码，方便与手机核对。登录项可以经同样的预览从浏览器和密码管理器迁入，也可以导出为 KeePass 数据库或 CSV 文件。

[Google 身份验证器](/zh/transfer/google) · [Microsoft Authenticator](/zh/transfer/microsoft) · [其他应用](/zh/transfer/other-apps) · [登录项](/zh/passwords/import-export)

</SplitBlock>

<SplitBlock proof="backup" flip>

## 无需记挂的备份

开启一次自动备份并选择文件夹。此后每次修改几秒后，Lockra 都会在那里写入一份加密副本，并只保留最近几份。如果选择 OneDrive、Google Drive 或 iCloud 的文件夹，副本会同步到你的其他设备，而 Lockra 本身无需联网；多设备同步则经 Lockra 中继或你自己的存储，让每台设备上的账号本身保持一致。

恢复时，可以通过与导入相同的预览把备份合并到现有账号中，也可以在保存当前保险库的副本后整体替换。

[备份与恢复](/zh/backup/) · [多设备同步](/zh/backup/sync)

</SplitBlock>

<SplitBlock proof="security">

## 为保密而设计

保险库是一个加密文件。没有主密码就无法读取，哪怕改动其中 1 个字节也会被发现。Lockra 在无操作时自动锁定，清除自己复制的验证码和密码；账号的密钥，以及保护级别为「填充前验证」的登录项的密码，只有再次输入主密码（或在手机上验证指纹）后才会显示。

[Lockra 如何保护你的账号](/zh/security/) · [隐私](/zh/privacy)

</SplitBlock>

<HomePlatforms />

<HomePrivacy />

## 安装

各平台的安装包、校验和与构建证明都在[发布页](https://github.com/sunerpy/lockra/releases)。[安装指南](/zh/guide/install)介绍了每个平台，包括如何首次启动未签名的应用。

<HomeScope />
