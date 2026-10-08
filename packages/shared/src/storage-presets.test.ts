import { emptyStorageForm, storageComplete, storageConfig, storageFormFrom } from "./storage-form";
import {
  BUILT_IN_RELAY,
  PRESETS,
  customPreset,
  detectPreset,
  presetProblem,
  presetsOf,
  startPreset,
  withPreset,
} from "./storage-presets";

// A new form starts on Lockra's relay; these are the empty forms of the other kinds.
const s3 = withPreset(emptyStorageForm(), { kind: "s3" });
const dav = withPreset(s3, { kind: "webdav" });

describe("storage presets", () => {
  it("make each S3 service's address from what is typed, with its addressing", () => {
    const cases: [Parameters<typeof withPreset>[1], string, boolean][] = [
      [{ preset: "aws", region: "eu-central-1" }, "https://s3.eu-central-1.amazonaws.com", false],
      [{ preset: "oss", region: "cn-hangzhou" }, "https://s3.oss-cn-hangzhou.aliyuncs.com", false],
      [{ preset: "cos", region: "ap-guangzhou" }, "https://cos.ap-guangzhou.myqcloud.com", false],
      [{ preset: "b2", region: "us-west-004" }, "https://s3.us-west-004.backblazeb2.com", false],
    ];
    for (const [patch, endpoint, pathStyle] of cases) {
      const form = withPreset({ ...s3, pathStyle: true }, patch);
      expect(form.endpoint).toBe(endpoint);
      expect(form.pathStyle).toBe(pathStyle);
    }
    // Nothing to make the address from yet.
    expect(withPreset(s3, { preset: "aws" }).endpoint).toBe("");
  });

  it("keep the China partition to its two regions, and its own domain", () => {
    const cn = withPreset(s3, { preset: "aws-cn" });
    expect(cn.region).toBe("cn-north-1");
    expect(cn.endpoint).toBe("https://s3.cn-north-1.amazonaws.com.cn");
    expect(withPreset(cn, { region: "cn-northwest-1" }).endpoint).toBe(
      "https://s3.cn-northwest-1.amazonaws.com.cn",
    );
    // A global region typed into the China preset does not stay.
    expect(withPreset(cn, { region: "us-east-1" }).region).toBe("cn-north-1");
    expect(PRESETS["aws-cn"].regions).toEqual(["cn-north-1", "cn-northwest-1"]);
  });

  it("give R2 its account endpoint and the region R2 names, auto", () => {
    const r2 = withPreset(s3, { preset: "r2", account: " 0123456789ABCDEF0123456789abcdef " });
    expect(r2.endpoint).toBe("https://0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com");
    expect(r2.region).toBe("auto");
    expect(r2.pathStyle).toBe(false);
  });

  it("leave MinIO's address to the user but turn path-style requests on", () => {
    const minio = withPreset(
      { ...s3, endpoint: "https://minio.example.com:9000" },
      { preset: "minio" },
    );
    expect(minio.endpoint).toBe("https://minio.example.com:9000");
    expect(minio.pathStyle).toBe(true);
    // The custom one leaves the toggle as it is.
    expect(withPreset({ ...s3, pathStyle: true }, { preset: "s3-custom" }).pathStyle).toBe(true);
  });

  it("make each WebDAV service's address", () => {
    expect(withPreset(dav, { preset: "jianguoyun" }).url).toBe("https://dav.jianguoyun.com/dav/");
    const nextcloud = withPreset(dav, {
      preset: "nextcloud",
      host: "https://cloud.example.com/",
      username: "me@example.com",
    });
    expect(nextcloud.url).toBe("https://cloud.example.com/remote.php/dav/files/me%40example.com/");
    expect(withPreset(dav, { preset: "nextcloud", host: "cloud.example.com" }).url).toBe("");
    expect(withPreset(dav, { preset: "synology", host: "nas.example.com" }).url).toBe(
      "https://nas.example.com:5006/",
    );
    expect(withPreset(dav, { preset: "synology", host: "nas.example.com:443" }).url).toBe(
      "https://nas.example.com:443/",
    );
    expect(withPreset(dav, { preset: "synology", host: " " }).url).toBe("");
  });

  it("start on Lockra's relay, which needs nothing typed in, and keep its address to relays", () => {
    const form = emptyStorageForm();
    expect(form).toMatchObject({ kind: "relay", preset: "relay-hosted", url: BUILT_IN_RELAY });
    expect(storageComplete(form)).toBe(true);
    expect(storageConfig(form)).toEqual({ kind: "relay", url: BUILT_IN_RELAY });
    expect(presetsOf("relay").map((p) => p.id)).toEqual(["relay-hosted", "relay-custom"]);
    expect([startPreset("relay"), customPreset("relay")]).toEqual(["relay-hosted", "relay-custom"]);
    // One's own relay keeps what is typed; back on the built-in one, its address returns.
    const own = withPreset(form, { preset: "relay-custom" });
    expect(own.url).toBe(BUILT_IN_RELAY);
    const typed = withPreset(own, { url: " https://relay.example.com/ " });
    expect(storageConfig(typed)).toEqual({ kind: "relay", url: "https://relay.example.com/" });
    expect(storageComplete({ ...typed, url: " " })).toBe(false);
    expect(withPreset(typed, { preset: "relay-hosted" }).url).toBe(BUILT_IN_RELAY);
    // A relay's address is no WebDAV address, nor the other way round.
    expect(withPreset(typed, { kind: "webdav" })).toMatchObject({
      preset: "webdav-custom",
      url: "",
    });
    const fromDav = withPreset({ ...dav, url: "https://dav.example.com/" }, { kind: "relay" });
    expect(fromDav).toMatchObject({ preset: "relay-hosted", url: BUILT_IN_RELAY });
    // Saved settings find their preset again.
    expect(detectPreset({ kind: "relay", url: `${BUILT_IN_RELAY}/` }).preset).toBe("relay-hosted");
    expect(detectPreset({ kind: "relay", url: "https://relay.example.com" }).preset).toBe(
      "relay-custom",
    );
    expect(storageFormFrom({ kind: "relay", url: "https://relay.example.com" })).toMatchObject({
      kind: "relay",
      preset: "relay-custom",
      url: "https://relay.example.com",
    });
  });

  it("start a new kind on its custom preset and never keep another kind's", () => {
    expect(dav.preset).toBe("webdav-custom");
    expect(withPreset(dav, { kind: "s3" }).preset).toBe("s3-custom");
    expect(withPreset(s3, { preset: "jianguoyun" }).preset).toBe("s3-custom");
    expect(customPreset("webdav")).toBe("webdav-custom");
    expect(presetsOf("s3").map((p) => p.id)).toEqual([
      "aws",
      "aws-cn",
      "r2",
      "oss",
      "cos",
      "b2",
      "minio",
      "s3-custom",
    ]);
    expect(presetsOf("webdav").map((p) => p.id)).toEqual([
      "jianguoyun",
      "nextcloud",
      "synology",
      "webdav-custom",
    ]);
  });

  it("keep the address a preset made when the custom one takes over", () => {
    const aws = withPreset(s3, { preset: "aws", region: "us-west-2" });
    const custom = withPreset(aws, { preset: "s3-custom" });
    expect(custom.endpoint).toBe("https://s3.us-west-2.amazonaws.com");
    expect(withPreset(custom, { endpoint: "https://s3.example.com" }).endpoint).toBe(
      "https://s3.example.com",
    );
  });

  it("refuse a China region under the global partition and the other way round", () => {
    expect(presetProblem({ ...s3, preset: "aws", region: "cn-north-1" })).toBe("awsChinaRegion");
    expect(presetProblem({ ...s3, preset: "aws-cn", region: "us-east-1" })).toBe("awsGlobalRegion");
    expect(presetProblem({ ...s3, preset: "aws", region: "EU Central" })).toBe("regionFormat");
    expect(presetProblem({ ...s3, preset: "oss", region: "cn-hangzhou" })).toBeNull();
    expect(presetProblem({ ...s3, preset: "aws", region: "" })).toBeNull();
    expect(presetProblem({ ...s3, preset: "r2", account: "not-an-id" })).toBe("r2Account");
    expect(presetProblem({ ...s3, preset: "r2", account: "" })).toBeNull();
    expect(presetProblem({ ...s3, preset: "s3-custom", region: "Any Region" })).toBeNull();
    const filled = withPreset(
      { ...s3, bucket: "b", accessKeyId: "a", secretAccessKey: "s" },
      { preset: "aws", region: "us-east-1" },
    );
    expect(storageComplete(filled)).toBe(true);
    expect(storageComplete({ ...filled, region: "cn-north-1" })).toBe(false);
    // A preset of the other kind counts as the custom one: the fields decide.
    expect(storageComplete({ ...filled, preset: "jianguoyun" })).toBe(true);
    expect(storageComplete({ ...filled, preset: "jianguoyun", region: "Any Region" })).toBe(true);
  });

  it("find the preset of saved settings, and only when it would make them again", () => {
    const view = (endpoint: string, region = "x", path_style = false) => ({
      kind: "s3" as const,
      endpoint,
      region,
      bucket: "b",
      prefix: "lockra",
      access_key_id: "AKID",
      path_style,
    });
    expect(detectPreset(view("https://s3.eu-west-1.amazonaws.com/"))).toMatchObject({
      preset: "aws",
      region: "eu-west-1",
    });
    expect(detectPreset(view("https://s3.cn-northwest-1.amazonaws.com.cn"))).toMatchObject({
      preset: "aws-cn",
      region: "cn-northwest-1",
    });
    expect(detectPreset(view("https://s3.cn-north-1.amazonaws.com")).preset).toBe("s3-custom");
    expect(detectPreset(view("https://s3.oss-cn-beijing.aliyuncs.com")).preset).toBe("oss");
    expect(detectPreset(view("https://cos.ap-shanghai.myqcloud.com")).preset).toBe("cos");
    expect(detectPreset(view("https://s3.eu-central-003.backblazeb2.com")).preset).toBe("b2");
    const account = "0123456789abcdef0123456789abcdef";
    expect(detectPreset(view(`https://${account}.r2.cloudflarestorage.com`, "auto"))).toEqual({
      preset: "r2",
      account,
      host: "",
    });
    expect(detectPreset(view(`https://${account}.r2.cloudflarestorage.com`, "eu")).preset).toBe(
      "s3-custom",
    );
    expect(detectPreset(view("https://s3.eu-west-1.amazonaws.com", "x", true)).preset).toBe(
      "s3-custom",
    );
    const webdav = (url: string, username = "me@example.com") => ({
      kind: "webdav" as const,
      url,
      prefix: "lockra",
      username,
    });
    expect(detectPreset(webdav("https://dav.jianguoyun.com/dav")).preset).toBe("jianguoyun");
    expect(
      detectPreset(webdav("https://cloud.example.com/remote.php/dav/files/me%40example.com/")),
    ).toEqual({ preset: "nextcloud", account: "", host: "cloud.example.com" });
    expect(
      detectPreset(webdav("https://cloud.example.com/remote.php/dav/files/someone/")).preset,
    ).toBe("webdav-custom");
    expect(
      detectPreset(webdav("https://cloud.example.com/remote.php/dav/files/%E0%A4%A/", "%E0%A4%A"))
        .preset,
    ).toBe("nextcloud");
    expect(detectPreset(webdav("https://nas.example.com:5006/")).preset).toBe("webdav-custom");
  });

  it("start the form from saved settings on their preset, and make the same settings again", () => {
    const view = {
      kind: "s3" as const,
      endpoint: "https://cos.ap-guangzhou.myqcloud.com",
      region: "ap-guangzhou",
      bucket: "vault-1250000000",
      prefix: "lockra",
      access_key_id: "AKID",
      path_style: false,
    };
    const form = storageFormFrom(view);
    expect(form).toMatchObject({ preset: "cos", region: "ap-guangzhou", secretAccessKey: "" });
    expect(storageConfig({ ...form, secretAccessKey: "s" })).toEqual({
      ...view,
      secret_access_key: "s",
    });
    const nextcloud = storageFormFrom({
      kind: "webdav",
      url: "https://cloud.example.com/remote.php/dav/files/me/",
      prefix: "lockra",
      username: "me",
    });
    expect(nextcloud).toMatchObject({ preset: "nextcloud", host: "cloud.example.com" });
    expect(withPreset(nextcloud, {}).url).toBe(
      "https://cloud.example.com/remote.php/dav/files/me/",
    );
  });
});
