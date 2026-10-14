// What the hello says of this extension (shown in Lockra; no decision rests on it).
import { type BrowserApi, TARGET } from "../browser/api";
import type { ClientInfo } from "../protocol/schemas";
import { pageAccess } from "./access";

/** The brands of `navigator.userAgentData` (Chromium), when it has them. */
export function brandsOf(nav: object): Set<string> {
  if (!("userAgentData" in nav)) return new Set();
  const data = nav.userAgentData;
  if (
    typeof data !== "object" ||
    data === null ||
    !("brands" in data) ||
    !Array.isArray(data.brands)
  )
    return new Set();
  const names: unknown[] = data.brands.map((b: unknown) =>
    typeof b === "object" && b !== null && "brand" in b ? b.brand : null,
  );
  return new Set(names.filter((name): name is string => typeof name === "string"));
}

/** The browser, from the user agent's brands (Chromium) or the build (Firefox). */
export function browserName(target: "chromium" | "firefox", brands: ReadonlySet<string>): string {
  if (target === "firefox") return "firefox";
  if (brands.has("Microsoft Edge")) return "edge";
  if (brands.has("Brave")) return "brave";
  if (brands.has("Google Chrome")) return "chrome";
  return "chromium";
}

export async function clientInfo(
  api: BrowserApi,
  brands: ReadonlySet<string>,
): Promise<ClientInfo> {
  return {
    browser: browserName(TARGET, brands),
    label: "",
    extension: api.runtime.id,
    version: api.runtime.getManifest().version,
    page_access: await pageAccess(api),
  };
}
