// The browser extension as Settings › 浏览器 and its dialogs show it: the policy's defaults and
// which changes ask for proof (the core decides; the page asks first, so the proof goes with the
// change), what each browser's row says, and a pairing's name.
import { browserName } from "./labels";
import type { BrowserId, BrowserPolicy, BrowserStatus, PairedView } from "./schema";

/** How long a pairing request waits for the user (lockra-core `PAIR_TTL`). */
export const PAIR_TTL_MS = 120_000;

/** How long a fill or a copy waits to be confirmed (lockra-core `CONFIRM_TTL`). */
export const CONFIRM_TTL_MS = 60_000;

/** The browser extension's defaults (lockra-core `BrowserPolicy::default()`). */
export function defaultBrowserPolicy(): BrowserPolicy {
  return {
    fill_otp: true,
    copy_otp: true,
    auto_submit: false,
    confirm_fill: false,
    cross_origin_frames: "never",
    on_page_load: false,
    match_port: false,
    match_host: false,
  };
}

/** Whether `next` lets the extensions do more than `policy` (lockra-core
 *  `BrowserPolicy::riskier`): that change asks for proof. */
export function riskierPolicy(policy: BrowserPolicy, next: BrowserPolicy): boolean {
  return (
    (next.on_page_load && !policy.on_page_load) ||
    (next.auto_submit && !policy.auto_submit) ||
    (next.fill_otp && !policy.fill_otp) ||
    (policy.confirm_fill && !next.confirm_fill) ||
    (next.cross_origin_frames === "ask" && policy.cross_origin_frames === "never")
  );
}

/** What a browser's row says. Lockra cannot see whether the extension is installed, only whether
 *  an extension of that browser ever paired: `connected` (one has a session now), `paired` (paired
 *  before, not connected now), `no_extension` (never paired), `no_manifest` (the browser came
 *  after the integration was turned on, and has no manifest yet). */
export type BrowserRowState = "connected" | "paired" | "no_extension" | "no_manifest";

export interface BrowserRow {
  browser: BrowserId;
  state: BrowserRowState;
}

/** The rows of Settings › 浏览器: the browsers used on this account or holding Lockra's manifest,
 *  in the shell's order. */
export function browserRows(
  statuses: readonly BrowserStatus[],
  paired: readonly PairedView[],
): BrowserRow[] {
  return statuses
    .filter((status) => status.installed || status.manifest)
    .map(({ browser, manifest }) => {
      const own = paired.filter((p) => p.browser === browser);
      const state: BrowserRowState = !manifest
        ? "no_manifest"
        : own.some((p) => p.connected)
          ? "connected"
          : own.length > 0
            ? "paired"
            : "no_extension";
      return { browser, state };
    });
}

/** The browsers to write a manifest for when the integration is turned on: those used on this
 *  account (writing one for a browser that is not there would make its folder). */
export function installedBrowsers(statuses: readonly BrowserStatus[]): BrowserId[] {
  return statuses.filter((status) => status.installed).map((status) => status.browser);
}

/** A pairing as the user knows it: `Chrome · 办公室电脑`, or the browser alone. */
export function pairedName(browser: string, label: string): string {
  return label === "" ? browserName(browser) : `${browserName(browser)} · ${label}`;
}
