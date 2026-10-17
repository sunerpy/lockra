import { describe, expect, it } from "vitest";
import {
  browserRows,
  defaultBrowserPolicy,
  installedBrowsers,
  pairedName,
  riskierPolicy,
} from "./browser";
import type { BrowserStatus, PairedView } from "./schema";

const statuses: BrowserStatus[] = [
  { browser: "chrome", installed: true, manifest: true },
  { browser: "edge", installed: true, manifest: true },
  { browser: "firefox", installed: true, manifest: true },
  { browser: "chromium", installed: true, manifest: false },
  { browser: "brave", installed: false, manifest: false },
];

function paired(browser: string, connected: boolean): PairedView {
  return {
    id: `p-${browser}`,
    browser,
    label: "",
    extension: "e",
    paired_at_ms: 0,
    last_seen_ms: null,
    connected,
    page_access: true,
    version: null,
    paused: false,
  };
}

describe("browser rows", () => {
  it("say what Lockra can tell of each browser", () => {
    expect(browserRows(statuses, [paired("chrome", true), paired("edge", false)])).toEqual([
      { browser: "chrome", state: "connected" },
      { browser: "edge", state: "paired" },
      { browser: "firefox", state: "no_extension" },
      { browser: "chromium", state: "no_manifest" },
    ]);
  });

  it("keep a browser that is gone while its manifest is there", () => {
    expect(browserRows([{ browser: "brave", installed: false, manifest: true }], [])).toEqual([
      { browser: "brave", state: "no_extension" },
    ]);
  });

  it("turning the integration on writes the browsers used here", () => {
    expect(installedBrowsers(statuses)).toEqual(["chrome", "edge", "firefox", "chromium"]);
  });
});

describe("the policy", () => {
  const base = defaultBrowserPolicy();

  it("asks for proof for what lets the extensions do more", () => {
    expect(riskierPolicy(base, { ...base, on_page_load: true })).toBe(true);
    expect(riskierPolicy(base, { ...base, auto_submit: true })).toBe(true);
    expect(riskierPolicy({ ...base, fill_otp: false }, base)).toBe(true);
    expect(riskierPolicy({ ...base, confirm_fill: true }, base)).toBe(true);
    expect(riskierPolicy(base, { ...base, cross_origin_frames: "ask" })).toBe(true);
  });

  it("not for what lets them do less, nor for copying the code or matching the port", () => {
    expect(riskierPolicy(base, { ...base, fill_otp: false })).toBe(false);
    expect(riskierPolicy(base, { ...base, match_port: true })).toBe(false);
    expect(riskierPolicy({ ...base, match_port: true }, base)).toBe(false);
    expect(riskierPolicy(base, { ...base, confirm_fill: true })).toBe(false);
    expect(riskierPolicy({ ...base, copy_otp: false }, base)).toBe(false);
    expect(riskierPolicy({ ...base, on_page_load: true }, base)).toBe(false);
  });
});

it("a pairing is named after its browser and its label", () => {
  expect(pairedName("chrome", "办公室电脑")).toBe("Chrome · 办公室电脑");
  expect(pairedName("edge", "")).toBe("Edge");
});
