// A sync storage's fields, S3-compatible or WebDAV (the desktop's Settings › Sync and the phone's
// sync pages), or on the desktop a folder a cloud drive keeps in sync; the form's logic is
// @lockra/shared's storage-form and storage-presets.
import {
  type ErrorCode,
  PRESETS,
  type PresetId,
  type StorageForm,
  errorText,
  isLockraError,
  presetProblem,
  presetsOf,
  withPreset,
} from "@lockra/shared";
import { useId, useState } from "react";
import { useT } from "../i18n/I18nProvider";
import { Button } from "./Button";
import { Input } from "./Input";
import { PasswordField } from "./PasswordField";
import { Segmented } from "./Segmented";
import { Select } from "./Select";
import { Toggle } from "./Toggle";

type FailureHint = "awsPartition" | "appPassword" | "ossDomain" | "cosBucket";

/** What a failed connection most likely means for a preset, beyond the error itself. */
function failureHint(preset: PresetId, failure: ErrorCode | undefined): FailureHint | null {
  const denied = failure === "sync_denied" || failure === "sync_wrong_credentials";
  if ((preset === "aws" || preset === "aws-cn") && denied) return "awsPartition";
  if ((preset === "jianguoyun" || preset === "nextcloud") && failure === "sync_denied") {
    return "appPassword";
  }
  if (preset === "oss" && (denied || failure === "sync_storage_failed")) return "ossDomain";
  if (preset === "cos" && failure === "sync_storage_failed") return "cosBucket";
  return null;
}

/** The storage's fields: S3-compatible or WebDAV, by provider. `lg` is the phone's: taller
 *  fields. `failure` is the error the last attempt with these settings ended in. With
 *  `pickFolder` (the desktop's folder dialog), a folder a cloud drive keeps in sync is offered
 *  too: the dialog chooses it, and the form only shows it. */
export function StorageFields({
  form,
  onChange,
  size = "md",
  failure,
  pickFolder,
}: {
  form: StorageForm;
  onChange: (patch: Partial<StorageForm>) => void;
  size?: "md" | "lg";
  failure?: ErrorCode;
  pickFolder?: () => Promise<string | null>;
}) {
  const t = useT();
  const regionsId = useId();
  const [picking, setPicking] = useState(false);
  const [pickError, setPickError] = useState<ErrorCode | undefined>(undefined);
  const preset = PRESETS[form.preset].kind === form.kind ? PRESETS[form.preset] : null;
  const fields =
    preset?.fields ?? (form.kind === "s3" ? ["endpoint", "region", "pathStyle"] : ["url"]);
  const has = (field: (typeof fields)[number]) => fields.includes(field);
  // Every change goes through the preset, which fills in what it decides.
  const change = (patch: Partial<StorageForm>) => onChange(withPreset(form, patch));
  const problem = presetProblem(form);
  // The custom presets ask for every field and have nothing to add.
  const hinted = preset?.id;
  const hint =
    hinted === undefined || hinted === "s3-custom" || hinted === "webdav-custom"
      ? ""
      : t(`sync.storage.presetHints.${hinted}`);
  const failed = failureHint(form.preset, failure);
  const address = preset?.address ? (form.kind === "s3" ? form.endpoint : form.url) : "";
  const regionError =
    problem === "awsChinaRegion" || problem === "awsGlobalRegion" || problem === "regionFormat"
      ? t(`sync.storage.problems.${problem}`)
      : undefined;
  const pick = async () => {
    if (!pickFolder) return;
    setPicking(true);
    setPickError(undefined);
    try {
      const folder = await pickFolder();
      if (folder !== null) change({ folder });
    } catch (error: unknown) {
      setPickError(isLockraError(error) ? error.code : "internal");
    } finally {
      setPicking(false);
    }
  };
  const kinds = [
    { value: "s3" as const, label: t("sync.storage.s3") },
    { value: "webdav" as const, label: t("sync.storage.webdav") },
    ...(pickFolder !== undefined || form.kind === "folder"
      ? [{ value: "folder" as const, label: t("sync.storage.folder") }]
      : []),
  ];
  const kindHint = {
    s3: t("sync.storage.s3Hint"),
    webdav: t("sync.storage.webdavHint"),
    folder: t("sync.storage.folderHint"),
  }[form.kind];
  const kindSwitch = (
    <div className="flex flex-col gap-1">
      <Segmented
        size={size}
        label={t("sync.storage.kind")}
        value={form.kind}
        onChange={(kind) => change({ kind })}
        options={kinds}
        className="self-start"
      />
      <p className="text-[12px] text-fg-subtle">{kindHint}</p>
    </div>
  );
  if (form.kind === "folder") {
    return (
      <div className="flex flex-col gap-3" data-testid="storage-fields">
        {kindSwitch}
        <p className="text-[12px] text-fg-subtle">{t("sync.storage.presetHints.folder")}</p>
        <div className="flex min-w-0 flex-col gap-1" data-testid="storage-folder">
          <div className="text-[12px] text-fg-muted">{t("sync.storage.folderLabel")}</div>
          {form.folder === "" ? (
            <div className="text-[13px] text-fg-subtle">{t("sync.storage.folderNone")}</div>
          ) : (
            <div
              className="min-w-0 font-mono text-[12px] break-all text-fg"
              data-testid="storage-folder-path">
              {form.folder}
            </div>
          )}
        </div>
        {pickFolder && (
          <Button
            variant="outline"
            icon="folder"
            size={size}
            loading={picking}
            onClick={() => void pick()}
            className="self-start"
            data-testid="storage-folder-pick">
            {t(form.folder === "" ? "sync.storage.folderChoose" : "sync.storage.folderChange")}
          </Button>
        )}
        {pickError !== undefined && (
          <p className="text-[12px] text-danger" role="alert">
            {errorText(t, pickError)}
          </p>
        )}
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-3" data-testid="storage-fields">
      {kindSwitch}
      <div className="flex flex-col gap-1">
        <Select
          size={size}
          label={t("sync.storage.provider")}
          value={preset?.id ?? form.preset}
          onChange={(id) => change({ preset: id })}
          options={presetsOf(form.kind).map((p) => ({
            value: p.id,
            label: t(`sync.storage.presets.${p.id}`),
          }))}
          data-testid="storage-provider"
          className="self-start"
        />
        {hint !== "" && (
          <p className="text-[12px] text-fg-subtle" data-testid="storage-preset-hint">
            {hint}
          </p>
        )}
      </div>
      {form.kind === "s3" ? (
        <div className="grid gap-3 sm:grid-cols-2">
          {has("endpoint") && (
            <Input
              size={size}
              label={t("sync.storage.endpoint")}
              value={form.endpoint}
              onChange={(e) => change({ endpoint: e.target.value })}
              placeholder="https://…"
              help={t("sync.storage.endpointHint")}
              mono
              spellCheck={false}
              className="sm:col-span-2"
            />
          )}
          {has("account") && (
            <Input
              size={size}
              label={t("sync.storage.account")}
              value={form.account}
              onChange={(e) => change({ account: e.target.value })}
              error={problem === "r2Account" ? t("sync.storage.problems.r2Account") : undefined}
              mono
              spellCheck={false}
              autoComplete="off"
              className="sm:col-span-2"
            />
          )}
          {has("region") &&
            (preset?.regionsOnly && preset.regions ? (
              <Select
                size={size}
                label={t("sync.storage.region")}
                value={form.region}
                onChange={(region) => change({ region })}
                options={preset.regions.map((region) => ({ value: region, label: region }))}
                mono
                data-testid="storage-region"
              />
            ) : (
              <>
                <Input
                  size={size}
                  label={t("sync.storage.region")}
                  value={form.region}
                  onChange={(e) => change({ region: e.target.value })}
                  placeholder={preset?.regions?.[0] ?? "eu-central-1"}
                  help={preset?.regions ? undefined : t("sync.storage.regionHint")}
                  error={regionError}
                  list={preset?.regions ? regionsId : undefined}
                  mono
                  spellCheck={false}
                  data-testid="storage-region"
                />
                {preset?.regions && (
                  <datalist id={regionsId} aria-label={t("sync.storage.regionChoices")}>
                    {preset.regions.map((region) => (
                      <option key={region} value={region} />
                    ))}
                  </datalist>
                )}
              </>
            ))}
          <Input
            size={size}
            label={t("sync.storage.bucket")}
            value={form.bucket}
            onChange={(e) => change({ bucket: e.target.value })}
            mono
            spellCheck={false}
          />
          <Input
            size={size}
            label={t("sync.storage.accessKeyId")}
            value={form.accessKeyId}
            onChange={(e) => change({ accessKeyId: e.target.value })}
            mono
            spellCheck={false}
            autoComplete="off"
          />
          <PasswordField
            size={size}
            label={t("sync.storage.secretAccessKey")}
            value={form.secretAccessKey}
            onChange={(secretAccessKey) => change({ secretAccessKey })}
            autoComplete="off"
          />
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2">
          {has("url") && (
            <Input
              size={size}
              label={t("sync.storage.url")}
              value={form.url}
              onChange={(e) => change({ url: e.target.value })}
              placeholder="https://…"
              help={t("sync.storage.urlHint")}
              mono
              spellCheck={false}
              className="sm:col-span-2"
            />
          )}
          {has("host") && (
            <Input
              size={size}
              label={t("sync.storage.host")}
              value={form.host}
              onChange={(e) => change({ host: e.target.value })}
              placeholder="cloud.example.com"
              help={t("sync.storage.hostHint")}
              mono
              spellCheck={false}
              autoComplete="off"
              className="sm:col-span-2"
            />
          )}
          <Input
            size={size}
            label={t("sync.storage.username")}
            value={form.username}
            onChange={(e) => change({ username: e.target.value })}
            spellCheck={false}
            autoComplete="off"
          />
          <PasswordField
            size={size}
            label={t("sync.storage.password")}
            value={form.password}
            onChange={(password) => change({ password })}
            help={t("sync.storage.passwordHint")}
            autoComplete="off"
          />
        </div>
      )}
      {address !== "" && (
        <p className="min-w-0 break-all text-[12px] text-fg-subtle" data-testid="storage-address">
          {t("sync.storage.address")}: <span className="font-mono text-fg-muted">{address}</span>
        </p>
      )}
      <div className="grid gap-3 sm:grid-cols-2">
        <Input
          size={size}
          label={t("sync.storage.prefix")}
          value={form.prefix}
          onChange={(e) => change({ prefix: e.target.value })}
          help={t("sync.storage.prefixHint")}
          mono
          spellCheck={false}
        />
        {form.kind === "s3" && has("pathStyle") && (
          <div className="flex items-start justify-between gap-3 pt-5">
            <div className="min-w-0">
              <div className="text-[13px] text-fg">{t("sync.storage.pathStyle")}</div>
              <div className="text-[12px] text-fg-subtle">{t("sync.storage.pathStyleHint")}</div>
            </div>
            <Toggle
              size={size}
              checked={form.pathStyle}
              onChange={(pathStyle) => change({ pathStyle })}
              ariaLabel={t("sync.storage.pathStyle")}
            />
          </div>
        )}
      </div>
      {failed !== null && (
        <p className="text-[12px] text-warning" role="status" data-testid="storage-failure-hint">
          {t(`sync.storage.failures.${failed}`)}
        </p>
      )}
      <p className="text-[12px] text-fg-subtle">{t("sync.storage.httpsOnly")}</p>
    </div>
  );
}
