import {
  emptyStorageForm,
  isSealedInvite,
  storageComplete,
  storageConfig,
  storageFormFrom,
  storageSummary,
} from "./storage-form";

describe("the sync storage form", () => {
  it("makes trimmed settings and keeps the secret as typed", () => {
    const s3 = {
      ...emptyStorageForm(),
      endpoint: " https://s3.example.com ",
      region: " auto ",
      bucket: " vault ",
      accessKeyId: " AKID ",
      secretAccessKey: " secret ",
      prefix: " lockra ",
      pathStyle: true,
    };
    expect(storageConfig(s3)).toEqual({
      kind: "s3",
      endpoint: "https://s3.example.com",
      region: "auto",
      bucket: "vault",
      prefix: "lockra",
      access_key_id: "AKID",
      secret_access_key: " secret ",
      path_style: true,
    });
    const dav = {
      ...emptyStorageForm(),
      kind: "webdav" as const,
      url: " https://dav.example.com/ ",
      username: " me ",
      password: " pw ",
    };
    expect(storageConfig(dav)).toEqual({
      kind: "webdav",
      url: "https://dav.example.com/",
      prefix: "lockra",
      username: "me",
      password: " pw ",
    });
  });

  it("is complete once every field of its kind is filled", () => {
    const s3 = { ...emptyStorageForm(), endpoint: "e", region: "r", bucket: "b", accessKeyId: "a" };
    expect(storageComplete(s3)).toBe(false);
    expect(storageComplete({ ...s3, secretAccessKey: "s" })).toBe(true);
    const dav = { ...emptyStorageForm(), kind: "webdav" as const, url: "u", username: "me" };
    expect(storageComplete(dav)).toBe(false);
    expect(storageComplete({ ...dav, password: "  " })).toBe(false);
    expect(storageComplete({ ...dav, password: "pw" })).toBe(true);
  });

  it("starts from a space's settings without the secret, and sums them up in a line", () => {
    const s3 = {
      kind: "s3" as const,
      endpoint: "https://s3.eu-central-1.amazonaws.com",
      region: "eu-central-1",
      bucket: "vault",
      prefix: "/lockra/",
      access_key_id: "AKID",
      path_style: false,
    };
    expect(storageFormFrom(s3)).toMatchObject({
      kind: "s3",
      endpoint: s3.endpoint,
      bucket: "vault",
      accessKeyId: "AKID",
      secretAccessKey: "",
    });
    expect(storageSummary(s3)).toBe("s3.eu-central-1.amazonaws.com / vault / lockra");
    const dav = { kind: "webdav" as const, url: "not an address", prefix: "", username: "me" };
    expect(storageFormFrom(dav)).toMatchObject({
      kind: "webdav",
      url: "not an address",
      password: "",
    });
    expect(storageSummary(dav)).toBe("not an address");
  });

  it("tells a sealed invitation, which needs its code, from a plain one", () => {
    expect(isSealedInvite("  lockra-invite:2:TEtTSU5WVDI\n")).toBe(true);
    expect(isSealedInvite("lockra-invite:1:eyJzdG9yYWdlIjp7fX0")).toBe(false);
    expect(isSealedInvite("")).toBe(false);
  });
});
