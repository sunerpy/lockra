// Vite config for the Lockra mobile webview. Tailwind 4 through @tailwindcss/vite; port and outDir
// are the ones src-tauri/tauri.conf.json expects (the desktop's dev server keeps 1420).
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1422,
    strictPort: true,
    // `tauri android dev` reaches the dev server from the phone.
    host: process.env.TAURI_DEV_HOST || false,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    // Android System WebView follows Chrome; minSdk 26 phones update it from the store.
    target: "es2022",
  },
});
