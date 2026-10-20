import { FakeBrowser } from "../test/fake-browser";
import { BUILD_FILE, FOLDER_MARKER, RELOAD_TRIED, SelfUpdate } from "./update";

/** A fetch of the extension's own files: Lockra's marker with `marker`, or no such file. */
function files(marker: unknown) {
  const urls: string[] = [];
  const fetch = async (url: string, init: { cache?: string }) => {
    urls.push(`${url} ${init.cache ?? ""}`);
    if (marker === undefined) throw new TypeError("Failed to fetch");
    return {
      ok: true,
      json: async () => (typeof marker === "string" ? JSON.parse(marker) : marker),
    };
  };
  return { fetch, urls };
}

function setUp(marker: unknown, loaded = "0.10.1") {
  const api = new FakeBrowser();
  let reloads = 0;
  api.runtime.reload = () => {
    reloads += 1;
  };
  const { fetch, urls } = files(marker);
  const update = new SelfUpdate({ api, loaded, fetch });
  return { api, update, urls, reloads: () => reloads };
}

/** Lockra's marker and the copy's build file, each by its name. */
function folderFiles(marker: unknown, build: unknown) {
  const fetch = async (url: string) => {
    const body = url.endsWith(`/${BUILD_FILE}`) ? build : marker;
    if (body === undefined) throw new TypeError("Failed to fetch");
    return { ok: true, json: async () => body };
  };
  return fetch;
}

describe("SelfUpdate", () => {
  it("reloads for another build of the version that runs, once per build", async () => {
    // 2026-10-20: copies of one version built from other sources (a build to test) left the
    // older background running, its pages new.
    const api = new FakeBrowser();
    let reloads = 0;
    api.runtime.reload = () => {
      reloads += 1;
    };
    const start = (build: unknown, running = "abc123") =>
      new SelfUpdate({
        api,
        loaded: "0.10.1",
        build: running,
        fetch: folderFiles({ version: "0.10.1" }, build),
      });
    const update = start({ build: "def456" });
    await update.refresh();
    expect(update.note("0.10.1")).toBe("waiting");
    expect(await update.reloadIfIdle(false)).toBe(true);
    expect(reloads).toBe(1);
    expect(await api.storage.local.get(RELOAD_TRIED)).toEqual({ [RELOAD_TRIED]: "0.10.1+def456" });
    // Started again on the same build: nothing more; the build that runs: nothing to do.
    const again = start({ build: "def456" });
    await again.refresh();
    expect(await again.reloadIfIdle(false)).toBe(false);
    const same = start({ build: "def456" }, "def456");
    await same.refresh();
    expect(same.note("0.10.1")).toBeNull();
    expect(await same.reloadIfIdle(false)).toBe(false);
    // A copy without the file (older than this check) is compared by its version alone.
    const older = start(undefined);
    await older.refresh();
    expect(await older.reloadIfIdle(false)).toBe(false);
    expect(reloads).toBe(1);
  });

  it("reloads once nothing is open when Lockra's folder holds another version", async () => {
    const { api, update, urls, reloads } = setUp({ version: "0.10.2", digest: "sha256:x" });
    await update.refresh();
    expect(urls).toEqual([
      `chrome-extension://testextensionidtestextensionidte/${FOLDER_MARKER} no-store`,
    ]);
    expect(update.note("0.10.2")).toBe("waiting");
    expect(await update.reloadIfIdle(true)).toBe(false);
    expect(reloads()).toBe(0);
    expect(await update.reloadIfIdle(false)).toBe(true);
    expect(reloads()).toBe(1);
    expect(await api.storage.local.get(RELOAD_TRIED)).toEqual({ [RELOAD_TRIED]: "0.10.2" });
  });

  it("tries once per version: a reload that changed nothing is not tried again", async () => {
    const first = setUp({ version: "0.10.2" });
    await first.update.refresh();
    await first.update.reloadIfIdle(false);
    expect(await first.update.reloadIfIdle(false)).toBe(false);
    // The extension that started again runs the same version: the folder's copy did not take.
    const again = new SelfUpdate({
      api: first.api,
      loaded: "0.10.1",
      fetch: files({ version: "0.10.2" }).fetch,
    });
    await again.refresh();
    expect(await again.reloadIfIdle(false)).toBe(false);
    expect(first.reloads()).toBe(1);
    expect(again.note("0.10.2")).toBe("folder");
    // A newer copy later is tried.
    const later = new SelfUpdate({
      api: first.api,
      loaded: "0.10.1",
      fetch: files({ version: "0.10.3" }).fetch,
    });
    await later.refresh();
    expect(later.note("0.10.3")).toBe("waiting");
  });

  it("a folder that holds the version that runs changes nothing; one behind Lockra says so", async () => {
    const { update, reloads } = setUp({ version: "0.10.1" });
    await update.refresh();
    expect(await update.reloadIfIdle(false)).toBe(false);
    expect(reloads()).toBe(0);
    expect(update.note("0.10.1")).toBeNull();
    expect(update.note("0.10.2")).toBe("folder");
    // An older Lockra (or none known) asks nothing.
    expect(update.note("0.10.0")).toBeNull();
    expect(update.note(null)).toBeNull();
  });

  it("an extension not loaded from Lockra's folder tells it is older than Lockra, and never reloads", async () => {
    for (const marker of [
      undefined,
      "not json",
      { digest: "sha256:x" },
      { version: 2 },
      { version: "" },
      null,
    ]) {
      const { update, reloads } = setUp(marker);
      await update.refresh();
      expect(await update.reloadIfIdle(false)).toBe(false);
      expect(reloads()).toBe(0);
      expect(update.note("0.10.2")).toBe("manual");
      expect(update.note("0.10.1")).toBeNull();
    }
  });

  it("says nothing before the folder was read", () => {
    const { update } = setUp({ version: "0.10.2" });
    expect(update.note("0.10.2")).toBeNull();
  });

  it("reads the file again when asked: Lockra put a new copy there meanwhile", async () => {
    const api = new FakeBrowser();
    api.runtime.reload = () => undefined;
    let version = "0.10.1";
    const update = new SelfUpdate({
      api,
      loaded: "0.10.1",
      fetch: async () => ({ ok: true, json: async () => ({ version }) }),
    });
    await update.refresh();
    expect(update.note("0.10.2")).toBe("folder");
    version = "0.10.2";
    await update.refresh();
    expect(update.note("0.10.2")).toBe("waiting");
  });

  it("a missing file answered as not found counts as none", async () => {
    const api = new FakeBrowser();
    const update = new SelfUpdate({
      api,
      loaded: "0.10.1",
      fetch: async () => ({ ok: false, json: async () => ({ version: "0.10.2" }) }),
    });
    await update.refresh();
    expect(update.note("0.10.2")).toBe("manual");
  });
});
