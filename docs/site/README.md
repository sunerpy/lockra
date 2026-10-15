# docs/site — the pages of firlab.app/lockra

This directory holds the words of the Lockra website, published at <https://firlab.app/lockra/>:
English at `docs/site/`, Chinese at `docs/site/zh/`, one file per page and the same path in both
languages. Everything else (the VitePress configuration, the theme, the components and the
deployment) lives in [`sunerpy/firlab`](https://github.com/sunerpy/firlab) under `lockra/`. The
site has no domain of its own: firlab builds it with the base `/lockra/` and publishes it inside
firlab.app.

| Path                                                                                   | Published at                                                                                            |
| -------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- |
| `index.md`, `zh/index.md`                                                              | `/lockra/`, `/lockra/zh/` (home pages)                                                                  |
| `guide/`, `accounts/`, `passwords/`, `transfer/`, `backup/`, `security/`, `reference/` | the user guide (firlab's sync script copies each of these directories: a new one needs its entry there) |
| `privacy.md`, `roadmap.md`, `developers.md`                                            | reference pages                                                                                         |
| `docs/{architecture,formats,security,release}.md`                                      | `/lockra/dev/*`, as they are; `/lockra/zh/dev/*` is a generated Chinese pointer                         |
| `public/`                                                                              | the site's root (`/lockra/lockra-logo.svg`, `/lockra/screens/*.webp`)                                   |

Links inside the pages are written from the site's root without the base (`/guide/install`,
`/zh/backup/`); VitePress adds `/lockra/`.

## How a change reaches the site

1. A pull request that touches these files or the design documents runs
   `.github/workflows/docs-site.yml`: it checks out the public firlab repository, syncs this
   directory into it and builds the site. A dead link, a page without its other language, an
   unknown component, a word from the lists below or a malformed home page fails the check.
2. After the merge, `.github/workflows/publish-site.yml` runs firlab's
   `lockra/scripts/sync-lockra-docs.sh` and commits the result to firlab's `main` as
   `docs(lockra): sync from lockra@<sha>`. It needs the secret `FIRLAB_DOCS_TOKEN`: a
   fine-grained token for `sunerpy/firlab` only, with Contents read and write. Without it the
   workflow fails with a message; nothing else is published.
3. firlab's `deploy.yml` builds firlab.app with the Lockra site inside it under `/lockra/` and
   deploys both to GitHub Pages.

The footer of every page names the Lockra commit its content came from.

Both workflows run the sync script from firlab's `main`, so they pass only once firlab carries
`lockra/`. Merging the firlab change that adds it publishes the site, whose pages describe the
released app and link to the releases page, so it lands after Lockra's first release is public
(docs/release.md, "The documentation site"); until then the two workflows fail and publish
nothing.

## Preview

```bash
git clone https://github.com/sunerpy/firlab ../firlab    # once
../firlab/lockra/scripts/sync-lockra-docs.sh "$PWD"
cd ../firlab/lockra && pnpm install --frozen-lockfile && pnpm dev   # http://localhost:5173/lockra/
```

Run the sync again after each edit. It stops with a message when a page has no counterpart in the
other language, uses a component the site does not register, or uses a word from the lists below.

## Writing

- Say what the reader does, sees and gets. User pages never name the internals: no crate, module
  or command names (`lockra-core`, `UiState`, IPC, `§`). `developers.md` is the exception.
- The first sentence of a page says what the page helps with.
- Use standard, readable written language. Chinese follows the register of Apple's and
  Microsoft's Chinese documentation: 「尚未设置」「无法读取」「仅保存在本机」. No colloquial words
  (还没、没能、免得、搭的、咋、啥、看看、试试、搞定; gonna, stuff, simply), no stacked
  explanations, no promises in the voice of a person. The sync script rejects these words, in
  quoted labels too: describe a label that contains one (「还没有写入连接文件」) instead of quoting
  it.
- One word per concept, the word the app uses. Chinese: 保险库, 主密码, 账号, 验证码, 密钥,
  分组, 收藏, 导入, 导出, 二维码, 备份, 自动备份, 在本机记住, 锁定. English: vault, master
  password, account, code, secret, group, pinned (**Pin to top**), import, export, QR code,
  backup, automatic backup, **Remember on this device**, lock.
- Password management (from 0.10.0) adds these. Chinese: 登录项, 「密码」页, 网站 and 匹配方式,
  保护级别 (「直接填充」「填充前验证」「不交给扩展」), 密码生成器 and 生成历史, 浏览器扩展, 浏览器集成,
  配对, 弹窗, 网站访问权限, 网站名单, 托盘, 后台运行 (「关闭窗口后继续运行」). English: login, the
  Passwords page, sites and matching, protection (**Fill directly**, **Verify first**, **Not for
  the extension**), password generator and generated passwords, browser extension, browser
  integration, pair, popup, site access, site lists, tray, running in the background (**Keep
  running after the window closes**). A login is a favorite (**Favorite**) where an account is
  pinned, and the Passwords page's filter still reads **Pinned**; in Chinese both are 收藏. An
  account's secret is 密钥, a login's password 密码.
- Name a control as the app labels it: in bold in English (**Save backup…**), in 「」 in Chinese
  (「保存备份…」), with `›` between levels (**Settings › Security › Auto-lock**). Read the label
  from `packages/shared/src/i18n/en.ts` and `zh-CN.ts`, not from memory.
- Chinese pages keep English only for product names and keys, with a space between Chinese and
  Latin text. Headings end without a full stop.
- Every number has a source in the code: the three-second backup delay, the 10 accounts per
  Google code, the two minutes before a secret hides, the Argon2id cost. Change the page when the
  number changes.
- Both languages have the same sections in the same order. Chinese headings make Chinese anchors
  (`#自动备份`); links from Chinese pages use them.
- A feature that is not released goes in its own section with `<StatusTag status="building" />`
  and is never described as available.
- No real address, host name or secret, in the text or in a screenshot: accounts use
  `example.com` addresses and made-up secrets.

## Components

Pages may use these components and no others:

| Component                                                                           | Use                                                       |
| ----------------------------------------------------------------------------------- | --------------------------------------------------------- |
| `<StatusTag status="available \| building \| planned" />`                           | release state, shown as text                              |
| `<ScreenFigure src dark? width height alt caption? />`                              | a screenshot; `dark` is the same screen in the dark theme |
| `<Badge>`                                                                           | VitePress's own badge                                     |
| `HomeIndex`, `HomeSteps`, `SplitBlock`, `HomePlatforms`, `HomePrivacy`, `HomeScope` | the home pages only; they render the `home:` frontmatter  |

`<SplitBlock proof="transfer | backup | security" flip?>` puts the Markdown inside it beside the
evidence of the same name from the frontmatter; `flip` puts the evidence on the left.

## The home pages

The words of both home pages live in their frontmatter: VitePress's `hero:` (name, text, tagline,
buttons) and a `home:` block that the components render. The build checks `home:` against
`lockra/src/.vitepress/theme/data/home-schema.ts` in firlab and fails on a missing field or an
unknown one.

| Key                  | Holds                                                                                               |
| -------------------- | --------------------------------------------------------------------------------------------------- |
| `facts`              | the lines under the tagline: `term`, `text`                                                         |
| `visual`             | `home`: the codes page in `light` and `dark`, `width`, `height`, `alt`                              |
| `index`              | `title`, `intro`, `groups[]`: `name`, `items[]`: `title`, `body`, `status`, `link`                  |
| `steps`              | `title`, `items[]`: `title`, `body`, `keys` (optional)                                              |
| `transfer`           | `columns` (three), `rows[]`: `name`, `into`, `out`, `note` (optional); `caption`                    |
| `backup`, `security` | `items[]`: `title`, `body`                                                                          |
| `platforms`          | `title`, `intro`, `columns`, `rows[]`: `name`, `status`, `cells` (one fewer than `columns`); `note` |
| `privacy`            | `title`, `intro`, `label`, `items[]`: `name`, `value`, `detail`                                     |
| `scope`              | `title`, `intro`, `items[]`                                                                         |

Status is one of `available`, `building` or `planned`. The home pages carry no version number:
the buttons link to the install guide and the releases page.

## Screenshots

Screenshots are WebP files in `public/screens/`, named `<page>-<lang>-<scheme>.webp`, 1440 × 900 at
device pixel ratio 1, WebP quality 85: the `light` figure shows the app in 澄空, the `dark` one in
澄空 · 夜, Lockra's default theme on a light and on a dark system. They come from the real app with
made-up accounts:

```bash
make site-screens     # scripts/capture-site-screens.sh docs/site/public/screens
```

The script builds the app, starts it under Xvfb once per language in throwaway folders, and lets
`scripts/smoke/site_screens.py` create a vault, add eight accounts, run an automatic backup and
read a Google Authenticator export code from the clipboard. It captures the codes page, the
backup page and the import preview, each in 澄空 and in 澄空 · 夜. It needs what
`make smoke-desktop` needs (Xvfb, xdotool, xclip, dbus-daemon, tauri-driver, WebKitWebDriver) plus Pillow and
cairosvg for Python, and Node 22 or later.

The phone app blocks screenshots, so its screens come from its dev build (the in-memory core) in a
Chrome reached over the DevTools protocol: `scripts/smoke/phone_screens.mjs` gives it the same
eight accounts, with real codes and the clock of the desktop shot, and
`scripts/smoke/compose_screens.py` puts the screenshots together, unredrawn:

| File                           | Shows                                                    |
| ------------------------------ | -------------------------------------------------------- |
| `devices-<lang>-<scheme>.webp` | the codes page on a computer and a phone, the same codes |
| `phones-<lang>-<scheme>.webp`  | the phone's codes page, adding an account and sync       |

```bash
LOCKRA_CDP=127.0.0.1:9222 make site-screens   # a Chrome started with --remote-debugging-port=9222
```

`LOCKRA_CDP` is required. A Chrome on another machine also needs `LOCKRA_PHONE_BIND=0.0.0.0` and
`LOCKRA_PHONE_HOST=<this machine's address>`, for the phone dev server it loads. The two READMEs
show `devices-<lang>-light` and `phones-<lang>-light` from here too, the light ones only, also to a
reader in dark mode.

Look at every image before committing it, and capture again when the interface's text or layout
changes.
