# Keyboard shortcuts

This page lists the keyboard shortcuts. On macOS, use ⌘ where this page says `Ctrl`.

## Anywhere in the app

| Keys            | Action                                                                                       |
| --------------- | -------------------------------------------------------------------------------------------- |
| `Ctrl K`        | Open the command palette: find an account and press Enter to copy its code, or run a command |
| `Ctrl N`        | Add an account by hand; on the Passwords page, add a login                                   |
| `Ctrl G`        | Generate a password                                                                          |
| `Ctrl L`        | Lock the vault                                                                               |
| `Ctrl ,`        | Open the settings                                                                            |
| `Ctrl F` or `/` | Go to the search field on the codes page, or on the Passwords page while it is open          |
| `Esc`           | Close the dialog or menu on top                                                              |

The shortcuts pause while a dialog is open.

## On the codes page

| Keys                        | Action                                                               |
| --------------------------- | -------------------------------------------------------------------- |
| `↓` in the search field     | Move into the list                                                   |
| `Enter` in the search field | Copy the first matching code                                         |
| `Esc` in the search field   | Clear the search                                                     |
| `↑` `↓`                     | Move between accounts (`↑` on the first returns to the search field) |
| `Home` `End`                | Go to the first or the last account                                  |
| `Enter` or `Space`          | Copy the selected account's code                                     |

## On the Passwords page

<StatusTag status="available" /> Available from version 0.10.0.

| Keys                                | Action                                                                           |
| ----------------------------------- | -------------------------------------------------------------------------------- |
| `↓` in the search field             | Move into the list and show the first login                                      |
| `Enter` in the search field         | Show the first matching login                                                    |
| `Esc` in the search field           | Clear the search                                                                 |
| `↑` `↓`                             | Move between logins and show each (`↑` on the first returns to the search field) |
| `Home` `End`                        | Go to the first or the last login                                                |
| `Enter`                             | Copy the selected login's password                                               |
| `Space`                             | Show the selected login                                                          |
| The context-menu key or `Shift F10` | Open the selected login's menu                                                   |

## In the command palette

| Keys    | Action                                                                                                                      |
| ------- | --------------------------------------------------------------------------------------------------------------------------- |
| `↑` `↓` | Move between the results                                                                                                    |
| `Enter` | Run the result: copy an account's code, or a login's password                                                               |
| `Tab`   | On a login, move to its next action (copy the user name, copy the code, show the password); elsewhere, go to the next group |
| `Esc`   | Close the palette                                                                                                           |

## In the settings

| Keys                            | Action                              |
| ------------------------------- | ----------------------------------- |
| `↑` `↓` in the list of sections | Move between the settings' sections |

## Anywhere on the computer

<StatusTag status="available" /> Available from version 0.10.0.

| Keys               | Action                                                                     |
| ------------------ | -------------------------------------------------------------------------- |
| `Ctrl Shift Space` | Bring up Lockra's command palette over any app, once you set this shortcut |

The quick search shortcut is off until you choose it in **Settings › General**, where
`Ctrl Shift Space` is suggested; on Wayland, bind `lockra-desktop --palette` in your desktop's
keyboard settings instead ([Quick search from anywhere](/passwords/#quick-search-from-anywhere)).

## In the browser

<StatusTag status="available" /> Available from version 0.10.0, with the
[browser extension](/passwords/browser).

| Keys                           | Action                                                                     |
| ------------------------------ | -------------------------------------------------------------------------- |
| `Ctrl Shift L`                 | Fill a login for this site; press again within 10 seconds for the next one |
| `Ctrl Shift Y`                 | Open the Lockra popup                                                      |
| `↑` `↓` in the menu by a field | Move between the site's logins                                             |
| `Enter` in the menu by a field | Fill the chosen login                                                      |
| `Esc` in the menu by a field   | Close the menu                                                             |

The browser's own settings change the first two: `chrome://extensions/shortcuts` in Chrome,
`edge://extensions/shortcuts` in Edge, and **Manage Extension Shortcuts** in Firefox's add-ons
manager.
