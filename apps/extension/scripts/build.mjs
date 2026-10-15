// Build the extension for each browser into dist/<target>/, and with --zip the release's packages:
//   dist/chromium           Chrome and Edge as the stores get it (they assign the ids; no `key`,
//                           which Edge Add-ons refuses)
//   dist/chromium-unpacked  the same with Edge Add-ons' public key (manifest/unpacked.json): the
//                           self-install package, Edge Add-ons' id wherever it is loaded unpacked
//   dist/chromium-dev       the same with the development key: the id debug builds of Lockra allow
//   dist/firefox            Firefox (gecko id lockra@firlab.app)
//   dist/lockra-extension_<version>_{chromium-store,chromium-unpacked,firefox}.zip   (--zip;
//                           deterministic; the store zip is the Chrome Web Store's and Edge
//                           Add-ons' package alike)
// The version is the root package.json's (release-please's), written into every manifest. A release
// ships exactly these three zips (scripts/package-extension.sh checks it).
// Usage: node scripts/build.mjs [--zip]
import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build } from "vite";
import { writeZip } from "./zip.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const dist = join(root, "dist");
const zip = process.argv.includes("--zip");

const read = (path) => JSON.parse(readFileSync(path, "utf8"));
const version = read(join(root, "..", "..", "package.json")).version;
// Chrome takes one to four dot-separated integers; release-please writes x.y.z.
if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`build: the version ${version} is not x.y.z`);

const base = read(join(root, "manifest", "base.json"));

function manifest(...parts) {
  return JSON.stringify({ ...base, ...Object.assign({}, ...parts), version }, null, 2) + "\n";
}

async function bundle(target) {
  const out = join(dist, target);
  process.env.LOCKRA_EXTENSION_TARGET = target;
  process.env.LOCKRA_EXTENSION_OUT = out;
  const configFile = join(root, "vite.config.ts");
  // The pages first (they empty the folder), then the two classic scripts beside them.
  for (const mode of ["pages", "background", "content"]) {
    // One after the other: each reads the target from the environment set above.
    // oxlint-disable-next-line no-await-in-loop
    await build({ configFile, mode, logLevel: "warn", root });
  }
  writeFileSync(
    join(out, "manifest.json"),
    manifest(read(join(root, "manifest", `${target}.json`))),
  );
  return out;
}

rmSync(dist, { recursive: true, force: true });
mkdirSync(dist, { recursive: true });
const chromium = await bundle("chromium");
const firefox = await bundle("firefox");

// The Chromium build again under a key of its own: `unpacked` (Edge Add-ons' public key) and `dev`.
function keyed(target, part) {
  const out = join(dist, target);
  cpSync(chromium, out, { recursive: true });
  writeFileSync(
    join(out, "manifest.json"),
    manifest(read(join(root, "manifest", "chromium.json")), read(join(root, "manifest", part))),
  );
  return out;
}
const unpacked = keyed("chromium-unpacked", "unpacked.json");
keyed("chromium-dev", "dev.json");

if (zip) {
  const name = (kind) => join(dist, `lockra-extension_${version}_${kind}.zip`);
  // Edge Add-ons takes the Chrome Web Store's package as it is: one file for both.
  writeZip(chromium, name("chromium-store"));
  writeZip(unpacked, name("chromium-unpacked"));
  writeZip(firefox, name("firefox"));
}

for (const target of ["chromium", "chromium-unpacked", "chromium-dev", "firefox"]) {
  if (!existsSync(join(dist, target, "manifest.json")))
    throw new Error(`build: ${target} has no manifest`);
}
process.stdout.write(
  `build: Lockra extension ${version} in ${dist}${zip ? " (with the zips)" : ""}\n`,
);
