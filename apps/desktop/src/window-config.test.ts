import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { SIDEBAR_RAIL_WIDTH_TRAFFIC_LIGHTS, TRAFFIC_LIGHTS_CLEARANCE } from "@lockra/ui";

/** Reads the real Tauri config off disk: the macOS override replaces `app.windows` wholesale
 *  (RFC 7396), and the capability file is the webview's whole permission set. vitest runs with
 *  the package as cwd; the repository root is accepted too. */
function locateSrcTauri(): string {
  const found = ["src-tauri", "apps/desktop/src-tauri"]
    .map((dir) => path.resolve(process.cwd(), dir))
    .find((dir) => existsSync(path.join(dir, "tauri.conf.json")));
  if (found === undefined) throw new Error(`src-tauri not found from ${process.cwd()}`);
  return found;
}
const SRC_TAURI = locateSrcTauri();

type Json = Record<string, unknown>;

function readJson(file: string): Json {
  const parsed: unknown = JSON.parse(readFileSync(path.join(SRC_TAURI, file), "utf8"));
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed))
    throw new Error(`${file} is not a JSON object`);
  return parsed as Json;
}

function mainWindow(file: string): Json {
  const windows = (readJson(file).app as { windows?: Json[] } | undefined)?.windows;
  if (windows === undefined || windows.length !== 1)
    throw new Error(`${file} must declare exactly one window`);
  const [win] = windows;
  if (win === undefined || win.label !== "main")
    throw new Error(`${file}: the window must be main`);
  return win;
}

describe("tauri window configuration", () => {
  const base = mainWindow("tauri.conf.json");
  const macos = mainWindow("tauri.macos.conf.json");

  it("draws its own title bar on Windows and Linux, keeps the shadow, and opens hidden", () => {
    expect(base.decorations).toBe(false);
    expect(base.shadow).toBe(true);
    // Shown by Rust once the theme is applied: no white flash.
    expect(base.visible).toBe(false);
    expect(base.minWidth).toBe(960);
    expect(base.minHeight).toBe(600);
  });

  it("keeps the native macOS traffic lights where the sidebar leaves their slot", () => {
    expect(macos.decorations).toBe(true);
    expect(macos.titleBarStyle).toBe("Overlay");
    expect(macos.hiddenTitle).toBe(true);
    expect(macos.trafficLightPosition).toEqual({ x: 12, y: 14 });
    const end = 12 + 2 * 20 + 12;
    expect(TRAFFIC_LIGHTS_CLEARANCE).toBe(end + 16);
    expect(SIDEBAR_RAIL_WIDTH_TRAFFIC_LIGHTS).toBe(end + 12);
  });

  it("regression: the macOS override restates the shared geometry, because it replaces the array", () => {
    for (const key of [
      "label",
      "title",
      "width",
      "height",
      "minWidth",
      "minHeight",
      "visible",
      "dragDropEnabled",
    ])
      expect(macos[key], `${key} drifted between the two configs`).toEqual(base[key]);
  });

  it("lets drops reach Rust (the webview never sees the paths)", () => {
    expect(base.dragDropEnabled).toBe(true);
  });
});

describe("the webview's permissions", () => {
  const capability = readJson("capabilities/default.json") as {
    windows: string[];
    permissions: string[];
  };

  it("are the core defaults and the title bar's window buttons, nothing else", () => {
    expect(capability.windows).toEqual(["main"]);
    expect(new Set(capability.permissions)).toEqual(
      new Set([
        "core:default",
        "core:window:allow-close",
        "core:window:allow-is-maximized",
        "core:window:allow-minimize",
        "core:window:allow-start-dragging",
        "core:window:allow-toggle-maximize",
      ]),
    );
    expect(capability.permissions).toHaveLength(6);
  });

  it("security: grant no file system, dialog, shell or opener access to the webview", () => {
    for (const permission of capability.permissions)
      expect(permission).not.toMatch(/^(fs|dialog|shell|opener|http|clipboard)/);
  });
});
