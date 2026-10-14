// @vitest-environment node
// The extension's zips are deterministic: sorted entries, fixed times, the same bytes twice.
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { inflateRawSync } from "node:zlib";
import { crc32, zipFolder } from "./zip.mjs";

/** The entries of a zip from its central directory. */
function entries(zip: Buffer): { name: string; data: Buffer; time: number; date: number }[] {
  const end = zip.lastIndexOf(Buffer.from([0x50, 0x4b, 0x05, 0x06]));
  const count = zip.readUInt16LE(end + 10);
  let at = zip.readUInt32LE(end + 16);
  const out = [];
  for (let i = 0; i < count; i++) {
    const method = zip.readUInt16LE(at + 10);
    const time = zip.readUInt16LE(at + 12);
    const date = zip.readUInt16LE(at + 14);
    const packed = zip.readUInt32LE(at + 20);
    const nameLength = zip.readUInt16LE(at + 28);
    const local = zip.readUInt32LE(at + 42);
    const name = zip.subarray(at + 46, at + 46 + nameLength).toString("utf8");
    const start = local + 30 + zip.readUInt16LE(local + 26);
    const body = zip.subarray(start, start + packed);
    out.push({ name, data: method === 8 ? inflateRawSync(body) : Buffer.from(body), time, date });
    at += 46 + nameLength;
  }
  return out;
}

describe("the zip", () => {
  it("holds every file, sorted, at the DOS epoch, and is the same twice", () => {
    const dir = mkdtempSync(join(tmpdir(), "lockra-zip-"));
    try {
      mkdirSync(join(dir, "assets"));
      writeFileSync(join(dir, "manifest.json"), '{"name":"x"}');
      writeFileSync(join(dir, "assets", "b.js"), "console".repeat(100));
      writeFileSync(join(dir, "a.png"), Buffer.from([1, 2, 3]));
      const first = zipFolder(dir);
      utimesSync(join(dir, "manifest.json"), new Date(2030, 1, 1), new Date(2030, 1, 1));
      expect(zipFolder(dir).equals(first)).toBe(true);
      const read = entries(first);
      expect(read.map((e) => e.name)).toEqual(["a.png", "assets/b.js", "manifest.json"]);
      expect(read.map((e) => [e.time, e.date])).toEqual([
        [0, 0x21],
        [0, 0x21],
        [0, 0x21],
      ]);
      expect(read.find((e) => e.name === "assets/b.js")?.data.toString()).toBe(
        "console".repeat(100),
      );
      expect(read.find((e) => e.name === "a.png")?.data).toEqual(Buffer.from([1, 2, 3]));
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it("checks its data with CRC-32", () => {
    expect(crc32(Buffer.from("123456789"))).toBe(0xcbf43926);
  });
});
