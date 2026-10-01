# Codes

This page explains the codes page: reading and copying codes, finding an account, and arranging
the list.

<ScreenFigure src="/screens/codes-en-light.webp" dark="/screens/codes-en-dark.webp" width="1440" height="900" alt="The codes page with the account list, the search field and the sort menu." />

## Reading a code

Each row shows the account's initial, its service and account name, the current code and a ring.
The ring empties as the code's period runs out; most codes last 30 seconds. In the last five
seconds the code turns amber and the next code appears under it, so you can wait for it instead of
typing one that is about to change.

Counter-based (HOTP) accounts show a **Next code** button instead of a ring: their code changes
only when you ask for the next one.

## Copying a code

Click a row, or select it with the arrow keys and press Enter. A message confirms the copy and says
when the clipboard will be cleared: after 30 seconds by default, and only if it still holds the
code, so anything you copied afterwards is left alone. The time can be changed, or clearing turned
off, in **Settings › Security**.

On Windows the copied code is kept out of the clipboard history and cloud clipboard; on Linux and
macOS it is marked to stay out of clipboard history where the clipboard manager supports it.

## Finding an account

Type in the search field, or press `/` or `Ctrl F` to go to it. The list keeps the accounts whose
service, account or group contains the text. Enter copies the first match, the down arrow moves into
the list, and Esc clears the search.

`Ctrl K` opens the command palette from anywhere in the app. Type part of a name and press Enter to
copy that account's code.

## Groups and pinned accounts

Give accounts a group, such as "Work", in **Edit…** (the ⋯ menu of a row). When groups exist, a
menu next to the search field shows one group at a time. **Pin to top**, in the same menu, keeps an
account first.

## Order

The sort menu orders the list by name, by the most recently added, or by the most recently used.
Pinned accounts stay on top in every order.

## Hiding codes

**Settings › Security › Hide codes** replaces the digits with dots until the pointer rests on a row
or the keyboard focus reaches it. Useful when you share your screen.

## The row menu

The ⋯ button at the end of a row offers:

- **Pin to top** (or **Unpin**): keep the account at the top.
- **Edit…**: change the service, the account name and the group.
- **Show secret…**: the secret, its otpauth link and QR code, after the master password (see
  [Security](/security/#secrets-on-screen)).
- **Delete…**: remove the account. Turn off two-factor sign-in at the service first, or move it to
  another authenticator, or you may lose access to it.
