/// <reference types="vite/client" />
import type { FrameLook } from "../scripts/frame-look.mjs";
import type { BrowserApi } from "./browser/api";

declare global {
  /** The browser family a build is for: `chromium` (Chrome, Edge) or `firefox` (vite.config.ts). */
  const LOCKRA_TARGET: "chromium" | "firefox";
  /** The inline frame's shadow and corners per scheme, from tokens.css (scripts/frame-look.mjs). */
  const LOCKRA_FRAME_LOOK: FrameLook;
  /** Firefox's WebExtension namespace. */
  var browser: BrowserApi | undefined;
  /** Chromium's (Firefox has it too, with callbacks in MV2 and promises in MV3). */
  var chrome: BrowserApi | undefined;
}
