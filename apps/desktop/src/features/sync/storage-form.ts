// The storage form's fields, the settings they make and the ones they start from. The secret
// (S3 secret key, WebDAV password) is typed in every time: the core never sends it back.
import type { StorageConfig, StorageKind, StorageView } from "@lockra/shared";

export interface StorageForm {
  kind: StorageKind;
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

/** A space's settings as the form shows them, the secret left empty. */
export function storageFormFrom(view: StorageView): StorageForm {
  const empty = emptyStorageForm();
  return view.kind === "s3"
    ? {
        ...empty,
        kind: "s3",
        endpoint: view.endpoint,
        region: view.region,
        bucket: view.bucket,
        accessKeyId: view.access_key_id,
        pathStyle: view.path_style,
        prefix: view.prefix,
      }
    : { ...empty, kind: "webdav", url: view.url, username: view.username, prefix: view.prefix };
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

/** Every field the kind needs is filled. */
export function storageComplete(form: StorageForm): boolean {
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
