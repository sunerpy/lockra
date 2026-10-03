// A sync storage's fields, S3-compatible or WebDAV (the desktop's Settings › Sync and the phone's
// sync pages); the form's logic is @lockra/shared's storage-form.
import type { StorageForm } from "@lockra/shared";
import { useT } from "../i18n/I18nProvider";
import { Input } from "./Input";
import { PasswordField } from "./PasswordField";
import { Segmented } from "./Segmented";
import { Toggle } from "./Toggle";

/** The storage's fields: S3-compatible or WebDAV. `lg` is the phone's: taller fields. */
export function StorageFields({
  form,
  onChange,
  size = "md",
}: {
  form: StorageForm;
  onChange: (patch: Partial<StorageForm>) => void;
  size?: "md" | "lg";
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-3" data-testid="storage-fields">
      <div className="flex flex-col gap-1">
        <Segmented
          size={size}
          label={t("sync.storage.kind")}
          value={form.kind}
          onChange={(kind) => onChange({ kind })}
          options={[
            { value: "s3", label: t("sync.storage.s3") },
            { value: "webdav", label: t("sync.storage.webdav") },
          ]}
          className="self-start"
        />
        <p className="text-[12px] text-fg-subtle">
          {form.kind === "s3" ? t("sync.storage.s3Hint") : t("sync.storage.webdavHint")}
        </p>
      </div>
      {form.kind === "s3" ? (
        <div className="grid gap-3 sm:grid-cols-2">
          <Input
            size={size}
            label={t("sync.storage.endpoint")}
            value={form.endpoint}
            onChange={(e) => onChange({ endpoint: e.target.value })}
            placeholder="https://…"
            help={t("sync.storage.endpointHint")}
            mono
            spellCheck={false}
            className="sm:col-span-2"
          />
          <Input
            size={size}
            label={t("sync.storage.region")}
            value={form.region}
            onChange={(e) => onChange({ region: e.target.value })}
            placeholder="eu-central-1"
            help={t("sync.storage.regionHint")}
            mono
            spellCheck={false}
          />
          <Input
            size={size}
            label={t("sync.storage.bucket")}
            value={form.bucket}
            onChange={(e) => onChange({ bucket: e.target.value })}
            mono
            spellCheck={false}
          />
          <Input
            size={size}
            label={t("sync.storage.accessKeyId")}
            value={form.accessKeyId}
            onChange={(e) => onChange({ accessKeyId: e.target.value })}
            mono
            spellCheck={false}
            autoComplete="off"
          />
          <PasswordField
            size={size}
            label={t("sync.storage.secretAccessKey")}
            value={form.secretAccessKey}
            onChange={(secretAccessKey) => onChange({ secretAccessKey })}
            autoComplete="off"
          />
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2">
          <Input
            size={size}
            label={t("sync.storage.url")}
            value={form.url}
            onChange={(e) => onChange({ url: e.target.value })}
            placeholder="https://…"
            help={t("sync.storage.urlHint")}
            mono
            spellCheck={false}
            className="sm:col-span-2"
          />
          <Input
            size={size}
            label={t("sync.storage.username")}
            value={form.username}
            onChange={(e) => onChange({ username: e.target.value })}
            spellCheck={false}
            autoComplete="off"
          />
          <PasswordField
            size={size}
            label={t("sync.storage.password")}
            value={form.password}
            onChange={(password) => onChange({ password })}
            help={t("sync.storage.passwordHint")}
            autoComplete="off"
          />
        </div>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          size={size}
          label={t("sync.storage.prefix")}
          value={form.prefix}
          onChange={(e) => onChange({ prefix: e.target.value })}
          help={t("sync.storage.prefixHint")}
          mono
          spellCheck={false}
        />
        {form.kind === "s3" && (
          <div className="flex items-start justify-between gap-3 pt-5">
            <div className="min-w-0">
              <div className="text-[13px] text-fg">{t("sync.storage.pathStyle")}</div>
              <div className="text-[12px] text-fg-subtle">{t("sync.storage.pathStyleHint")}</div>
            </div>
            <Toggle
              size={size}
              checked={form.pathStyle}
              onChange={(pathStyle) => onChange({ pathStyle })}
              ariaLabel={t("sync.storage.pathStyle")}
            />
          </div>
        )}
      </div>
      <p className="text-[12px] text-fg-subtle">{t("sync.storage.httpsOnly")}</p>
    </div>
  );
}
