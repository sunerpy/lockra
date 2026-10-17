import { compareVersions, olderThan } from "./version";

describe("compareVersions", () => {
  it("orders x.y.z versions part by part, as numbers", () => {
    expect(compareVersions("0.10.1", "0.10.2")).toBeLessThan(0);
    expect(compareVersions("0.10.0", "0.9.9")).toBeGreaterThan(0);
    expect(compareVersions("1.0.0", "1.0.0")).toBe(0);
    expect(compareVersions("0.10", "0.10.0")).toBe(0);
    expect(compareVersions("2", "10")).toBeLessThan(0);
  });

  it("has no order for what is not a version", () => {
    expect(compareVersions("0.10.x", "0.10.1")).toBeNull();
    expect(compareVersions("", "0.10.1")).toBeNull();
    expect(compareVersions("0.10.1-beta", "0.10.1")).toBeNull();
  });
});

describe("olderThan", () => {
  it("is true only for a version known to be older", () => {
    expect(olderThan("0.10.1", "0.10.2")).toBe(true);
    expect(olderThan("0.10.2", "0.10.2")).toBe(false);
    expect(olderThan("0.10.3", "0.10.2")).toBe(false);
    expect(olderThan(null, "0.10.2")).toBe(false);
    expect(olderThan("0.10.1", null)).toBe(false);
    expect(olderThan("?", "0.10.2")).toBe(false);
  });
});
