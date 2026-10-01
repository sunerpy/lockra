import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const here = resolve(import.meta.dirname, "..");

/** The webview never goes online (only the Rust updater does, when asked): every asset ships in the
 *  bundle and the CSP grants no remote host. */
describe("offline assets", () => {
  it("index.html references nothing remote", () => {
    const html = readFileSync(resolve(here, "index.html"), "utf8");
    expect(html).not.toMatch(/https?:\/\//);
  });

  it("the CSP grants only the app itself, inline styles, data: fonts and images, and IPC", () => {
    const conf: { app: { security: { csp: string } } } = JSON.parse(
      readFileSync(resolve(here, "src-tauri/tauri.conf.json"), "utf8"),
    );
    expect(conf.app.security.csp).toBe(
      "default-src 'self'; style-src 'self' 'unsafe-inline'; font-src 'self' data:; img-src 'self' data:; connect-src ipc: http://ipc.localhost",
    );
  });

  it("the tokens bundle the three design families", () => {
    const tokens = readFileSync(resolve(here, "../../packages/ui/src/tokens.css"), "utf8");
    for (const family of ["instrument-sans", "jetbrains-mono", "noto-sans-sc"])
      expect(tokens).toContain(`@import "@fontsource-variable/${family}/index.css";`);
    expect(tokens).not.toMatch(/url\(\s*["']?https?:/);
  });
});
