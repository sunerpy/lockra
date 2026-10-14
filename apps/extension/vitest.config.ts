import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react()],
  define: { LOCKRA_TARGET: JSON.stringify("chromium") },
  test: {
    // The DOM parts (content script, frames, popup, options) run on jsdom; the channel's tests
    // (noise, protocol, background) say `@vitest-environment node` for Node's WebCrypto.
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.{ts,tsx}", "scripts/**/*.test.ts"],
    setupFiles: ["src/test/setup.ts"],
    restoreMocks: true,
    testTimeout: 20_000,
    coverage: {
      provider: "v8",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.test.{ts,tsx}",
        "src/**/*.d.ts",
        "src/test/**",
        // The entry points only wire the real browser APIs to the modules the tests drive.
        "src/background/main.ts",
        "src/content/main.ts",
        "src/inline/main.ts",
        "src/popup/main.tsx",
        "src/options/main.tsx",
      ],
      thresholds: { lines: 85 },
      reporter: ["text", "text-summary", "lcovonly"],
    },
  },
});
