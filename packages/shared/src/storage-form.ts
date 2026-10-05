// The sync storage form's fields (the desktop's and the phone's), the settings they make and the
// ones they start from. The secret (S3 secret key, WebDAV password) is typed in every time: the
// core never sends it back.
import type { StorageConfig, StorageKind, StorageView } from "./schema";
import {
  type PresetId,
  PRESETS,
  customPreset,
  detectPreset,
  presetProblem,
} from "./storage-presets";

export interface StorageForm {
  kind: StorageKind;
  /** The service chosen by name (storage-presets.ts); the custom ones ask for every field. */
  preset: PresetId;
  /** Cloudflare R2's account id. */
  account: string;
  /** The server of a Nextcloud or Synology preset, with its port when not the usual one. */
  host: string;
  endpoint: string;
  region: string;
  bucket: string;
  accessKeyId: string;
  secretAccessKey: string;
  pathStyle: boolean;
  url: string;
  username: string;
  password: string;
  prefix: string;
}

export function emptyStorageForm(): StorageForm {
  return {
    kind: "s3",
    preset: customPreset("s3"),
    account: "",
    host: "",
    endpoint: "",
    region: "",
    bucket: "",
    accessKeyId: "",
    secretAccessKey: "",
    pathStyle: false,
    url: "",
    username: "",
    password: "",
    prefix: "lockra",
  };
}

/** A space's settings as the form shows them, the secret left empty, on the preset that makes
 *  them when there is one. */
export function storageFormFrom(view: StorageView): StorageForm {
  const empty = emptyStorageForm();
  const { region, ...preset } = detectPreset(view);
  return view.kind === "s3"
    ? {
        ...empty,
        ...preset,
        kind: "s3",
        endpoint: view.endpoint,
        region: region ?? view.region,
        bucket: view.bucket,
        accessKeyId: view.access_key_id,
        pathStyle: view.path_style,
        prefix: view.prefix,
      }
    : {
        ...empty,
        ...preset,
        kind: "webdav",
        url: view.url,
        username: view.username,
        prefix: view.prefix,
      };
}

/** The settings the core receives (the core checks them again). */
export function storageConfig(form: StorageForm): StorageConfig {
  return form.kind === "s3"
    ? {
        kind: "s3",
        endpoint: form.endpoint.trim(),
        region: form.region.trim(),
        bucket: form.bucket.trim(),
        prefix: form.prefix.trim(),
        access_key_id: form.accessKeyId.trim(),
        secret_access_key: form.secretAccessKey,
        path_style: form.pathStyle,
      }
    : {
        kind: "webdav",
        url: form.url.trim(),
        prefix: form.prefix.trim(),
        username: form.username.trim(),
        password: form.password,
      };
}

/** Every field the kind needs is filled, and the preset's settings can be used. */
export function storageComplete(form: StorageForm): boolean {
  // A preset of the other kind (a form put together by hand) counts as the custom one.
  const own =
    PRESETS[form.preset].kind === form.kind ? form : { ...form, preset: customPreset(form.kind) };
  if (presetProblem(own) !== null) return false;
  const required =
    form.kind === "s3"
      ? [form.endpoint, form.region, form.bucket, form.accessKeyId, form.secretAccessKey]
      : [form.url, form.username, form.password];
  return required.every((value) => value.trim() !== "");
}

/** Where a space lives, in one line: the host and the folder (no credential). */
export function storageSummary(view: StorageView): string {
  const address = view.kind === "s3" ? view.endpoint : view.url;
  let host = address;
  try {
    host = new URL(address).host;
  } catch {
    // Shown as it is.
  }
  const place = view.kind === "s3" ? [view.bucket, view.prefix] : [view.prefix];
  return [host, ...place.map((p) => p.replace(/^\/+|\/+$/g, "")).filter((p) => p !== "")].join(
    " / ",
  );
}

/** A sealed invitation (`lockra-invite:2:…`): it opens with the one-time code shown beside it. */
export function isSealedInvite(text: string): boolean {
  return text.trim().startsWith("lockra-invite:2:");
}
