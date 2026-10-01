# Security policy

Lockra keeps two-factor secrets, so security reports get priority.

## Reporting a vulnerability

Use GitHub's private vulnerability reporting:
<https://github.com/sunerpy/lockra/security/advisories/new>. Please do not open a public issue,
and never include a real secret, otpauth link, QR code or vault file in a report; describe how to
reproduce the problem with a test account instead.

You can expect an acknowledgement within a week. Once a fix is released, the advisory is published
with credit unless you ask otherwise.

## Supported versions

Only the latest release receives fixes. Lockra is in `0.x`: update to the newest version before
reporting.

## Scope

In scope: the vault and backup formats, key handling, the keychain integration, what the
interface can reach through the shell (files, the clipboard, IPC), import and export parsing, and
the release pipeline. [docs/security.md](docs/security.md) describes the model and its accepted
residual risks (for example, Linux cannot exclude a window from screen capture); a report that a
documented residual risk exists is not a vulnerability, but a way to make one worse is.
