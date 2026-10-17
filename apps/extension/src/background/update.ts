// Updating an extension loaded unpacked (docs/architecture.md, "The extension's folder"): Chrome,
// Edge and Brave never update one. Lockra keeps its own copy of the extension in a folder of its
// own and writes `lockra-folder.json` into it last, so an extension loaded from there finds the
// file in its own package. When the folder holds another version than the one that runs, the
// extension reloads itself once nothing is open (no popup, options page or inline frame, no
// request or pairing under way): the browser reads the folder again and keeps the extension's
// storage, its pairing with it. One try per version: when the reload changed nothing, it stops
// and the popup says so. Without the file (a folder the user unzipped, Firefox) the popup tells an
// extension older than Lockra.
import { olderThan } from "@lockra/shared/version";
import type { BrowserApi } from "../browser/api";
import type { UpdateNote } from "../shared/messages";

/** The file Lockra writes last into its copy of the extension (lockra-desktop `MARKER`). */
export const FOLDER_MARKER = "lockra-folder.json";
/** The folder's version a reload was last tried for (storage.local). */
export const RELOAD_TRIED = "update.reload_tried";

/** A fetch of the extension's own files. */
export type FetchFile = (
  url: string,
  init: { cache: "no-store" },
) => Promise<{ ok: boolean; json(): Promise<unknown> }>;

export interface SelfUpdateDeps {
  api: Pick<BrowserApi, "runtime" | "storage">;
  /** The version that runs: the manifest as the browser loaded it. */
  loaded: string;
  fetch: FetchFile;
}

export class SelfUpdate {
  /** The version in Lockra's folder: null without the file, undefined until read. */
  private folder: string | null | undefined;
  private tried: string | null = null;

  constructor(private readonly deps: SelfUpdateDeps) {}

  /** Read the file again: Lockra may have put a new copy there meanwhile. */
  async refresh(): Promise<void> {
    const [folder, stored] = await Promise.all([
      this.readMarker(),
      this.deps.api.storage.local.get(RELOAD_TRIED).catch(() => ({})),
    ]);
    const tried = (stored as Record<string, unknown>)[RELOAD_TRIED];
    this.folder = folder;
    this.tried = typeof tried === "string" ? tried : null;
  }

  /** Another version waits in the folder and has not been tried. */
  get waiting(): boolean {
    return (
      typeof this.folder === "string" &&
      this.folder !== this.deps.loaded &&
      this.folder !== this.tried
    );
  }

  /** Reload when a version waits and nothing is open (`busy`): whether it does. */
  async reloadIfIdle(busy: boolean): Promise<boolean> {
    if (busy || !this.waiting || typeof this.folder !== "string") return false;
    this.tried = this.folder;
    await this.deps.api.storage.local.set({ [RELOAD_TRIED]: this.folder });
    this.deps.api.runtime.reload();
    return true;
  }

  /** What the popup says, given Lockra's version: a new copy waits for the popup to close
   *  (`waiting`), the folder's copy is behind Lockra (`folder`: Lockra could not write it yet, or
   *  the reload changed nothing), or the extension came another way and is older (`manual`). */
  note(app: string | null): UpdateNote | null {
    if (this.folder === undefined) return null;
    if (this.waiting) return "waiting";
    if (!olderThan(this.deps.loaded, app)) return null;
    return this.folder === null ? "manual" : "folder";
  }

  private async readMarker(): Promise<string | null> {
    try {
      const url = this.deps.api.runtime.getURL(FOLDER_MARKER);
      const response = await this.deps.fetch(url, { cache: "no-store" });
      if (!response.ok) return null;
      const marker: unknown = await response.json();
      const version =
        typeof marker === "object" && marker !== null && "version" in marker
          ? marker.version
          : null;
      return typeof version === "string" && version !== "" ? version : null;
    } catch {
      // Not there (the browser answers a missing file of the package with a network error), or
      // not Lockra's.
      return null;
    }
  }
}
