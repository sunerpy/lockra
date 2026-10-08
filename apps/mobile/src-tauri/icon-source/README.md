# The phone app's icon

The Lockra mark (`apps/desktop/src-tauri/icons/icon.svg`, the same as
`packages/ui/src/components/Logo.tsx`) as the Android launcher icon. Regenerate every density
after changing a source:

```bash
cd apps/mobile/src-tauri && cargo tauri icon icon-source/icon.json
```

The command writes `gen/android/app/src/main/res/mipmap-*` (the adaptive icon with its foreground,
background and monochrome layers, and the plain icons) and `icons/`. The phone app uses
`icons/icon.png` only: delete the desktop and iOS icons it writes beside it.

- `lockra.svg`: the whole mark.
- `lockra-background.svg`: the mark's navy, full bleed; the launcher masks the shape.
- `lockra-foreground.svg`, `lockra-monochrome.svg`: the padlock alone, at 0.8, so that it stays
  inside the 66 dp safe circle of the 108 dp canvas (the generator draws an SVG canvas whole, so the
  scale lives in the sources). The monochrome layer cuts the code's three dots out of the lock.
