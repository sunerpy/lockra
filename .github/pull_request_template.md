<!--
The title is the squash commit's subject and release-please reads it: a Conventional Commit,
`type(scope): subject`, for example `fix(import): 跳过 PhoneFactor 中加密的账号`.
Types: feat fix docs chore refactor perf test build ci style revert.
-->

## What changes

## How it was verified

- [ ] `make check` passes locally
- [ ] Behaviour changes come with a test that failed before the change
- [ ] User-visible changes: the pages under `docs/site/` are updated in both languages
- [ ] Interface changes: screenshots in light and dark (or `make smoke-desktop`)
- [ ] IPC changes: fixtures regenerated (`UPDATE_IPC_FIXTURES=1 cargo test -p lockra-bridge --test contract`)
