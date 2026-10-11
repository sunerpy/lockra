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
`resolveTheme` in `packages/ui/src/theme.ts`).

### Structure

A theme is more than its roles: each sets the variables below (the classic four share one block that
keeps the look they had). They reach the components through `hairline` (the default 1 px edge, in
the components layer so that an element's own `border-danger` or `hover:border-…` wins),
`app-canvas`, `panel` (card shadow and frost), `sidebar-shell`, `titlebar`, `overlay-frost` and
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
| `--titlebar-fill` / `--titlebar-rule`                                         | surface / border                        | transparent                                                              | canvas / border                                         |
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
- Scale: page title 14 px semibold (title bar); section label 14 px medium; descriptions 12 px
  `fg-muted`; eyebrows 11 px mono uppercase `fg-subtle`; codes 22 px mono semibold
  tabular (32 px in the export check list), split 3-3 (six digits), 4-4 (eight), 3-4 (seven).

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

- Shell = `fixed-sidenav-shell` + `scroll-body-shell`: a 224 px sidebar (56 px icon rail when
  collapsed) on the left; on the right a grid `grid-template-rows: auto minmax(0, 1fr) auto` of
  the 40 px title bar, the page body and the 28 px shortcut footer.
- **Only the page body scrolls.** Its element carries `min-height: 0` (`min-h-0`), or a long list
  pushes the footer off-screen instead of scrolling. Sidebar, title bar and footer never scroll.
- Grid tracks use `minmax(0, 1fr)`, never bare `1fr` (a bare track takes its widest content's
  width and overflows a narrow window).
- The sidebar brand row and the title bar form one 40 px drag strip (`data-tauri-drag-region`).
  The title bar's right slot holds the update note (Voltip's `UpdateBadge`: an accent-soft pill,
  `新版本 0.3.0` / `下载中 36%` / `重启以更新`), which opens the update dialog; it is absent when
  there is nothing to update.
- Overlays (`overlay-stack`): dialogs, the command palette, the QR viewer and toasts stack over
  the shell with a 28 % scrim; Esc closes only the top one.
- Page body: max content width 1040 px, 24 px horizontal padding, 16 px between cards.

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

| Primitive       | What it is                                                                                                         |
| --------------- | ------------------------------------------------------------------------------------------------------------------ |
| `OtpCode`       | the code in mono, grouped; `masked` shows `••• •••`; `warning` tone in the last 5 s                                |
| `CountdownRing` | 20 px ring, accent stroke shrinking over the window, warning ≤ 5 s, static when reduce-motion                      |
| `EntryAvatar`   | rounded square (24–40 px, its own corner) with the issuer's first letter on its colour; never a brand logo         |
| `EntryRow`      | avatar · issuer/account · code · ring · actions; the whole row copies on click/Enter; just copied, 「✓ 已复制」 in |
|                 | the theme's pill replaces the code for 1.6 s; hovered, 「点击复制」 shows beside it (taking no room until then)    |
| `EntryTile`     | the card of 验证码视图 › 卡片: avatar, names, star and menu over a 46 px code, a bar that empties, the next code   |
|                 | in the last 5 s or the HOTP counter; copied, it says so for 1.6 s (`useFlash`). 300 px columns on the desktop,     |
|                 | two to a row on the phone (`size="sm"`, 28 px code). Selecting and reordering stay in the list                     |
| `PasswordField` | input with show/hide and an optional four-step strength meter                                                      |
| `DropZone`      | dashed `border-strong` area; `accent-soft` fill while a drag hovers                                                |
| `QrView`        | white plate (the QR's own quiet zone) inside a dialog, page `n / N`, prev/next                                     |
| `StepList`      | numbered steps (mono numerals in `inset` circles) for the import sources                                           |
| `useClock`      | a shared, second-aligned clock for countdowns (Voltip's `useNow` ticks every 30 s and only drives relative times)  |

Every interactive primitive has `default hover focus-visible active disabled` states, plus
`loading` (buttons: spinner, label kept) and `error` (inputs: `danger` border and message below).
Everything a mouse does is reachable by keyboard; icon-only buttons carry an `aria-label` and a
tooltip.

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
