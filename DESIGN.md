# Lockra design system

Lockra's interface is derived from Voltip's (`sunerpy/voltip`, Apache-2.0): the same four
classic palettes (four more were drawn for Lockra, see Palettes), accent system, fonts, radii, window chrome and component anatomy, applied to an
authenticator. The values below are a reading guide; **the code is the truth**:
`packages/ui/src/tokens.css` (every colour, font, radius and shadow) and
`packages/ui/src/components/*`. A rule here that the code contradicts is a bug in one of the two.

## 1. Tokens

### Palettes

Eight themes switched by `data-theme` on `<html>`; every component reads CSS variables through the
Tailwind utilities declared in `@theme inline` (`bg-canvas`, `text-fg-muted`, `border-border`, …).

| Theme         | Name            | Character                                                          |
| ------------- | --------------- | ------------------------------------------------------------------ |
| `light`       | 明亮 · 白瓷     | cool white canvas `#f2f3f6`, white surfaces, ink `#1b1d22`         |
| `dark`        | 暗黑 · 夜灯     | near-black canvas `#0c0d11`, surfaces `#14161b`                    |
| `warm`        | 暖纸 · 手稿     | paper `#f5efe4`, ochre accent `#b85c38`                            |
| `graphite`    | 石墨 · 仪表     | slate canvas `#22252b`, surfaces `#2a2e35`                         |
| `glass`       | 澄空 · 极光玻璃 | an aurora on `#eef2fa` behind frosted white (66 %), blue `#3d7bff` |
| `glass_night` | 澄空 · 夜       | the aurora on `#0b1020`, white at 7 %, blue `#6e9bff`              |
| `paper`       | 墨白 · 纸上墨迹 | paper `#f7f6f2`, white cards, ink `#111111` for text and meters    |
| `paper_night` | 墨白 · 夜       | `#141311`, cards `#1c1b19`, ink `#f2efe8`                          |

Following the system switches within a pair (Settings › 外观 draws each as a column, light above
dark): 明亮 ↔ 暗黑, 澄空 ↔ 澄空 · 夜, 墨白 ↔ 墨白 · 夜. 暖纸 and 石墨 have no pair: picking one stops
following (as does a tile of the other scheme: what was picked is what shows), and a vault that
follows the system with one of them set shows 明亮 or 暗黑, as before the pairs (`pickTheme`,
`resolveTheme` in `packages/ui/src/theme.ts`). New installs start on 澄空, following the system;
an install from before 0.9.1 that never saved a setting keeps 明亮, its default
(`SettingsStore::load_at_start`). The pages paint 澄空 until the settings arrive (`index.html`).

### Structure

A theme is more than its roles: each sets the variables below (the classic four share one block that
keeps the look they had). They reach the components through `hairline` (the default 1 px edge, in
the components layer so that an element's own `border-danger` or `hover:border-…` wins),
`app-canvas`, `panel` (card shadow and frost), `sidebar-shell`, `overlay-frost` and
`tile-copied`, and through `bg-surface` / `bg-nav` / `bg-overlay`, `rounded-6/10/14` and
`shadow-primary`. `bg-surface` is the fill, translucent in 澄空: text in the surface's colour (a
danger button's label) takes `text-surface-solid`, the opaque role, and `theme.test.ts` rejects
text in the fill. `prefers-reduced-transparency` turns the frost and the translucent fills off.

| Variable                                                                      | Classic                                 | 澄空 (and 夜)                                                            | 墨白 (and 夜)                                           |
| ----------------------------------------------------------------------------- | --------------------------------------- | ------------------------------------------------------------------------ | ------------------------------------------------------- |
| `--surface-fill` / `--nav-fill` / `--overlay-fill`                            | the opaque roles                        | white at 66 / 50 / 90 %; at night 7 / 5 %, the overlay `#161c33` at 92 % | the opaque roles                                        |
| `--edge` (hairlines)                                                          | `border`                                | white at 90 % (night 12 %)                                               | `border`                                                |
| `--canvas-image`                                                              | none                                    | four radial gradients                                                    | none                                                    |
| `--card-shadow` / `--surface-blur`                                            | none                                    | soft shadow / blur 22 px                                                 | none                                                    |
| `--radius-control` / `--radius-card` / `--radius-dialog`                      | 6 / 10 / 14                             | 12 / 18 / 22                                                             | 10 / 16 / 18                                            |
| `--sidebar-inset` / `--sidebar-radius`                                        | 0 (a rule on the right)                 | 12 / 20 (it floats)                                                      | 0                                                       |
| `--primary-shadow`                                                            | none                                    | a blue glow                                                              | none                                                    |
| `--meter` (ring and card bar)                                                 | accent                                  | accent                                                                   | ink (`fg`)                                              |
| `--chip-on-bg` / `--chip-on-fg` (the filter chip that is on)                  | accent-soft / accent-text               | white glass (`nav-active`) / `fg`                                        | ink: `primary` / `primary-fg`                           |
| `--tile-copied-*` / `--tile-pill-*` (a card just copied), `--tile-hover-edge` | the card, an accent pill; the edge kept | the same                                                                 | the card turns to ink, plain text; an ink edge on hover |
| `--row-pill-bg` / `--row-pill-fg` (a row just copied)                         | accent-soft / accent-text               | the same                                                                 | ink: `fg` / `surface`                                   |
| `--logo-body` / `--logo-dots` (the in-app logo)                               | `#339cff` / white                       | `#3d7bff` (night `#6e9bff`) / white                                      | `#2142ff` (night `#8fa3ff`) / white                     |

Roles (same names in every theme): `canvas surface inset inset2 border border-strong fg fg-muted
fg-subtle primary primary-fg accent accent-fg accent-soft accent-text accent-text-hover thumb ok
ok-soft ok-text danger danger-soft warning warning-soft info info-soft nav nav-active track
keycap-bg keycap-border scrim qr-plate mark` and the shadows `pop` (menus, popovers, toasts), `win`
(dialogs) and `thumb` (the switch). `track` is Voltip's `led-off` (the unlit part of progress
bars), renamed for what it does here; `qr-plate` is white in every theme (a QR code's quiet
zone); Voltip's other dictation-only roles (LED levels, pill,
waveform, desktop mock, diff) are not carried over.

### Accent

`data-accent` on `<html>`: `default` (the theme's own: `#339cff` in light, dark, graphite; ochre
in warm) or `blue green yellow pink orange purple ink`. Every theme × accent pair has precomputed
`accent / accent-fg / accent-soft / accent-text / accent-text-hover`: `accent-fg` is black or white,
whichever contrasts more; `accent-text` and its hover reach 4.5:1 on surface, canvas, inset and
`accent-soft` (checked by `theme.test.ts`). Filled accent shapes in light themes may sit below 3:1
against white, as in Codex: state is never carried by the fill alone.

### Type

- `font-ui`: Instrument Sans, then Noto Sans SC for CJK, then the platform fallbacks.
- `font-mono`: JetBrains Mono (codes, secrets, keycaps, file names, numbers).
- All three ship with the app (`@fontsource-variable/*`); nothing is fetched (CSP forbids it).
- Base size is the `--ui-font-size` knob (12–18 px, default 14; Settings › Appearance).
- Scale: page title 26 px bold (the page's own header; 30 px on the phone); section label 14 px
  medium; descriptions 12 px `fg-muted`; eyebrows 11 px mono uppercase `fg-subtle`; codes mono
  semibold tabular, 26 px in the list (22 px on the phone's cards), 54 px on 「最近使用」 (44 px on
  the phone), 32 px in the export check list, split 3-3 (six digits), 4-4 (eight), 3-4 (seven).

### Shape, depth, motion

- Radii `4 6 10 14 20 999` (`rounded-4` … `rounded-pill`). Cards are `rounded-10`, inputs and
  buttons `rounded-6`, dialogs `rounded-14`; those three take the theme's radius (6 / 10 / 14 in
  the classic four, see Structure). Anything that must keep its shape whatever the theme (the
  account avatar, the theme previews' frame) sets its own pixels.
- Cards have **no shadow**: a 1 px hairline (`hairline`, the theme's edge). Shadows only lift
  overlays; 澄空's frosted cards are the exception (`panel`).
- Density knob `data-density`: rows 32 px (default) or 28 px (compact), card padding 16 / 12 px.
- Motion: 150 ms colour transitions on interactive elements; the countdown ring moves with a 1 s
  linear transition. `data-reduce-motion="true"` (Settings or the OS preference) turns every
  animation and transition off.

### Colour rules

- **Green (`ok`) is for 8 px status lamps only** (unlocked, backup healthy). Never a fill.
- `accent`: links (`accent-text`, no underline), the countdown ring, selection, focus outline
  (`accent-text`, 2 px, offset 1), progress, primary toggles.
- `warning`: a code in its last five seconds (ring, card bar and digits; the 12 px seconds beside the
  bar stay `fg-muted`, since `warning` is under 4.5:1 there).
- `danger`: destructive actions, failed backups, wrong passwords.
- **No hex literal in a TSX file** (`scripts/check-no-literal-colors.sh`), the logo included: inside
  the app it sits on the theme's card and takes `--logo-body` / `--logo-dots`.

### Account colours

Ten pairs per theme, `--tag-{red orange amber green teal blue indigo purple pink gray}` (a soft
fill) and `--tag-…-text` (its text), picked through `data-tag` and drawn with `bg-tag-bg` and
`text-tag-fg`. They identify an account (its avatar, the swatches of **Edit…**) and are never a
state: green here is not `ok`. Every pair reaches 4.5:1 (`theme.test.ts`); without `data-tag` the
avatar keeps the neutral `inset2` / `fg-muted`. An account left on **Automatic** takes one of the
nine colours other than grey from a hash of its name, so a service has the same colour everywhere.

## 2. Spatial model

The window is frameless (`decorations: false`; macOS keeps its traffic lights through
`titleBarStyle: "Overlay"`), min 960 × 600.

- One layout in every theme (boards A1, A2 of Penpot 「Lockra 界面方向」, owner's choice
  2026-10-11): a theme changes colours and materials (Structure), never where things are.
- Shell = `fixed-sidenav-shell` + `scroll-body-shell`: a 232 px sidebar (56 px icon rail when
  collapsed) on the left; on the right a grid `grid-template-rows: auto minmax(0, 1fr)` of the
  40 px title bar and the page body. The title bar is the drag strip, the update note and the
  window buttons, with no fill and no rule; each page starts with its own `PageHeader` (title,
  a line of facts, its main actions). There is no shortcut footer: the keys are in the buttons'
  tooltips (锁定 · Ctrl L, 设置 · Ctrl ,, 添加账号 · Ctrl N) and in the command palette.
- Sidebar: the groups 保险库 (验证码 and, where the build offers password management, 密码 with the
  `lock` icon, each with a count pill), 迁移 (导入, 导出) and 数据 (备份, and 同步, which opens
  Settings › 同步), rows of about 40 px (the board draws 44; 40 keeps the foot on a 600 px window),
  the current one raised on the theme's card shadow. Its foot is a status card
  (the lamp and 已解锁, the auto-lock, the sync) over three square buttons: 锁定, the theme (each
  click the next one) and 设置. The icon rail keeps the buttons, stacked, and drops the card.
- **Only the page body scrolls.** Its element carries `min-height: 0` (`min-h-0`), or a long list
  grows the window's row instead of scrolling. Sidebar and title bar never scroll.
- Grid tracks use `minmax(0, 1fr)`, never bare `1fr` (a bare track takes its widest content's
  width and overflows a narrow window).
- The sidebar brand row and the title bar form one 40 px drag strip (`data-tauri-drag-region`).
  The title bar's right slot holds the update note (Voltip's `UpdateBadge`: an accent-soft pill,
  `新版本 0.3.0` / `下载中 36%` / `重启以更新`), which opens the update dialog; it is absent when
  there is nothing to update.
- Overlays (`overlay-stack`): dialogs, the command palette, the QR viewer and toasts stack over
  the shell with a 28 % scrim; Esc closes only the top one.
- Page body: max content width 1040 px, 24 px horizontal padding, 16 px between cards.
- 验证码 (board A1): the header with a search pill and the 「添加账号」 pill; 「最近使用」 (the account
  copied last) beside 「保险库状态」, 5 : 3, both hidden while searching, selecting or reordering;
  the filters (全部, 收藏, each group, 未分组) with the list's tools (reorder, list or cards, sort and a
  ⋯ menu for 选择, 按分组显示, 全部折叠, 全部展开, and 置顶 for the filter shown); the list in one
  card, rows 64 px. Sections (with their folding headings) belong to 全部 only: any other filter is
  one flat list.
- The pinned group (置顶分组, Settings › 通用, this device's): its chip comes first, with a pin
  (收藏 keeps its star), and the page opens on it; in 全部 its section heads the list, and with 收藏
  pinned the pinned accounts gather there, out of their groups, in a section that does not fold.
  While reordering it stays first; every other section moves, 未分组 among them.
- 密码 (boards 01–05 of Penpot 「Lockra 密码管理」): the header as on 验证码 (the facts read N 个登录项
  · N 个分组 · the sync), the search pill (`min(26rem, 36vw)`, the `/` keycap; absent while there are
  no logins) and the 「添加登录项」 pill, a menu: 新建登录项… · Ctrl N, 生成密码… · Ctrl G, then
  从其他应用导入…. The tools: the filters (全部, 收藏, each group, 未分组; shown when there is more
  than one), the 标签 menu button (`TagFilter`: tags with counts, several at once, a login must
  carry all), then on the right the sort `Select` (按名称, 最近使用, 最近修改, 最近添加; favourites
  first in each) and, in 全部 with groups, a ⋯ for 全部折叠 and 全部展开. Under them a grid
  `minmax(0, 5fr) minmax(0, 3fr)`, 14 px apart: the list in one card (`PasswordRow`, 64 px; sections
  with folding headings in 全部 only) and the detail pane (`ItemDetail`), sticky 14 px under the
  title bar, whose card scrolls on its own: the one exception to "only the page body scrolls". A
  click or ↑ ↓ selects (the pane shows it), Enter copies the password, a right click or the
  context-menu key opens the row's menu. States: no logins (a card with an `EmptyState`, 导入 and
  添加登录项, no search), loading (`SkeletonRows`, only after 300 ms), failed (a danger
  `Banner` with 重试, the last list kept), no match (a compact `EmptyState` offering a login of
  that name).
- The detail pane: a 44 px avatar (36 in a pane under 300 px, where 编辑 also becomes a bordered
  28 px `IconButton`), the title 18 / 600 (16 narrow) with the star, the group · tags in 12.5 px
  `fg-muted`, 编辑 (`Button outline sm`) and ⋯. Then, as they apply: the `shieldCheck` line of a
  填充前验证 login, the conflict (`Banner warn bar`: the other version hidden until 显示, 改用这个
  primary, 保留当前 outline) or the read-only `Banner neutral`, and the `DetailSection`s 登录信息
  (`FieldRow`s, secrets as dots, the linked code with its ring from the code frames), 网站
  (`UrlRow`: the address and its matching as a `Badge`), 备注 and 自定义字段 (dots until 显示, hidden
  again after 120 s), 整理, 填充与保护 and 记录 (`KeyValueRow`s).
- The phone (board A2): a 30 px title with 已解锁 · N 个账号 and round 添加账号 and 锁定 buttons; the
  search pill; 「最近使用」; the filters, scrolling sideways, then reorder; each row a card of its
  own (its ⋯ always shown: there is no hover); a floating dock over the list, which lifts the
  toasts above it (`--toast-bottom`): 验证码, the raised 56 px `primary` button (扫码 on the codes,
  新建登录项 with `plus` on the logins), 密码 (`lock`, where the build offers password management)
  and 设置, in 70 px cells, the current one `accent-text`.
- The phone's logins (boards 20–29): the same header with 已解锁 · N 个登录项 and round 生成密码
  (`dice`) and 锁定 buttons, the search pill, the filters scrolling sideways with 标签, sections that
  fold, and each login a `PasswordRow card` (70 px, `rounded-14`): a tap opens the login, its 40 px
  button copies the password, ⋯ or a long press opens a `BottomSheet` of `ActionRow`s (复制用户名,
  复制密码, 复制验证码 with the current code, 显示密码, 编辑, 删除…). A login's page follows the
  detail pane in 40 px touch targets; the password shows on a page of its own; the edit page puts
  保存 in the bar, has no 外观 and no 自动填充 (one line says the desktop sets them), and asks for
  the fingerprint, or the master password, through a sheet.
- Toasts: a pill in `fg` at the bottom centre; a copy names the account
  (「已复制 GitHub 的验证码 · 30 秒后清空剪贴板」).

## 3. Primitives

Ported from Voltip (same anatomy and props, Voltip-only options removed): `Icon Lamp LampText
Eyebrow Card Panel Badge Chip Button IconButton Toggle Segmented Input Select Menu Popover Keycap(s)
Banner StatusRow SettingsLayout EmptyState Progress Table ThemeTile ThemeSwitch Dialog Toast
CommandPalette Sidebar Toolbar TitleBar OptionCard`, plus `Logo`: a padlock under its shackle, its
body showing a masked code (three dots). Inside the app it sits on the theme's card (`bg-surface`,
`hairline`, `panel`: frosted in 澄空, paper in 墨白), the shackle in `fg`, the body the theme's own
blue (`--logo-body`, whatever the accent), the dots `--logo-dots`. The app icon
(`apps/desktop/src-tauri/icons/icon.svg`, the Android icon sources, the site's `lockra-logo.svg`)
puts a white padlock on the brand's blue-violet tile (`#3D7BFF` → `#7C5CFF`, dots `#5B6BFF`). Both
were the owner's choice on 2026-10-11 (Penpot 「Lockra 界面方向」 › 「00 · Logo」, options B and C).

New for Lockra:

| Primitive          | What it is                                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------------------------------------- |
| `OtpCode`          | the code in mono, grouped; `masked` shows `••• •••`; `warning` tone in the last 5 s                                 |
| `CountdownRing`    | a ring (20 px, stroke 2.5 by default; 30 / 3.5 in the rows, 26 / 3 on the phone's, 80 / 6 on 「最近使用」), the     |
|                    | `--meter` stroke shrinking over the window, warning ≤ 5 s, static when reduce-motion                                |
| `EntryAvatar`      | rounded square (24–44 px, its own corner) with the issuer's first letter on its colour; never a brand logo          |
| `EntryRow`         | avatar · issuer/account · code · ring · pin and menu, 64 px; the whole row copies on click/Enter; a pinned account  |
|                    | has a star after its name; with a mouse the pin and the menu show on hover or focus only; just copied,              |
|                    | 「✓ 已复制」 in the theme's pill replaces the code for 1.6 s; hovered, 「点击复制」 shows beside it (taking no room |
|                    | until then). `card` makes it a card of its own (the phone, 70 px)                                                   |
| `RecentCard`       | 「最近使用」: the account copied last, its code large, the seconds in a ring with the next code under it; copies on |
|                    | click, Enter or Space; HOTP offers its next code. `size="sm"` on the phone                                          |
| `FilterChips`      | the codes page's filters, a radio group of pills with counts (← → Home End); `scroll` on the phone                  |
| `PageHeader`       | a desktop page's 26 px title, its line of facts and its actions                                                     |
| `SidebarStatus`    | the sidebar's foot: a card with the lamp and the vault's state; `SidebarButton(s)` the square buttons under it      |
| `EntryTile`        | the card of 验证码视图 › 卡片: avatar, names, star and menu over a 46 px code, a bar that empties, the next code    |
|                    | in the last 5 s or the HOTP counter; copied, it says so for 1.6 s (`useFlash`). 300 px columns on the desktop,      |
|                    | two to a row on the phone (`size="sm"`, 28 px code). Selecting and reordering stay in the list                      |
| `PasswordField`    | input with show/hide and an optional four-step strength meter                                                       |
| `DropZone`         | dashed `border-strong` area; `accent-soft` fill while a drag hovers                                                 |
| `QrView`           | white plate (the QR's own quiet zone) inside a dialog, page `n / N`, prev/next                                      |
| `StepList`         | numbered steps (mono numerals in `inset` circles) for the import sources                                            |
| `useClock`         | a shared, second-aligned clock in ms for countdowns (`useNow`, also ms, ticks every 30 s for relative times)        |
| `PasswordRow`      | a login: 40 px avatar, title 15 / 600 with its star, `shieldCheck` and badges (验证码, 冲突, 只读), the user name   |
|                    | (12.5 px), the site on the right (hidden in a list under 480 px), and copy user name, copy password and ⋯; 64 px,   |
|                    | selected on `nav-active` with the card shadow; just copied, the theme's pill says so for 1.6 s. `card` on the phone |
| `DetailSection`    | a section of a login's details: its heading, then `FieldRow` (label, value, actions), `KeyValueRow`, `UrlRow`       |
| `TagFilter`        | the tags' menu button: several tags at once, its label the first chosen and `+n`, `×` to clear                      |
| `TagsInput`        | tags as `accent-soft` pills with a `×`, Enter adds the one typed, the tags in use suggested                         |
| `ProtectionChoice` | the three protection levels as radio cards, each with its line of help                                              |
| `Slider`           | the generator's length and word count                                                                               |
| `ScoreMeter`       | a password's strength: four bars and their name, and the generator's bits                                           |
| `SkeletonRows`     | loading rows, shown only once 300 ms have passed                                                                    |
| `TileRow`          | a settings list row with a 32 px icon tile (`inset`), the name (14 / 500) and a status line (a 6 px lamp and 12 px  |
|                    | `fg-muted` words), the actions on the right and an optional note under it; Settings › 浏览器's browsers and paired  |
|                    | extensions                                                                                                          |

`ShortcutRecorder` (Settings › 通用 › 快捷搜索) is a global shortcut's field, drawn as an input (32 px):
the keys as keycaps in the platform's words (⌘ ⇧ on a Mac, Ctrl Shift elsewhere). Focused, it
records the next key pressed with Ctrl, Alt or ⌘; the modifiers show as they are held, and a key
without one says, in `danger`, what it needs. Esc cancels, Tab moves on, an `x` clears it, and
`invalid` draws the edge in `danger`.

Every interactive primitive has `default hover focus-visible active disabled` states, plus
`loading` (buttons: spinner, label kept) and `error` (inputs: `danger` border and message below).
Everything a mouse does is reachable by keyboard; icon-only buttons carry an `aria-label` and a
tooltip.

### A login's dialogs and the generator

Boards 06–12 of Penpot 「Lockra 密码管理」.

- **The form** (新建登录项 / 编辑登录项): a 600 px `Dialog scroll`, its title and buttons fixed and
  the body between them scrolling, at most `min(720px, 100vh − 48px)` high. Groups under a hairline
  with a 13 / 600 title: the title beside a 40 px avatar that previews it, 用户名 and 邮箱 side by
  side, 密码 (`PasswordField` with its strength) with 生成…, 网站 (an `Input` per address, its
  matching in a menu of two sections, 常用 and 高级), 验证码 (the linked account on `inset`
  with its code, or 关联验证码… opening a 420 px picker of the accounts), 备注, 自定义字段 (name,
  文本 or 隐藏, value, page field on `inset`), 整理 (`GroupCombobox`, `TagsInput`, 收藏),
  外观 (`AccountAppearance`), 保护级别 (`ProtectionChoice`) and 自动填充 (three `Segmented sm`,
  跟随设置 · 开 · 关, each saying what 跟随设置 means now). Problems: a `Banner danger bar` counting
  them, each field's reason under it, the focus on the first.
- **显示密码** (520 px): a `Banner warn`, the password mono 26 / 600 on `inset`, its length and kinds
  in 11.5 px, the `ScoreMeter`, the Linux note, and a 14 px ring with the seconds before it hides.
- **生成密码** (520 px): `Segmented` 随机密码 / 口令 (220 px), the result mono 22 / 600 (18 for a
  passphrase) on `inset` with 重新生成 and 复制, the `ScoreMeter` with its bits, the rule's `info`
  line or a `Banner danger bar` with 忽略规则, then labelled rows: 长度 (`Slider`, 4–128),
  the four character groups as toggle chips (on: `accent-soft`, an `accent` ring and a check),
  避免易混字符, 排除字符; for a passphrase 单词数, 分隔符, 首字母大写, 加一个数字. 生成历史 · n 条 at
  the bottom left opens the history after the proof: mono 13 / 500 rows with the time and the site,
  清空历史… in `text-danger`, hidden after 120 s.
- **验证身份** (440 px): the prompt, the system check's button above the master password, the
  action's own button.
- **The command palette**: a login's row carries its actions (`CommandItem.actions`): Enter runs
  the first (复制密码), Tab and Shift Tab move through them, the current one marked; the footer reads
  「↑↓ 选择 · ↵ 执行 · Tab 切换操作 · Esc 关闭」.

### The browser extension

`apps/extension` draws with the same `tokens.css`, following the browser's light or dark scheme
(澄空 or 澄空 · 夜; no theme crosses the protocol). Boards 30–35 of Penpot 「Lockra 密码管理」.

- **Popup**: 360 × 540 in every state. A 52 px header (logo, the lamp and Lockra's state, the
  options), the content (only it scrolls), a 44 px footer (the fill shortcut's keys, a short hint
  that truncates, 「打开 Lockra」). The site's logins in one card, rows of 60 px, the first the
  default (`nav-active`, ↵, a primary 填充). The other states are centred: a 56 px icon on the
  theme's card, a 17 px title, a line, one action, a note.
- **Inline menu and prompts**: drawn in an extension frame on any page, so opaque (`surface`, not
  glass) with a 1 px border and `shadow-pop`: the menu 340 px wide (at least the field's), 6 px under
  the field, `rounded-10`, rows of 52 px; the save and update prompts 360 px at the page's top
  right, `rounded-14`. The shadow is the frame host's, in the page: the frame would cut it off at
  its edge.

### Settings › 浏览器 and the requests that wait for the user

Boards 13–19 of Penpot 「Lockra 密码管理」; the group sits between 同步 and 关于 where the build
offers the browser extension.

- **The pane**: the two switches (浏览器集成, 关闭窗口后继续运行) in `StatusRow`s. Off, a line says
  what turning it on brings and nothing else shows. On: 浏览器 (a `TileRow` per browser used here:
  已连接 `ok`, 清单已写入 `idle`, 未安装扩展 `off`, and 还没有写入连接文件 `warn` with 写入), 已配对的扩展
  (`TileRow`s with `puzzle`, the date paired and last used, 取消配对 in `text-danger`; a missing
  site access adds an `alert` line in `warning-text`), 自动填充 (six `StatusRow`s, the frames rule a
  small `Segmented`), 网站名单 (two cards side by side, `minmax(0, 1fr)` each, 36 px rows) and 最近活动
  (a dense `Table`: time in mono, browser, site in `fg-muted`, the event; a refusal in `danger` with
  `ban`, a pause with `pause` in `warning-text`; 「只保存在内存里，锁定时清空」 and 清空 beside the
  heading). A paused extension puts a `Banner warn bar` with 恢复 at the top of the pane.
- **Dialogs of the pane**: turning the integration on while Lockra does not run in the background
  asks once (440 px: 暂不, 开启后台运行). Filling as pages load first shows its risks (480 px: a
  warning banner, the six conditions, 「从不填验证码、从不自动提交」, 继续…; no checkbox, the owner's
  answer of 2026-10-14), then the proof dialog; so does any change that lets the extensions do more.
  Adding a site (420 px) and unpairing (420 px, a `danger` button) are their own small dialogs.
- **The requests** (460 px, over anything open, the window brought up by the shell): a pairing
  shows the code in mono 40 / 600 on `inset`, a 14 px ring and the seconds left, then the browser
  and the extension's id, 「还有 N 个配对请求在排队」 while others wait; expired, the code turns to
  dashes and only 关闭 is left. A fill or a copy to confirm shows the site, the login and the browser
  on `inset` (the frame's site and the page's for a frame of another site) and the seconds before it
  is denied. Where the core asks for proof the buttons are 拒绝 · 改用主密码 · 使用 Touch ID / Windows
  Hello …, and the master password after the system check fails or was not chosen; a frame of
  another site needs only 仍然填充. Esc denies.

## 4. Content states

Each region handles: empty, loading (first state not yet received), partial (Google batch with
missing codes, import with unsupported rows), error (inline banner with the translated error
code), locked (no codes ever rendered), overflow (issuer or account 200 characters long, unbroken
strings: truncate with ellipsis and a `title`; 200 entries: the list virtualizes nothing but stays
smooth because rows are plain DOM).

## 5. Accessibility

- Text contrast ≥ 4.5:1 in every theme × accent (`theme.test.ts`); `fg-subtle` is only for
  secondary text at 11–12 px where Voltip uses it.
- Focus is always visible (`:focus-visible` outline in `accent-text`).
- Codes are announced as digits (`aria-label` with spaces), the ring as "time left: n seconds".
- `prefers-reduced-motion` is honoured even when the setting is off.

## 6. QA and accepted deviations

- **Widths**: a desktop window with a 960 × 600 minimum, so the 375 / 768 breakpoints of the
  web QA gate do not apply. Screens are checked at **960 × 600, 1280 × 800, 1440 × 900**, in the
  light and dark themes, in the real Tauri window (WebKitGTK under Xvfb); results in
  `docs/acceptance/visual-qa.md`. The newer themes and the card view are checked at 1280 × 800
  against their Penpot boards in `docs/acceptance/themes.md`, with the deviations listed there.
- **Windows** loses Snap Layouts on the maximize button (`decorations: false`, an upstream
  WebView2 limitation; accepted, as in Voltip).
- **Linux** cannot exclude a window from screen capture: secret views say so in place.
- The CJK font adds several megabytes to the bundle (unicode-range chunks load on demand).
