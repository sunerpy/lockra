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
/** The build every copy of the extension carries (scripts/build.mjs): a copy of the same version
 *  built from other sources is another one too. */
export const BUILD_FILE = "lockra-build.json";
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
  /** The build that runs (LOCKRA_BUILD); without it, versions alone are compared. */
  build?: string;
  fetch: FetchFile;
}

export class SelfUpdate {
  /** The version in Lockra's folder: null without the file, undefined until read. */
  private folder: string | null | undefined;
  /** The build of the folder's copy: null when it says none (a copy older than this check). */
  private folderBuild: string | null = null;
  private tried: string | null = null;

  constructor(private readonly deps: SelfUpdateDeps) {}

  /** Read the file again: Lockra may have put a new copy there meanwhile. */
  async refresh(): Promise<void> {
    const [folder, build, stored] = await Promise.all([
      this.readMarker(),
      this.deps.build === undefined ? null : this.readBuild(),
      this.deps.api.storage.local.get(RELOAD_TRIED).catch(() => ({})),
    ]);
    const tried = (stored as Record<string, unknown>)[RELOAD_TRIED];
    this.folder = folder;
    this.folderBuild = folder === null ? null : build;
    this.tried = typeof tried === "string" ? tried : null;
  }

  /** The copy a reload is tried for, once: its version, and its build when it says one. */
  private get copy(): string | null {
    if (typeof this.folder !== "string") return null;
    return this.folderBuild === null ? this.folder : `${this.folder}+${this.folderBuild}`;
  }

  /** Another version, or another build of this one, waits in the folder and has not been tried. */
  get waiting(): boolean {
    const copy = this.copy;
    if (copy === null) return false;
    const other =
      this.folder !== this.deps.loaded ||
      (this.folderBuild !== null && this.folderBuild !== this.deps.build);
    return other && copy !== this.tried;
  }

  /** Reload when a copy waits and nothing is open (`busy`): whether it does. */
  async reloadIfIdle(busy: boolean): Promise<boolean> {
    const copy = this.copy;
    if (busy || !this.waiting || copy === null) return false;
    this.tried = copy;
    await this.deps.api.storage.local.set({ [RELOAD_TRIED]: copy });
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

  private async readBuild(): Promise<string | null> {
    try {
      const response = await this.deps.fetch(this.deps.api.runtime.getURL(BUILD_FILE), {
        cache: "no-store",
      });
      if (!response.ok) return null;
      const file: unknown = await response.json();
      const build =
        typeof file === "object" && file !== null && "build" in file ? file.build : null;
      return typeof build === "string" && build !== "" ? build : null;
    } catch {
      return null;
    }
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
