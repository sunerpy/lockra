// The build's identity: a digest of the sources every part of the extension is built from (its own
// and the workspace packages' it bundles) and of the version. The background, the content script and
// the pages are three Vite runs of one build: they compute the same id, and a page that meets a
// background of another id knows the browser still runs an older background (an unpacked
// extension's files changed under it, the pages read the new ones at once).
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";

const ROOTS = ["src", "manifest", "public", "inline.html", "popup.html", "options.html"];
const PACKAGES = ["../../packages/shared/src", "../../packages/ui/src"];

function files(path) {
  const stat = statSync(path, { throwIfNoEntry: false });
  if (!stat) return [];
  if (stat.isFile()) return [path];
  return readdirSync(path)
    .toSorted()
    .flatMap((name) => files(join(path, name)));
}

/** Twelve hex digits of the digest. */
export function readBuildId(root) {
  const version = JSON.parse(readFileSync(resolve(root, "../../package.json"), "utf8")).version;
  const hash = createHash("sha256").update(`lockra ${version}\0`);
  for (const path of [...ROOTS, ...PACKAGES].flatMap((part) => files(resolve(root, part)))) {
    if (/\.test\.[cm]?[jt]sx?$/.test(path)) continue;
    hash.update(relative(root, path).replaceAll("\\", "/")).update("\0");
    hash.update(readFileSync(path)).update("\0");
  }
  return hash.digest("hex").slice(0, 12);
}
