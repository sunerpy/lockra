# Themes 澄空 and 墨白, and the card view

The implementation of the two Penpot directions the owner kept as themes (2026-10-11), checked
against their boards. Boards: the self-hosted Penpot file 「Lockra 界面方向」, pages A (澄空 ·
Glass: `01 桌面`, `04 桌面 · 夜`, `06` the cards), B (墨白 · Editorial: `01 桌面`, `02 手机`,
`04 桌面 · 夜`, `06` the list) and `00 · 主题切换` (`01 设置 · 外观`). The owner's decisions:

- A and B are switchable themes that change the appearance only (colours, surfaces, radii,
  shadows); there is one shell. On 2026-10-11, after 0.8.7, that shell became A's layout, in
  every theme and on the phone too (「所有主题统一换成澄空布局」, see "The 澄空 layout" below).
- 「验证码视图」 (list or cards) is a setting of its own, the same in every theme.
- 明亮, 暗黑, 暖纸 and 石墨 stay; 澄空 · 夜 and 墨白 · 夜 are added.
- Following the system switches within a column (明亮 ↔ 暗黑, 澄空 ↔ 澄空 · 夜, 墨白 ↔ 墨白 · 夜);
  暖纸 and 石墨 do not follow it.

`DESIGN.md` ("Palettes", "Structure", `EntryTile`) describes what was built.

## Automated

| Layer  | What                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| core   | `crates/lockra-core/src/settings.rs`: the four theme ids and `codes_view` kept; an unknown theme or view (a newer Lockra's) falls back by itself, the other settings kept                                                                                                                                                                                                                                                                                                                              |
| bridge | `crates/lockra-bridge/tests/contract.rs`: the settings with `glass_night` and `grid` against the fixtures                                                                                                                                                                                                                                                                                                                                                                                              |
| tokens | `theme.test.ts`: eight palettes with every role, every accent in every theme at 4.5:1, the account colours, every structure variable in every theme (the classic four keep their look), 墨白's copied card readable, the columns and the pick rule (`THEME_COLUMNS`, `pickTheme`, `resolveTheme`), the danger label at 4.5:1 in every theme and no text in the surface's translucent fill                                                                                                              |
| web    | `EntryTile.test.tsx` (copy by click and keys, the bar, warning and next code, a late frame, copied, masked, the star, sizes, `tileStep`), `useFlash.test.ts`, `lockra.test.tsx` (a row's copied pill and hover hint), `Codes.test.tsx` "cards" (toolbar switch, copied mark, arrow keys, sections, star, menu, right click, HOTP, select and reorder in the list), `Settings.test.tsx` (the grid, the pair link, the pick rule, the view row), the phone's `accounts.test.tsx` and `settings.test.tsx` |

## Run here

2026-10-11, Ubuntu 24.04 x64 under Xvfb (WebKitGTK), and the phone's development build in Chrome
with a 390 × 844 mobile viewport (the in-memory core of `pnpm dev`; the Android WebView is Chromium
too, but no device or emulator was run). Every board export and every screenshot below was read on
its own, shrunk to 1280 × 800 (desktop) or 390 × 844 (phone), and compared by eye.

- `make smoke-desktop`: the list and the cards in the four newer themes
  (`screens/desktop/codes-1280-{glass,glass_night,paper,paper_night}`, `codes-cards-1280-*`), the
  cards in 明亮 and 暗黑, a card and a row just copied in 墨白 and 澄空
  (`codes-cards-copied-1280-{paper,glass}`, `codes-copied-1280-{paper,glass}`), Settings › 外观 in
  澄空 (`settings-appearance-1280-glass`), and the long names as cards
  (`codes-long-cards-1280-light`). The classic themes' other screenshots are unchanged pixel for
  pixel, apart from the new view switch in the codes toolbar and the codes themselves.
- `make showcase`: every primitive in the eight themes (`screens/showcase/showcase-*`), with the
  card states (normal, warning with the next code, copied, HOTP, the phone's size) and a copied row.
- The phone (`screens/phone/`): the list in 澄空 and 澄空 · 夜 (`codes-390-glass`,
  `codes-390-glass_night`), the cards in 墨白 and 墨白 · 夜 (`codes-cards-390-paper`,
  `codes-cards-390-paper_night`, the latter in a code's last seconds) and in 澄空
  (`codes-cards-390-glass`), and Settings with the theme pairs and the view row
  (`settings-appearance-390-paper`).

| Board                              | Screenshot                                                                      | Result                                                                                                |
| ---------------------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| A `01 桌面` (澄空, the list)       | `codes-1280-glass`, `codes-copied-1280-glass`                                   | pass with deviations 1, 10, 11: aurora, floating frosted sidebar, frosted list, the copied row's pill |
| A `02 手机`                        | `screens/phone/codes-390-glass`                                                 | pass with deviation 1: aurora, frosted list and header, codes and rings                               |
| A `04 桌面 · 夜`                   | `codes-1280-glass_night`, `codes-cards-1280-glass_night`                        | pass with deviations 1, 12: the dark aurora, light edges, light-blue rings and primary                |
| A `05 手机 · 夜`                   | `screens/phone/codes-390-glass_night`                                           | pass with deviation 1                                                                                 |
| A `06` (澄空 as cards)             | `codes-cards-1280-glass`, `codes-cards-copied-1280-glass`                       | pass with deviations 1, 4: three columns at 1280, the accent bar and seconds, the copied pill, HOTP   |
| B `01 桌面` (墨白 as cards)        | `codes-cards-1280-paper`, `codes-cards-copied-1280-paper`                       | pass with deviations 1, 3, 8, 9: ink bars, the card turned to ink when copied, the ink edge on hover  |
| B `02 手机`                        | `screens/phone/codes-cards-390-paper`                                           | pass with deviations 1, 5, 14: two cards to a row, the 28 px code, the HOTP button alone              |
| B `04 桌面 · 夜`                   | `codes-cards-1280-paper_night`                                                  | pass with deviations 1, 3: light ink bars, the copied card in light ink                               |
| B `05 手机 · 夜`                   | `screens/phone/codes-cards-390-paper_night`                                     | pass with deviations 1, 5, 14: the warning state as on the board's AWS card                           |
| B `06` (墨白 as a list)            | `codes-1280-paper`, `codes-copied-1280-paper`                                   | pass with deviations 1, 12, 13: ink rings, the copied row's ink pill                                  |
| `00 · 主题切换` › `01 设置 · 外观` | `settings-appearance-1280-glass`, `screens/phone/settings-appearance-390-paper` | pass with deviations 6, 7: light above dark, the followed column linked, the hint, the view row       |

Problems found by looking, and fixed:

1. **Avatars turned into circles** in 澄空 and 墨白: `rounded-10` follows the theme's card radius
   (18 / 16 px). The avatar now sets its own corner (10 px; 8 px at the phone card's 28 px).
2. **The segmented control's thumb** kept a 5 px corner inside a 12 px frame in 澄空; it now sits
   one pixel inside the theme's control radius.
3. **The phone's HOTP card** cut its counter to 「计数器 …」 beside the button; the phone card shows
   the button alone, as B `02 手机` does.
4. **A hovered 墨白 card** lacked the ink edge of B `01 桌面`; `--tile-hover-edge` gives it (the
   other themes keep their edge).
5. **The danger button's label** took the surface's translucent fill (66 % white in 澄空, 7 % at
   night) and all but vanished in `showcase-glass_night` (found in review). Text in the surface's
   colour now takes the opaque role (`text-surface-solid`); `theme.test.ts` checks the label at
   4.5:1 in every theme and that no component draws text in the fill.
6. **The list rows lacked the boards' copied state and hover hint** (A `01`, A `04`, B `06`): a
   copied row now shows 「✓ 已复制」 in place of its code for a moment, in an accent pill (ink in
   墨白, `--row-pill-*`), and a hovered row shows 「点击复制」. The first version of the hint kept its
   room while hidden and cut the phone's names to 「Git…」; it now takes no room until it shows.

## Deviations (board vs app, for the owner)

偏离（画板 vs 落地，供 owner 复核）：

1. **Layout is not part of a theme** (decision 1; superseded in 0.8.8, see below): A's 「最近使用」 and 保险库状态 cards, the page
   title in the body, the search in the header, the group chips and the sidebar's status card,
   and B's top navigation, title, chips and 扫描二维码 button are not built; both themes use the one
   shell (sidebar, title bar, status card, toolbar). On the phone, the header stays and the rows
   share one card; the boards' title, round buttons, hero card, chips, one card per row and the tab
   bar or dock with its scan button are not built.
2. **Fonts**: the boards use Inter and Noto Sans CJK (the Penpot instance's fonts); the app keeps
   Instrument Sans, Noto Sans SC and JetBrains Mono in every theme.
3. **The seconds beside a card's bar stay `fg-muted`** in the last five seconds (the boards draw
   them amber): `warning` reaches about 3.5:1 at 12 px, under the 4.5:1 rule. The bar and the code
   still turn amber.
4. **The copied note** on a desktop card says the clipboard time in every theme
   (「已复制 · 30 秒后清空剪贴板」, as B and the toast); A `06` drew 「已复制」 alone in its pill. Rows
   and phone cards say 「已复制」.
5. **The phone card keeps ⋯** (B `02 手机` draws none): beside the long press, the visible way to
   an account's actions.
6. **Settings › 外观's tiles** are the existing previews with the name under them, without the
   board's 「新」 badges (a badge would stay after it is no longer new).
7. **What a click does while following the system** is not on the board: a tile of the system's
   own brightness picks its column and keeps following; a tile of the other brightness, or 暖纸 /
   石墨, is applied as it is and turns following off.
8. **The view switch** lists 列表 before 卡片 (the default first); B `01 桌面` drew the grid first.
9. **Card width**: 300 px columns that fill the body (three at 1280 beside the sidebar, about
   310 px each); B `01 桌面` has 373 px cards under its top navigation.
10. **A hovered row** takes the inset tint, as in every theme; A `01` raises it in white with a
    shadow.
11. **Toasts** are the shared frosted toast at the bottom right; A `01` and A `04` draw a dark pill
    in the middle (built in 0.8.8).
12. **A row's star, pencil and ⋯** stay visible on every row, as in every theme; A `04` and B `06`
    show the star and ⋯ on the hovered row only (built in 0.8.8, the pencil gone).
13. **Rows are not divided**: B `06` draws a hairline between rows.
14. **The phone card's seconds** read 「19 秒」; B `02` and B `05` write 「18s」.

Older than this change and fixed with it (the owner's request, 2026-10-11): an input's error border
and the chosen accent swatch's ring showed the hairline colour rather than `danger` / `fg`, because
the `hairline` utility was emitted after Tailwind's colour utilities and its `border` shorthand won.
`hairline` now lives in the components layer, under every utility (`theme.test.ts`; in the built
stylesheet `.hairline` is in `@layer components`, `.border-danger` in `@layer utilities`).

## The logo (2026-10-11)

The owner found the near-black tile of the mark out of place in the new themes and chose two of
five treatments drawn in Penpot (「Lockra 界面方向」 › 「00 · Logo」: E today's tile, A the primary
colour, B the theme's card, C a brand gradient, D no tile): **B inside the app** and **C for the app
icon**.

- Inside the app (sidebar, welcome, unlock): `Logo` sits on the theme's card (`bg-surface`,
  `hairline`, `panel`), the shackle in `fg`, the body `--logo-body` (the theme's own blue, whatever
  the accent), the dots `--logo-dots`; no colour of its own, so `Logo.tsx` left the colour
  check's exceptions.
- The app icon: a white padlock on the blue-violet tile (`#3D7BFF` → `#7C5CFF`, dots `#5B6BFF`):
  `apps/desktop/src-tauri/icons/icon.svg` (every desktop size through `scripts/build-icons.sh`;
  the ICO keeps its 16–256 px layers), the Android sources in `apps/mobile/src-tauri/icon-source/`
  (launcher icons through `tauri icon`), and the site's `docs/site/public/lockra-logo.svg`. Checked
  at 16, 24, 32, 48 and 128 px on light and dark, and as Android's adaptive, square and round
  icons.

## The 澄空 layout in every theme (0.8.8)

The owner, after 0.8.7: 「没有引入澄空样式的布局吗 我看还是老旧的布局设计」. Answers: every theme
takes A's layout (themes keep changing colours and materials only), on the desktop and the phone.
Boards A `01 桌面` (A1), `02 手机` (A2), `04 桌面 · 夜` (A4) and `05 手机 · 夜` (A5); their geometry
came from the builders that drew them. `DESIGN.md` §2 describes what was built.

Automated: `Shell.test.tsx` (ui: the status card, the square buttons and the theme button, the
brand without a lamp, 232 px, the rows lining up), `FilterChips.test.tsx`, `RecentCard.test.tsx`,
`lockra.test.tsx` (the rows' measures, the star after a pinned name, the pin and menu on hover,
the phone's cards), `primitives.test.tsx` (`PageHeader`, the toast pill), `useNow.test.ts`,
`labels.test.ts` (`syncSummary`, the copied notice with the account's name), the desktop's
`Shell.test.tsx` (the page's own title, the bare title bar, 同步 opening Settings › 同步, the status
card) and `Codes.test.tsx` (header, 「最近使用」 and 保险库状态, the devices, the chips, the ⋯ menu),
the phone's `codes.test.tsx` (title, add and lock, the card, the chips, the row cards, the dock).

Run here, 2026-10-11: `make check`; `make smoke-desktop` (every `screens/desktop/` screenshot taken
again: the codes page in the eight themes at 960, 1280 and 1440, the cards, selecting, reordering,
the long names, 200 accounts, the other pages with their headers); `make showcase`; the phone's
development build in Chrome at 390 × 844 (`screens/phone/codes-390-{glass,glass_night,paper,light}`,
`codes-cards-390-{glass,paper,paper_night}`), each read on its own and set beside its board.

| Board              | Screenshot                                                                           | Result                                                                                                          |
| ------------------ | ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| A1 `01 桌面`       | `codes-1280-glass`, `codes-cards-1280-glass`                                         | pass with deviations L1–L8: sidebar, status card and buttons, header, both cards, chips                         |
| A4 `04 桌面 · 夜`  | `codes-1280-glass_night`                                                             | pass with the same deviations                                                                                   |
| A2 `02 手机`       | `screens/phone/codes-390-glass`                                                      | pass with deviations L9–L12: title, round buttons, search, the card, chips, rows, the dock                      |
| A5 `05 手机 · 夜`  | `screens/phone/codes-390-glass_night`                                                | pass with deviations L9–L12                                                                                     |
| (the other themes) | `codes-1280-{light,dark,paper,paper_night}`, `screens/phone/codes-390-{paper,light}` | the same layout in their own colours: the chip that is on is the accent's tint in the classic four, ink in 墨白 |

Problems found by looking, and fixed:

1. **The phone's chips ran under the reorder button** (they bled past the right edge as well as the
   left); they now bleed to the left only and end at the button.
2. **A counter row lost its ring's room while selecting**, and its code stood out of the column;
   it keeps an empty slot of the ring's size.
3. **The long names fell below the fold** of `codes-long-1280-light` once the cards took the top;
   the smoke now copies that account (so 「最近使用」 shows it too) and sorts by use.
4. **The vault card's auto-lock read a minute late** with the 30-second clock; it counts with the
   second clock, as the sidebar does.
5. Older, found on the way: `useNow` returned seconds and every caller read them as milliseconds,
   so 上次备份 and the sync times always said 「刚刚」. It returns milliseconds now
   (`useNow.test.ts`, and `Codes.test.tsx` "shows the time since the last backup" at 3½ hours).

偏离（画板 vs 落地，供 owner 复核）：

- L1. **Sidebar rows are about 40 px** (A1: 44) so that its foot fits a 600 px window; it is 232 px
  wide as on the board, inside the theme's 12 px inset (A1: 16).
- L2. **The search's key is `/`** (A1: ⌘K): `Ctrl K` opens the command palette, `/` and `Ctrl F` the
  search. The palette has no button of its own now; the old footer's key list is gone and the keys
  are in the tooltips.
- L3. **The list's tools have a ⋯ menu** (选择, 按分组显示, 全部折叠, 全部展开) after A1's reorder, view
  and sort; the tools keep the theme's control radius (A1 draws pills).
- L4. **The two cards step aside** while searching, selecting or reordering (not on the board).
- L5. **The theme button steps through the themes** (each click the next; its tooltip names it);
  the full list is Settings › 外观.
- L6. **A hovered row takes the inset tint** (A1: white with a shadow), as in every theme.
- L7. **Rows keep the copied pill and the next code under the code** as in 0.8.7; A1's warning row
  matches. 「最近使用」 always shows the next code under its ring, as A1 does.
- L8. **The window buttons** are the title bar's 46 px buttons (A1 draws three small glyphs); the
  title bar has no fill and no rule in any theme now (the classic four had a surface strip with a
  rule, 墨白 a rule).
- L9. **The phone's header buttons are 添加账号 and 锁定** (A2: 锁定 and 设置): 设置 is in the dock,
  and adding by a photo, a file or a link needs a way in besides the scan button.
- L10. **The phone's rows keep ⋯** (A2 draws none): beside the long press, the visible way to an
  account's actions; long account names are cut sooner (「root@acme-c…」).
- L11. **The reorder button follows the phone's chips** (A2 draws chips only).
- L12. **The dock is on the codes screen only**; the pages it opens have their back button.

## Not checked here

- Windows (WebView2) and macOS (WKWebView): only the Linux build was run; `backdrop-filter` and the
  translucent fills are standard CSS in both, untested by eye.
- An Android device: the phone was checked in Chrome's mobile viewport only.
- `prefers-reduced-transparency` (the opaque fallback for the frost): in the stylesheet, not seen
  on a system that asks for it.
- The frost's cost with hundreds of cards on a slow GPU.
