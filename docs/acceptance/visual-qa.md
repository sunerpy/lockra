# Visual QA

The real app in WebKitGTK under Xvfb (Ubuntu 24.04, webkit2gtk 2.52), checked screenshot by
screenshot against `DESIGN.md` and Voltip's reference screens. Reproduce with `make showcase`
(the primitives) and `make smoke-desktop` (the app); both write into `docs/acceptance/screens/`.

## How the screens are made

- `scripts/screenshot-showcase.sh` opens the development showcase (`#showcase-freeze`: a fixed
  clock, animations off) in `tauri dev` and shoots the upper and lower half in each of the four
  themes, waiting until two consecutive captures agree.
- `scripts/smoke-desktop-linux.sh` builds the release web bundle into a debug binary, starts it
  with throwaway data and config folders and the in-memory keychain of debug builds, and drives it
  with `scripts/smoke/desktop.py`: WebDriver (tauri-driver + WebKitWebDriver) for the DOM, xdotool
  for real keyboard input (typing the master password, Ctrl+L, Ctrl+,), xclip for the X clipboard.
  It checks, in order: create a vault by typing; import an otpauth link from the clipboard; the
  copied code equals RFC 6238 computed independently in Python; lock and unlock; Google's export
  QR code, decoded from the screenshot with rxing (`decode-qr`) and parsed by an independent
  protobuf reader, holds the same account; then every page at 960 × 600, 1280 × 800 and
  1440 × 900 in the light and dark themes, two settings panes, a 200-character issuer and 208
  accounts; finally the app is started on its own and closed with the title bar's button, which
  must end the process with status 0. Every wait is a condition with a deadline.
- Using WebDriver to click and read the DOM, rather than screen coordinates, is a deviation from
  the original plan's tooling (xdotool only); keyboard input still goes through X.

## Results (2026-10-01)

| Check (DESIGN.md)                                                                                                           | Result                                                                                                       |
| --------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------ |
| §1 palettes, accents, type, radii: the four themes and the accent swatches                                                  | pass (showcase, settings › appearance)                                                                       |
| §1 green only for 8 px lamps; warning tone in a code's last five seconds, with the next code                                | pass (`codes-long-1280-light`)                                                                               |
| §1 codes in mono, 3-3 / 4-4 / 3-4 groups; HOTP rows with "next" instead of a ring                                           | pass                                                                                                         |
| §2 fixed sidebar and title bar, 28 px footer, only the page body scrolls                                                    | pass after fix 2 below (`codes-960-*`, `codes-200-1280-light`)                                               |
| §2 tracks `minmax(0, 1fr)`; content width 1040 px, 24 px padding                                                            | pass                                                                                                         |
| §3 every primitive's states, four themes                                                                                    | pass after fix 1 (`showcase-*`)                                                                              |
| §4 empty, partial (Google batch with a missing code), unsupported rows, overflow (truncation with ellipsis), 200 accounts   | pass                                                                                                         |
| §5 focus visible, keyboard paths (palette, arrows in the list, Enter copies, Esc closes the top overlay)                    | pass (unit tests and the smoke run's keyboard steps)                                                         |
| Window chrome: frameless title bar, window buttons, drag strip, close exits with 0                                          | pass                                                                                                         |
| Chinese copy throughout; product names keep their case                                                                      | pass after fix 3                                                                                             |
| The in-app update after Voltip's: the title bar note, Settings › General's switch and status line, the dialog over Settings | pass (`update-badge-light`, `update-settings-light`, `update-dialog-light`, `make smoke-update`, 2026-10-01) |
| Multi-device sync: Settings › Sync off and on, the sync key, joining from the welcome screen, the rollback banner           | pass after four fixes ([sync.md](sync.md); `screens/sync/`, `make smoke-sync`, 2026-10-02)                   |

Problems found by looking, and fixed:

1. **Theme tiles overlapped** where a row ran out of room (captions ran together); tiles are now
   fixed-size and never shrink, and their rows wrap.
2. **A short window lost its footer**: the shell grid's implicit `auto` row grew to the sidebar's
   content height, so at 960 × 600 (and with 200 accounts) the whole document scrolled and the
   shortcut footer and the sidebar's last item fell below the window. The row is now an explicit
   `minmax(0, 1fr)` (regression test in `Shell.test.tsx`).
3. **Import cards**: the PhoneFactor path, one unbroken word, ran out of its card at 1280 px; the
   eyebrow style uppercased product names ("MICROSOFT AUTHENTICATOR"). The cards now use the
   option-card header (icon tile and title in its own case) and break the path anywhere.
4. The showcase rendered English copy (the container has no `zh_CN` locale for the webview); the
   scripts now set the locale in `settings.json`.

## Accepted deviations

- A desktop window with a 960 × 600 minimum: the 375 / 768 px breakpoints of the web QA gate do
  not apply; 960, 1280 and 1440 are checked instead.
- Windows loses Snap Layouts on the maximize button (`decorations: false`, as in Voltip).
- Linux cannot exclude a window from screen capture; the reveal view says so.
- The CJK font adds several megabytes (its unicode-range chunks load on demand).

## Not verified here

macOS (build and run), Windows on real hardware (the installer is cross-built only), the
keychains of the three platforms, Google Authenticator importing Lockra's migration codes,
Microsoft Authenticator scanning Lockra's codes, a PhoneFactor database from a real rooted phone,
and the CI workflow on GitHub's runners. Manual steps for each are in
[docs/release.md](../release.md) (Manual checks on real devices).
