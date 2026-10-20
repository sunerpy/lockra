// @vitest-environment node
// One build's three Vite runs share one id; a change of a source or of the version gives another.
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { readBuildId } from "./build-id.mjs";

describe("readBuildId", () => {
  let top: string;
  let root: string;

  beforeEach(() => {
    top = mkdtempSync(join(tmpdir(), "lockra-build-id-"));
    root = join(top, "apps", "extension");
    for (const dir of [
      "src/content",
      "manifest",
      "../../packages/shared/src",
      "../../packages/ui/src",
    ])
      mkdirSync(join(root, dir), { recursive: true });
    writeFileSync(join(top, "package.json"), JSON.stringify({ version: "1.2.3" }));
    writeFileSync(join(root, "src/content/agent.ts"), "export const a = 1;\n");
    writeFileSync(join(root, "src/content/agent.test.ts"), "test\n");
    writeFileSync(join(root, "popup.html"), "<html></html>\n");
    writeFileSync(join(top, "packages/shared/src/messages.ts"), "export {};\n");
  });

  afterEach(() => rmSync(top, { recursive: true, force: true }));

  it("is the same for the same sources and changes with any of them, not with a test", () => {
    const id = readBuildId(root);
    expect(id).toMatch(/^[0-9a-f]{12}$/);
    expect(readBuildId(root)).toBe(id);
    writeFileSync(join(root, "src/content/agent.test.ts"), "another test\n");
    expect(readBuildId(root)).toBe(id);
    writeFileSync(join(top, "packages/shared/src/messages.ts"), "export const b = 2;\n");
    const changed = readBuildId(root);
    expect(changed).not.toBe(id);
    writeFileSync(join(top, "package.json"), JSON.stringify({ version: "1.2.4" }));
    expect(readBuildId(root)).not.toBe(changed);
  });
});
