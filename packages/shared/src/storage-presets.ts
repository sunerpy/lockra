// Storage presets for the sync form: a service chosen by name fills in the address, the region and
// the addressing style, so that only the credentials and a region, account or host are typed. The
// form still makes a plain StorageConfig, which the core checks again. Each address format comes
// from the service's own documentation, cited at the preset.
import type { StorageKind, StorageView } from "./schema";
import type { StorageForm } from "./storage-form";

export type PresetId =
  | "aws"
  | "aws-cn"
  | "r2"
  | "oss"
  | "cos"
  | "b2"
  | "minio"
  | "s3-custom"
  | "jianguoyun"
  | "nextcloud"
  | "synology"
  | "webdav-custom";

/** What a preset asks for besides the credentials and the folder. */
export type PresetField = "endpoint" | "region" | "account" | "url" | "host" | "pathStyle";

export interface Preset {
  id: PresetId;
  kind: StorageKind;
  /** The fields the form shows. */
  fields: readonly PresetField[];
  /** Regions to suggest; with `regionsOnly`, the only ones accepted. */
  regions?: readonly string[];
  regionsOnly?: boolean;
  /** The addressing the service needs (`undefined`: the user's choice). */
  pathStyle?: boolean;
  /** The region of a service that has none. */
  fixedRegion?: string;
  /** The address the form makes, `""` while something it needs is missing. */
  address?: (form: StorageForm) => string;
}

/** A host as typed: no scheme, no trailing slash. */
function host(value: string): string {
  return value
    .trim()
    .replace(/^https?:\/\//i, "")
    .replace(/\/+$/, "");
}

function region(form: StorageForm): string {
  return form.region.trim();
}

// https://docs.aws.amazon.com/general/latest/gr/s3.html (regional endpoints, commercial partition).
const AWS_REGIONS = [
  "us-east-1",
  "us-east-2",
  "us-west-1",
  "us-west-2",
  "ca-central-1",
  "sa-east-1",
  "eu-central-1",
  "eu-central-2",
  "eu-west-1",
  "eu-west-2",
  "eu-west-3",
  "eu-north-1",
  "eu-south-1",
  "eu-south-2",
  "ap-east-1",
  "ap-south-1",
  "ap-south-2",
  "ap-northeast-1",
  "ap-northeast-2",
  "ap-northeast-3",
  "ap-southeast-1",
  "ap-southeast-2",
  "ap-southeast-3",
  "ap-southeast-4",
  "me-south-1",
  "me-central-1",
  "il-central-1",
  "af-south-1",
] as const;

// https://www.amazonaws.cn/en/about-aws/regional-product-services/ lists s3.cn-north-1 and
// s3.cn-northwest-1 under `.amazonaws.com.cn`: the China partition, with its own accounts and keys.
const AWS_CN_REGIONS: readonly string[] = ["cn-north-1", "cn-northwest-1"];

// https://help.aliyun.com/zh/oss/user-guide/regions-and-endpoints (region ids).
const OSS_REGIONS = [
  "cn-hangzhou",
  "cn-shanghai",
  "cn-nanjing",
  "cn-qingdao",
  "cn-beijing",
  "cn-zhangjiakou",
  "cn-huhehaote",
  "cn-wulanchabu",
  "cn-shenzhen",
  "cn-heyuan",
  "cn-guangzhou",
  "cn-chengdu",
  "cn-hongkong",
  "ap-southeast-1",
  "ap-northeast-1",
  "us-west-1",
  "us-east-1",
  "eu-central-1",
] as const;

// https://cloud.tencent.com/document/product/436/6224 (regions and access endpoints).
const COS_REGIONS = [
  "ap-beijing",
  "ap-nanjing",
  "ap-shanghai",
  "ap-guangzhou",
  "ap-chengdu",
  "ap-chongqing",
  "ap-hongkong",
  "ap-singapore",
  "ap-tokyo",
  "ap-seoul",
  "ap-bangkok",
  "ap-jakarta",
  "na-siliconvalley",
  "na-ashburn",
  "eu-frankfurt",
  "sa-saopaulo",
] as const;

// The region is part of the endpoint B2 shows on the bucket's page, s3.<region>.backblazeb2.com.
const B2_REGIONS = [
  "us-west-001",
  "us-west-002",
  "us-west-004",
  "us-east-005",
  "eu-central-003",
] as const;

export const PRESETS: Readonly<Record<PresetId, Preset>> = {
  aws: {
    id: "aws",
    kind: "s3",
    fields: ["region"],
    regions: AWS_REGIONS,
    pathStyle: false,
    address: (form) => (region(form) === "" ? "" : `https://s3.${region(form)}.amazonaws.com`),
  },
  "aws-cn": {
    id: "aws-cn",
    kind: "s3",
    fields: ["region"],
    regions: AWS_CN_REGIONS,
    regionsOnly: true,
    pathStyle: false,
    address: (form) => (region(form) === "" ? "" : `https://s3.${region(form)}.amazonaws.com.cn`),
  },
  // https://developers.cloudflare.com/r2/api/s3/api/: the account's endpoint, region `auto`; the
  // SDK examples there address buckets in the host name.
  r2: {
    id: "r2",
    kind: "s3",
    fields: ["account"],
    fixedRegion: "auto",
    pathStyle: false,
    address: (form) =>
      form.account.trim() === ""
        ? ""
        : `https://${form.account.trim().toLowerCase()}.r2.cloudflarestorage.com`,
  },
  // https://www.alibabacloud.com/help/en/oss/developer-reference/use-aws-sdks-to-access-oss:
  // s3.oss-<region>.aliyuncs.com, region id as is, virtual-hosted style only.
  oss: {
    id: "oss",
    kind: "s3",
    fields: ["region"],
    regions: OSS_REGIONS,
    pathStyle: false,
    address: (form) => (region(form) === "" ? "" : `https://s3.oss-${region(form)}.aliyuncs.com`),
  },
  // https://cloud.tencent.com/document/product/436/37421: cos.<region>.myqcloud.com, virtual-hosted
  // style, bucket names end in the account's APPID.
  cos: {
    id: "cos",
    kind: "s3",
    fields: ["region"],
    regions: COS_REGIONS,
    pathStyle: false,
    address: (form) => (region(form) === "" ? "" : `https://cos.${region(form)}.myqcloud.com`),
  },
  // https://www.backblaze.com/docs/cloud-storage-s3-compatible-api
  b2: {
    id: "b2",
    kind: "s3",
    fields: ["region"],
    regions: B2_REGIONS,
    pathStyle: false,
    address: (form) => (region(form) === "" ? "" : `https://s3.${region(form)}.backblazeb2.com`),
  },
  // MinIO answers on the address it is deployed at and needs path-style requests.
  minio: {
    id: "minio",
    kind: "s3",
    fields: ["endpoint", "region"],
    regions: ["us-east-1"],
    pathStyle: true,
  },
  "s3-custom": { id: "s3-custom", kind: "s3", fields: ["endpoint", "region", "pathStyle"] },
  // https://help.jianguoyun.com/?p=2064: one address for every account, with an app password.
  jianguoyun: {
    id: "jianguoyun",
    kind: "webdav",
    fields: [],
    address: () => "https://dav.jianguoyun.com/dav/",
  },
  // https://docs.nextcloud.com/server/latest/user_manual/en/files/access_webdav.html
  nextcloud: {
    id: "nextcloud",
    kind: "webdav",
    fields: ["host"],
    address: (form) =>
      host(form.host) === "" || form.username.trim() === ""
        ? ""
        : `https://${host(form.host)}/remote.php/dav/files/${encodeURIComponent(form.username.trim())}/`,
  },
  // Synology's WebDAV Server package listens for HTTPS on 5006 unless changed.
  synology: {
    id: "synology",
    kind: "webdav",
    fields: ["host"],
    address: (form) => {
      const typed = host(form.host);
      if (typed === "") return "";
      return /:\d+$/.test(typed) ? `https://${typed}/` : `https://${typed}:5006/`;
    },
  },
  "webdav-custom": { id: "webdav-custom", kind: "webdav", fields: ["url"] },
};

/** The presets of a kind, in the order the form lists them. */
export function presetsOf(kind: StorageKind): Preset[] {
  return Object.values(PRESETS).filter((preset) => preset.kind === kind);
}

/** The preset a kind starts with: the one that asks for every field. */
export function customPreset(kind: StorageKind): PresetId {
  return kind === "s3" ? "s3-custom" : "webdav-custom";
}

/**
 * The form after `patch`, with what its preset decides filled in: the address, the region of a
 * service without one, and the addressing style. A new kind starts on its custom preset; a
 * preset left for the custom one keeps the address it made, ready to be edited.
 */
export function withPreset(form: StorageForm, patch: Partial<StorageForm>): StorageForm {
  const next = { ...form, ...patch };
  if (patch.kind !== undefined && patch.kind !== form.kind && patch.preset === undefined) {
    next.preset = customPreset(patch.kind);
  }
  const preset = PRESETS[next.preset];
  if (preset.kind !== next.kind) next.preset = customPreset(next.kind);
  const chosen = PRESETS[next.preset];
  if (chosen.pathStyle !== undefined) next.pathStyle = chosen.pathStyle;
  if (chosen.fixedRegion !== undefined) next.region = chosen.fixedRegion;
  if (chosen.regionsOnly && chosen.regions && !chosen.regions.includes(next.region.trim())) {
    next.region = chosen.regions[0] ?? "";
  }
  if (chosen.address) {
    const address = chosen.address(next);
    if (next.kind === "s3") next.endpoint = address;
    else next.url = address;
  }
  return next;
}

/** Why the form's preset settings cannot be used as they are. */
export type PresetProblem = "awsChinaRegion" | "awsGlobalRegion" | "regionFormat" | "r2Account";

export function presetProblem(form: StorageForm): PresetProblem | null {
  const typed = form.region.trim();
  switch (form.preset) {
    case "aws":
      if (typed.startsWith("cn-")) return "awsChinaRegion";
      break;
    case "aws-cn":
      if (typed !== "" && !AWS_CN_REGIONS.includes(typed)) return "awsGlobalRegion";
      break;
    case "r2":
      if (form.account.trim() !== "" && !/^[0-9a-f]{32}$/i.test(form.account.trim()))
        return "r2Account";
      return null;
    default:
      return null;
  }
  return typed === "" || /^[a-z]{2,}(-[a-z0-9]+)+$/.test(typed) ? null : "regionFormat";
}

/** The preset that would make these settings, and what it reads from them. */
export function detectPreset(
  view: StorageView,
): Pick<StorageForm, "preset" | "account" | "host"> & { region?: string } {
  const none = { account: "", host: "" };
  if (view.kind === "s3") {
    const endpoint = view.endpoint.trim().replace(/\/+$/, "");
    const matchers: [PresetId, RegExp][] = [
      ["aws-cn", /^https:\/\/s3\.(cn-[a-z0-9-]+)\.amazonaws\.com\.cn$/],
      ["aws", /^https:\/\/s3\.([a-z0-9-]+)\.amazonaws\.com$/],
      ["oss", /^https:\/\/s3\.oss-([a-z0-9-]+)\.aliyuncs\.com$/],
      ["cos", /^https:\/\/cos\.([a-z0-9-]+)\.myqcloud\.com$/],
      ["b2", /^https:\/\/s3\.([a-z0-9-]+)\.backblazeb2\.com$/],
    ];
    if (!view.path_style) {
      for (const [id, pattern] of matchers) {
        const match = pattern.exec(endpoint);
        if (match?.[1] && (id !== "aws" || !match[1].startsWith("cn-"))) {
          return { preset: id, region: match[1], ...none };
        }
      }
      const r2 = /^https:\/\/([0-9a-f]{32})\.r2\.cloudflarestorage\.com$/.exec(endpoint);
      if (r2?.[1] && view.region === "auto") return { preset: "r2", ...none, account: r2[1] };
    }
    return { preset: "s3-custom", ...none };
  }
  const url = view.url.trim();
  if (url.replace(/\/+$/, "") === "https://dav.jianguoyun.com/dav")
    return { preset: "jianguoyun", ...none };
  const nextcloud = /^https:\/\/(.+)\/remote\.php\/dav\/files\/([^/]+)\/?$/.exec(url);
  if (nextcloud?.[1] && nextcloud[2] && safeDecode(nextcloud[2]) === view.username) {
    return { preset: "nextcloud", ...none, host: nextcloud[1] };
  }
  return { preset: "webdav-custom", ...none };
}

function safeDecode(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}
