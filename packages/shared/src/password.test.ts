import { describe, expect, it } from "vitest";
import { MIN_PASSWORD, passwordLongEnough } from "./password";

describe("passwordLongEnough", () => {
  it("counts characters as the core does, not UTF-16 units", () => {
    expect(MIN_PASSWORD).toBe(8);
    expect(passwordLongEnough("1234567")).toBe(false);
    expect(passwordLongEnough("12345678")).toBe(true);
    expect(passwordLongEnough("春眠不觉晓处处闻")).toBe(true);
    // Four emoji are eight UTF-16 units but four characters.
    expect(passwordLongEnough("🔒🔑🛡️")).toBe(false);
  });
});
