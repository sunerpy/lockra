# Browser extension acceptance

## What runs on every pull request

`make smoke-browser` (`scripts/smoke-browser-linux.sh`, CI's `browser-smoke`) pairs the development
build of the extension with the real app under Xvfb, in Chrome for Testing and in Firefox, both
pinned and checked by SHA-256. The test sites are `scripts/smoke/site/`, over HTTPS with a
certificate the run makes and over plain HTTP. Site access is granted up front: the request and
its prompt are left to the unit tests and to the matrix below. Each browser goes through:

- pairing: the popup's code is the app's, and it is approved in the app with the master password;
- the inline menu: focusing the field opens it, then ↓ and Enter fill the login;
- the linked code on the next page, in one field and in six one-digit fields;
- the shortcut (a real X key) on a sign-in that spans two pages: it fills the user name on the
  first, the second gets the password by itself, and nothing submits it;
- the policy's limits: an HTTPS login on an HTTP page waits for consent, and a frame of another
  site gets nothing (the app records `cross_origin`);
- pages that try to hijack a click:
  - the page turns transparent under the menu;
  - an element and a later popover cover it, and the menu comes back on top;
  - the page frames the menu's page itself, without the token;
- a page with two thousand fields;
- a sign-up form: the generated password follows `passwordrules` and is saved in Lockra;
- the save prompt and the update prompt change the app's logins;
- locking the app locks the extension.

Chrome leaves the fill command without its suggested key (Ctrl+Shift+L) when the extension is
loaded from the command line: `commands.getAll` reports an empty shortcut, while the action's own
key is assigned. The smoke test sets the key the way `chrome://extensions/shortcuts` does. Whether
Chrome assigns it on a store install is a row of the matrix below.

## Manual matrix before a release

| System  | Package       | Browsers                                                                                   |
| ------- | ------------- | ------------------------------------------------------------------------------------------ |
| Windows | NSIS, MSI     | Chrome, Edge, Firefox                                                                      |
| macOS   | dmg           | Chrome, Firefox                                                                            |
| Linux   | deb, AppImage | Chrome, Firefox                                                                            |
| Ubuntu  | deb           | the Snap Firefox (record whether it connects; Snap and Flatpak browsers are not supported) |

In every row:

1. Settings › 浏览器: turning the integration on writes the browser's manifest (its row says 清单已写入, then 已连接 once paired).
2. Pairing: the codes match, and both the system check (Touch ID, Windows Hello) and the master password approve.
3. The shortcut: check the extension's shortcut is Ctrl+Shift+L (`chrome://extensions/shortcuts`, `edge://extensions/shortcuts`, Firefox's Manage Extension Shortcuts). Then fill with it, and from the menu.
4. The next page's code, the save prompt, and locking Lockra locking the extension.
5. Lockra not running: the popup's 启动 Lockra starts it, in the tray while it runs in the background.
6. The AppImage: the manifest points at the AppImage itself, which mounts again for every connection. Record how long a connection takes.
7. A password the browser keeps for the test site: a click into the field opens the browser's list under it and Lockra's menu above it. In Firefox, after 让 Lockra 接管浏览器的密码功能, Firefox fills and lists nothing and the menu opens under the field; with 避开浏览器自己的密码列表 off, the menu opens under the field in every browser.

## Results

Not run yet: the matrix is run on the release candidate of 0.10.0.
