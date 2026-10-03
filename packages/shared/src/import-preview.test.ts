import { describe, expect, it } from "vitest";
import { actionsFor, importChoices, takenCount } from "./import-preview";
import type { CandidateView, ImportView } from "./schema";

function candidate(id: number, status: CandidateView["status"]): CandidateView {
  return {
    id,
    source: { type: "text" },
    origin: "uri",
    issuer: `Service ${id}`,
    account: "",
    kind: { type: "totp", period: 30 },
    algorithm: "sha1",
    digits: 6,
    line: id + 1,
    status,
    default_action: status.type === "new" ? "add" : "skip",
  };
}

describe("actionsFor", () => {
  it("lets new accounts be added or skipped, name clashes also replace, the rest only skip", () => {
    expect(actionsFor({ type: "new" })).toEqual(["add", "skip"]);
    expect(actionsFor({ type: "conflict", entry_id: "x" })).toEqual(["add", "replace", "skip"]);
    expect(actionsFor({ type: "exists", entry_id: "x" })).toEqual([]);
    expect(actionsFor({ type: "unsupported", reason: "md5_algorithm" })).toEqual([]);
  });
});

describe("importChoices / takenCount", () => {
  const view: ImportView = {
    candidates: [
      candidate(0, { type: "new" }),
      candidate(1, { type: "conflict", entry_id: "x" }),
      candidate(2, { type: "duplicate" }),
      candidate(3, { type: "new" }),
    ],
    google_batches: [],
    awaiting_password: null,
  };

  it("sends a choice for every account the user decides on, the default where they did not", () => {
    expect(importChoices(view, new Map())).toEqual([
      { id: 0, action: "add" },
      { id: 1, action: "skip" },
      { id: 3, action: "add" },
    ]);
    const chosen = new Map([
      [1, "replace" as const],
      [3, "skip" as const],
    ]);
    expect(importChoices(view, chosen)).toEqual([
      { id: 0, action: "add" },
      { id: 1, action: "replace" },
      { id: 3, action: "skip" },
    ]);
  });

  it("counts what is added or replaces an account", () => {
    expect(takenCount(importChoices(view, new Map()))).toBe(2);
    expect(takenCount(importChoices(view, new Map([[0, "skip" as const]])))).toBe(1);
    expect(takenCount([])).toBe(0);
  });
});
