// Vite config for the browser extension. One package, three builds (scripts/build.mjs runs them per
// browser into dist/<target>/):
// - `pages` (the default mode): the popup, the options page and the inline frame (inline.html, the
//   menu and the save prompts drawn in pages), multi-page, React + @lockra/ui for the first two,
//   plain TypeScript for the third, all on tokens.css through Tailwind;
// - `background`: one classic script (a service worker in Chromium, an event page in Firefox);
// - `content`: one classic script, the content script (≤ 30 KB gzip, scripts/check-extension-bundle.sh).
// Classic scripts because a registered content script cannot be a module and the background is the
// same file in both browsers; each is a library build with no chunks.
import { resolve } from "node:path";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { readBuildId } from "./scripts/build-id.mjs";
import { readFrameLook } from "./scripts/frame-look.mjs";

const root = import.meta.dirname;
const target = process.env.LOCKRA_EXTENSION_TARGET ?? "chromium";
const outDir = process.env.LOCKRA_EXTENSION_OUT ?? resolve(root, "dist", target);

const WORKSPACE_MODULE = /[\\/]packages[\\/](ui|shared)[\\/]src[\\/].*\.tsx?$/;

const SCRIPTS = {
  background: resolve(root, "src/background/main.ts"),
  content: resolve(root, "src/content/main.ts"),
} as const;

export default defineConfig(({ mode }) => {
  const define = {
    LOCKRA_TARGET: JSON.stringify(target),
    LOCKRA_FRAME_LOOK: JSON.stringify(readFrameLook()),
    LOCKRA_BUILD: JSON.stringify(readBuildId(root)),
  };
  if (mode === "background" || mode === "content") {
    return {
      define,
      publicDir: false,
      build: {
        outDir,
        emptyOutDir: false,
        target: "es2022",
        // No source maps in the packages: a store reviewer reads the source archive instead.
        sourcemap: false,
        lib: {
          entry: SCRIPTS[mode],
          formats: ["iife"],
          name: `lockra_${mode}`,
          fileName: () => `${mode}.js`,
        },
      },
    };
  }
  return {
    define,
    plugins: [react(), tailwindcss()],
    base: "./",
    publicDir: resolve(root, "public"),
    build: {
      outDir,
      emptyOutDir: true,
      target: "es2022",
      sourcemap: false,
      // The fonts are files of the package, never inlined as data: URLs (the CSP has no data:).
      assetsInlineLimit: 0,
      // Both browsers preload modules themselves.
      modulePreload: { polyfill: false },
      rollupOptions: {
        input: {
          popup: resolve(root, "popup.html"),
          options: resolve(root, "options.html"),
          inline: resolve(root, "inline.html"),
        },
        // The workspace's packages are libraries without import-time effects: what the pages do not
        // use of them (the app's IPC schemas and zod, its whole dictionaries) stays out.
        treeshake: { moduleSideEffects: (id: string) => !WORKSPACE_MODULE.test(id) },
      },
    },
  };
});
