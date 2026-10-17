import {
  type ActivityView,
  type BrowserPolicy,
  type BrowserRowState,
  type BrowserStatus,
  type ExtensionFolder,
  type PairedView,
  type SiteRuleView,
  browserName,
  browserRows,
  errorText,
  formatDateTime,
  installedBrowsers,
  olderThan,
  pairedName,
  relativeTime,
  riskierPolicy,
} from "@lockra/shared";
import {
  Banner,
  Button,
  Card,
  Dialog,
  Icon,
  IconButton,
  Input,
  Lamp,
  type LampTone,
  Segmented,
  SettingsPane,
  SettingsRows,
  SettingsSection,
  StatusRow,
  Table,
  TileRow,
  Toggle,
  type VerifyAction,
  useBackend,
  useClock,
  useLocale,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useEffect, useId, useState } from "react";
import { useDispatch, useGuarded, useSubmit } from "../../app/dispatch";
import { useUpdateSettings } from "../../app/settings";
import { useVerify } from "../../features/items/verify";

/** Settings › 浏览器 (boards 13–16): the integration's switch and running in the background; with
 *  it on, each browser's connection, the folder Lockra keeps its extension in, the paired
 *  extensions, the autofill policy, the site lists and the recent activity. What the extensions may do is the vault's (`browser_set_policy`);
 *  whether Lockra listens and which browsers have its manifest are the shell's. */
export function Browser() {
  const t = useT();
  const { backend } = useBackend();
  const guarded = useGuarded();
  const update = useUpdateSettings();
  const { settings, browser, site_rules: siteRules } = useUiState();
  const [statuses, setStatuses] = useState<BrowserStatus[] | null>(null);
  const [prompting, setPrompting] = useState(false);
  const enabled = browser.enabled;

  // The browsers' state is the shell's, not the core's: asked for as the pane opens.
  useEffect(() => {
    if (!enabled) return;
    let live = true;
    void guarded(() => backend.browserStatus()).then((answer) => {
      if (live && answer !== undefined) setStatuses(answer);
    });
    return () => {
      live = false;
    };
  }, [enabled, backend, guarded]);

  const turn = async (on: boolean) => {
    if (!on) {
      const after = await guarded(() => backend.browserDisable());
      if (after !== undefined) setStatuses(after);
      return;
    }
    const before = await guarded(() => backend.browserStatus());
    if (before === undefined) return;
    const after = await guarded(() => backend.browserEnable(installedBrowsers(before)));
    if (after === undefined) return;
    setStatuses(after);
    // The extension needs Lockra running: offer to keep it in the tray.
    if (!settings.run_in_background) setPrompting(true);
  };

  const write = async (id: BrowserStatus["browser"]) => {
    const after = await guarded(() => backend.browserEnable([id]));
    if (after !== undefined) setStatuses(after);
  };

  return (
    <SettingsPane title={t("settings.section.browser")} lede={t("settings.browser.lede")}>
      {enabled &&
        browser.paired.filter((p) => p.paused).map((p) => <PausedBanner key={p.id} paired={p} />)}
      <SettingsRows>
        <StatusRow
          label={t("settings.browser.integration")}
          help={t("settings.browser.integrationHelp")}>
          <Toggle
            checked={enabled}
            onChange={(on) => void turn(on)}
            ariaLabel={t("settings.browser.integration")}
          />
        </StatusRow>
        <StatusRow
          label={t("settings.browser.background")}
          help={t("settings.browser.backgroundHelp")}>
          <Toggle
            checked={settings.run_in_background}
            onChange={(run_in_background) => update({ run_in_background })}
            ariaLabel={t("settings.browser.background")}
          />
        </StatusRow>
      </SettingsRows>
      {!enabled && (
        <p className="text-[12px] leading-4 text-fg-muted" data-testid="browser-off-note">
          {t("settings.browser.offNote")}
        </p>
      )}
      {enabled && (
        <>
          <BrowserList statuses={statuses} paired={browser.paired} onWrite={write} />
          <FolderSection />
          <PairedList paired={browser.paired} />
          <Policy policy={browser.policy} />
          <SiteLists rules={siteRules} />
          <Activity activity={browser.activity} />
        </>
      )}
      <Dialog
        open={prompting}
        title={t("settings.browser.backgroundPrompt.title")}
        onClose={() => setPrompting(false)}
        width={440}
        actions={
          <>
            <Button variant="ghost" onClick={() => setPrompting(false)}>
              {t("settings.browser.backgroundPrompt.later")}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                setPrompting(false);
                update({ run_in_background: true });
              }}
              data-autofocus>
              {t("settings.browser.backgroundPrompt.enable")}
            </Button>
          </>
        }>
        <p>{t("settings.browser.backgroundPrompt.body")}</p>
      </Dialog>
    </SettingsPane>
  );
}

const ROW_LAMP: Record<BrowserRowState, LampTone> = {
  connected: "ok",
  paired: "idle",
  no_extension: "off",
  no_manifest: "warn",
};

function BrowserList({
  statuses,
  paired,
  onWrite,
}: {
  statuses: BrowserStatus[] | null;
  paired: readonly PairedView[];
  onWrite: (id: BrowserStatus["browser"]) => Promise<void>;
}) {
  const t = useT();
  const rows = statuses === null ? [] : browserRows(statuses, paired);
  return (
    <SettingsSection
      title={t("settings.browser.browsers.title")}
      description={t("settings.browser.browsers.description")}>
      <div data-testid="browser-rows">
        {statuses !== null && rows.length === 0 && (
          <p className="text-[12px] text-fg-muted">{t("settings.browser.browsers.none")}</p>
        )}
        {rows.map((row) => (
          <TileRow
            key={row.browser}
            icon="browser"
            title={browserName(row.browser)}
            subtitle={
              <>
                <Lamp tone={ROW_LAMP[row.state]} size={6} />
                {t(`settings.browser.browsers.state.${row.state}`)}
              </>
            }
            actions={
              row.state === "no_manifest" ? (
                <Button size="sm" variant="outline" onClick={() => void onWrite(row.browser)}>
                  {t("settings.browser.browsers.write")}
                </Button>
              ) : undefined
            }
            data-testid={`browser-row-${row.browser}`}
          />
        ))}
      </div>
    </SettingsSection>
  );
}

/** How long 「已复制」 shows on the copy button. */
const COPIED_FOR_MS = 2_000;

/** The folder the shell keeps its copy of the extension in (none in a build that carries none):
 *  where it is, the version, and how to load it. The browsers never update an extension loaded
 *  unpacked: Lockra puts each new one there and the extension reloads itself. */
function FolderSection() {
  const t = useT();
  const { backend } = useBackend();
  const guarded = useGuarded();
  const [folder, setFolder] = useState<ExtensionFolder | null>(null);
  const [copied, setCopied] = useState(false);
  const [notOpened, setNotOpened] = useState(false);
  /** The package's address, when no browser opened it. */
  const [address, setAddress] = useState<string | null>(null);

  useEffect(() => {
    let live = true;
    void guarded(() => backend.browserExtension()).then((answer) => {
      if (live && answer !== undefined) setFolder(answer);
    });
    return () => {
      live = false;
    };
  }, [backend, guarded]);

  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_FOR_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  const download = async () => {
    const shown = await guarded(() => backend.browserExtensionDownload());
    if (shown !== undefined) setAddress(shown);
  };
  const packageLine = (
    <div className="mt-3 flex items-start gap-3" data-testid="extension-download">
      <p className="min-w-0 flex-1 text-[12px] leading-4 text-fg-muted">
        {t("settings.browser.folder.downloadHint")}
        {address !== null && (
          <span className="mt-1 block text-warning-text">
            {t("settings.browser.folder.downloadNotOpened")}{" "}
            <span
              className="font-mono break-all text-fg select-text"
              data-testid="extension-download-address">
              {address}
            </span>
          </span>
        )}
      </p>
      <Button size="sm" variant="outline" icon="download" onClick={() => void download()}>
        {t("settings.browser.folder.download")}
      </Button>
    </div>
  );
  if (folder === null) {
    return (
      <SettingsSection title={t("settings.browser.folder.title")}>{packageLine}</SettingsSection>
    );
  }
  const copy = async () => {
    const done = await guarded(async () => {
      await backend.browserExtensionCopy();
      return true;
    });
    if (done) setCopied(true);
  };
  const open = async () => {
    const opened = await guarded(() => backend.browserExtensionReveal());
    if (opened !== undefined) setNotOpened(!opened);
  };
  return (
    <SettingsSection
      title={t("settings.browser.folder.title")}
      description={t("settings.browser.folder.description")}>
      <div data-testid="extension-folder">
        <TileRow
          icon="folder"
          title={
            <span className="block truncate font-mono text-[12px] select-text" title={folder.path}>
              {folder.path}
            </span>
          }
          subtitle={
            <span className="flex items-center gap-1.5" data-testid="extension-folder-state">
              <Lamp tone={folder.ready ? "ok" : "warn"} size={6} />
              {folder.ready
                ? t("settings.browser.folder.carries", { version: folder.version })
                : t("settings.browser.folder.notReady")}
            </span>
          }
          actions={
            <>
              <Button
                size="sm"
                variant="outline"
                icon={copied ? "check" : "copy"}
                onClick={() => void copy()}>
                {copied ? t("settings.browser.folder.copied") : t("settings.browser.folder.copy")}
              </Button>
              <Button size="sm" variant="outline" icon="folder" onClick={() => void open()}>
                {t("settings.browser.folder.open")}
              </Button>
            </>
          }
          note={
            notOpened ? (
              <span className="text-warning-text">{t("settings.browser.folder.notOpened")}</span>
            ) : undefined
          }
        />
      </div>
      <ol
        className="mt-2 list-decimal space-y-1 pl-5 text-[12px] leading-4 text-fg-muted"
        data-testid="extension-folder-steps">
        <li>{t("settings.browser.folder.step1")}</li>
        <li>{t("settings.browser.folder.step2")}</li>
        <li>{t("settings.browser.folder.step3")}</li>
      </ol>
      {packageLine}
    </SettingsSection>
  );
}

function PairedList({ paired }: { paired: readonly PairedView[] }) {
  const t = useT();
  const locale = useLocale();
  const now = useClock();
  const dispatch = useDispatch();
  const { app_version: appVersion } = useUiState();
  const [unpairing, setUnpairing] = useState<PairedView | null>(null);
  const meta = (p: PairedView) =>
    t("settings.browser.paired.meta", {
      date: formatDateTime(locale, p.paired_at_ms, { month: "long", day: "numeric" }),
      seen:
        p.last_seen_ms === null
          ? t("settings.browser.paired.neverSeen")
          : relativeTime(t, p.last_seen_ms, now),
    });
  return (
    <SettingsSection title={t("settings.browser.paired.title")}>
      <div>
        {paired.length === 0 && (
          <p className="text-[12px] text-fg-muted">{t("settings.browser.paired.none")}</p>
        )}
        {paired.map((p) => (
          <TileRow
            key={p.id}
            icon="puzzle"
            title={pairedName(p.browser, p.label)}
            subtitle={
              <>
                {(p.paused || p.connected) && (
                  <>
                    <Lamp tone={p.paused ? "warn" : "ok"} size={6} />
                    <span className="shrink-0">
                      {t(
                        p.paused
                          ? "settings.browser.paired.paused"
                          : "settings.browser.paired.connected",
                      )}
                    </span>
                    <span aria-hidden>·</span>
                  </>
                )}
                <span className="truncate">{meta(p)}</span>
                {p.version !== null && (
                  <>
                    <span aria-hidden>·</span>
                    <span className="shrink-0">
                      {t("settings.browser.paired.version", { version: p.version })}
                    </span>
                    {olderThan(p.version, appVersion) && (
                      <span className="shrink-0 text-warning-text">
                        {t("settings.browser.paired.outdated")}
                      </span>
                    )}
                  </>
                )}
              </>
            }
            actions={
              <Button variant="text-danger" size="sm" onClick={() => setUnpairing(p)}>
                {t("settings.browser.paired.unpair")}
              </Button>
            }
            note={
              p.page_access === false || olderThan(p.version, appVersion) ? (
                <>
                  {p.page_access === false && (
                    <span className="flex items-center gap-1.5 text-warning-text">
                      <Icon name="alert" size={13} />
                      {t("settings.browser.paired.noPageAccess")}
                    </span>
                  )}
                  {olderThan(p.version, appVersion) && (
                    <span className="flex items-center gap-1.5 text-warning-text">
                      <Icon name="alert" size={13} />
                      {t("settings.browser.paired.outdatedNote", { app: appVersion })}
                    </span>
                  )}
                </>
              ) : undefined
            }
            data-testid={`paired-${p.id}`}
          />
        ))}
      </div>
      <Dialog
        open={unpairing !== null}
        title={t("settings.browser.paired.unpairTitle", {
          name: unpairing === null ? "" : pairedName(unpairing.browser, unpairing.label),
        })}
        onClose={() => setUnpairing(null)}
        actions={
          <>
            <Button variant="ghost" onClick={() => setUnpairing(null)}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                if (unpairing === null) return;
                setUnpairing(null);
                void dispatch({ command: "browser_unpair", id: unpairing.id });
              }}>
              {t("settings.browser.paired.unpair")}
            </Button>
          </>
        }>
        <p>{t("settings.browser.paired.unpairBody")}</p>
      </Dialog>
    </SettingsSection>
  );
}

function PausedBanner({ paired }: { paired: PairedView }) {
  const t = useT();
  const dispatch = useDispatch();
  return (
    <Banner
      tone="warn"
      marker="bar"
      title={t("settings.browser.pausedTitle", { name: pairedName(paired.browser, paired.label) })}
      actions={
        <Button
          size="sm"
          variant="primary"
          onClick={() => void dispatch({ command: "browser_resume" })}>
          {t("settings.browser.resume")}
        </Button>
      }
      data-testid="browser-paused">
      {t("settings.browser.pausedBody")}
    </Banner>
  );
}

type PolicySwitch = "fill_otp" | "copy_otp" | "auto_submit" | "confirm_fill";
type PolicyWords =
  | "fillOtp"
  | "fillOtpHelp"
  | "copyOtp"
  | "copyOtpHelp"
  | "autoSubmit"
  | "autoSubmitHelp"
  | "confirmFill"
  | "confirmFillHelp"
  | "frames"
  | "framesHelp"
  | "onPageLoad"
  | "onPageLoadHelp";

/** The six choices of what the paired extensions may do. A change that lets them do more asks for
 *  proof first (the core would refuse it without); filling as pages load shows its risks before
 *  that. */
function Policy({ policy }: { policy: BrowserPolicy }) {
  const t = useT();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const verify = useVerify();
  const [risks, setRisks] = useState(false);
  const set = async (next: BrowserPolicy, action: VerifyAction, name: string) => {
    if (!riskierPolicy(policy, next)) {
      await dispatch({ command: "browser_set_policy", policy: next });
      return;
    }
    await verify(action, name, (proof) =>
      backend.dispatch({ command: "browser_set_policy", policy: next, ...proof }),
    );
  };
  const p = (key: PolicyWords) => t(`settings.browser.policy.${key}`);
  const row = (key: PolicySwitch, words: "fillOtp" | "copyOtp" | "autoSubmit" | "confirmFill") => (
    <StatusRow label={p(words)} help={p(`${words}Help`)}>
      <Toggle
        checked={policy[key]}
        onChange={(on) => void set({ ...policy, [key]: on }, on ? "open" : "change", p(words))}
        ariaLabel={p(words)}
      />
    </StatusRow>
  );
  return (
    <SettingsSection title={t("settings.browser.policy.title")}>
      <SettingsRows>
        {row("fill_otp", "fillOtp")}
        {row("copy_otp", "copyOtp")}
        {row("auto_submit", "autoSubmit")}
        {row("confirm_fill", "confirmFill")}
        <StatusRow label={p("frames")} help={p("framesHelp")}>
          <Segmented
            size="sm"
            label={p("frames")}
            value={policy.cross_origin_frames}
            onChange={(cross_origin_frames) =>
              void set({ ...policy, cross_origin_frames }, "change", p("frames"))
            }
            options={[
              { value: "never", label: t("settings.browser.policy.frame.never") },
              { value: "ask", label: t("settings.browser.policy.frame.ask") },
            ]}
          />
        </StatusRow>
        <StatusRow label={p("onPageLoad")} help={p("onPageLoadHelp")}>
          <Toggle
            checked={policy.on_page_load}
            onChange={(on) =>
              on
                ? setRisks(true)
                : void dispatch({
                    command: "browser_set_policy",
                    policy: { ...policy, on_page_load: false },
                  })
            }
            ariaLabel={p("onPageLoad")}
          />
        </StatusRow>
      </SettingsRows>
      <PageLoadRisks
        open={risks}
        onClose={() => setRisks(false)}
        onContinue={() => {
          setRisks(false);
          void set({ ...policy, on_page_load: true }, "open", p("onPageLoad"));
        }}
      />
    </SettingsSection>
  );
}

const CONDITIONS = ["top", "https", "single", "level", "list", "visible"] as const;

/** Board 16 d1: what filling as pages load risks, and when it fills; then the proof (d2). */
function PageLoadRisks({
  open,
  onClose,
  onContinue,
}: {
  open: boolean;
  onClose: () => void;
  onContinue: () => void;
}) {
  const t = useT();
  return (
    <Dialog
      open={open}
      title={t("settings.browser.pageLoad.title")}
      onClose={onClose}
      width={480}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={onContinue} data-autofocus>
            {t("settings.browser.pageLoad.continue")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3">
        <Banner tone="warn" marker="icon" title={t("settings.browser.pageLoad.risk")}>
          {t("settings.browser.pageLoad.riskBody")}
        </Banner>
        <p>{t("settings.browser.pageLoad.intro")}</p>
        <ul className="flex list-disc flex-col gap-1 pl-5">
          {CONDITIONS.map((c) => (
            <li key={c}>{t(`settings.browser.pageLoad.conditions.${c}`)}</li>
          ))}
        </ul>
        <p>{t("settings.browser.pageLoad.never")}</p>
      </div>
    </Dialog>
  );
}

type ListKind = "never_save" | "never_fill";

function SiteLists({ rules }: { rules: readonly SiteRuleView[] }) {
  const t = useT();
  const dispatch = useDispatch();
  const [adding, setAdding] = useState<ListKind | null>(null);
  const list = (kind: ListKind, icon: "bellOff" | "ban") => {
    const own = rules.filter((r) => r.kind === kind);
    return (
      <Card padding="sm" data-testid={`sites-${kind}`} className="flex min-w-0 flex-col gap-1">
        <div className="flex items-center gap-2 px-1 pb-1 text-[13px] font-medium text-fg">
          <Icon name={icon} size={14} className="text-fg-muted" />
          {t(`settings.browser.sites.${kind}`)}
        </div>
        {own.length === 0 && (
          <p className="px-1 py-2 text-[12px] text-fg-subtle">
            {t("settings.browser.sites.empty")}
          </p>
        )}
        {own.map((rule) => {
          const site = rule.domains.join(", ");
          return (
            <div key={rule.id} className="flex h-9 min-w-0 items-center gap-2 px-1">
              <span className="min-w-0 flex-1 truncate text-[13px] text-fg" title={site}>
                {site}
              </span>
              <IconButton
                icon="x"
                size={24}
                label={t("settings.browser.sites.remove", { site })}
                onClick={() => void dispatch({ command: "site_rule_remove", id: rule.id })}
              />
            </div>
          );
        })}
        <Button
          variant="text"
          size="sm"
          className="self-start px-1"
          onClick={() => setAdding(kind)}>
          {t("settings.browser.sites.add")}
        </Button>
      </Card>
    );
  };
  return (
    <SettingsSection title={t("settings.browser.sites.title")}>
      <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3.5">
        {list("never_save", "bellOff")}
        {list("never_fill", "ban")}
      </div>
      {adding !== null && <AddSite kind={adding} onClose={() => setAdding(null)} />}
    </SettingsSection>
  );
}

/** Board 16 e: a site for one of the two lists; the core cleans it to its domain. */
function AddSite({ kind, onClose }: { kind: ListKind; onClose: () => void }) {
  const t = useT();
  const { backend } = useBackend();
  const submit = useSubmit();
  const formId = useId();
  const [site, setSite] = useState("");
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const domain = site.trim();
    if (domain === "") return;
    const done = await submit.run(() =>
      backend.dispatch({ command: "site_rule_add", kind, domains: [domain] }),
    );
    if (done !== undefined) onClose();
  };
  return (
    <Dialog
      open
      title={t(`settings.browser.sites.addTitle.${kind}`)}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            type="submit"
            form={formId}
            loading={submit.busy}
            disabled={site.trim() === ""}>
            {t("settings.browser.sites.confirm")}
          </Button>
        </>
      }>
      <form id={formId} onSubmit={(e) => void onSubmit(e)}>
        <Input
          label={t("settings.browser.sites.field")}
          value={site}
          onChange={(e) => setSite(e.target.value)}
          placeholder="bank.example.cn"
          help={t("settings.browser.sites.hint")}
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
          autoComplete="off"
          spellCheck={false}
          data-autofocus
        />
      </form>
    </Dialog>
  );
}

function Activity({ activity }: { activity: readonly ActivityView[] }) {
  const t = useT();
  const locale = useLocale();
  const dispatch = useDispatch();
  const event = (row: ActivityView) => {
    const title = row.title ?? row.site;
    const text =
      row.event === "refused"
        ? t("settings.browser.activity.events.refused", {
            reason: t(`extension.refusal.${row.refusal ?? "invalid"}`),
          })
        : t(`settings.browser.activity.events.${row.event}`, { title });
    if (row.event === "refused")
      return (
        <span className="inline-flex min-w-0 items-center gap-1.5 text-danger">
          <Icon name="ban" size={13} className="shrink-0" />
          <span className="truncate" title={text}>
            {text}
          </span>
        </span>
      );
    if (row.event === "paused")
      return (
        <span className="inline-flex min-w-0 items-center gap-1.5">
          <Icon name="pause" size={14} className="shrink-0 text-warning-text" />
          <span className="truncate">{text}</span>
        </span>
      );
    return (
      <span className="block truncate" title={text}>
        {text}
      </span>
    );
  };
  return (
    <SettingsSection
      title={t("settings.browser.activity.title")}
      aside={
        <>
          <span className="text-fg-subtle">{t("settings.browser.activity.note")}</span>
          <Button
            variant="text"
            size="sm"
            disabled={activity.length === 0}
            onClick={() => void dispatch({ command: "browser_activity_clear" })}>
            {t("settings.browser.activity.clear")}
          </Button>
        </>
      }>
      <div data-testid="browser-activity">
        <Table
          dense
          label={t("settings.browser.activity.title")}
          rows={activity}
          rowKey={(row) =>
            `${row.at_ms}-${row.browser}-${row.site}-${row.event}-${activity.indexOf(row)}`
          }
          empty={
            <p className="py-3 text-[12px] text-fg-muted">{t("settings.browser.activity.empty")}</p>
          }
          columns={[
            {
              id: "time",
              header: t("settings.browser.activity.time"),
              fit: true,
              cell: (row) => ({
                type: "mono",
                text: formatDateTime(locale, row.at_ms, { hour: "2-digit", minute: "2-digit" }),
              }),
            },
            {
              id: "browser",
              header: t("settings.browser.activity.browser"),
              width: 84,
              cell: (row) => ({ type: "text", text: browserName(row.browser) }),
            },
            {
              id: "site",
              header: t("settings.browser.activity.site"),
              width: 210,
              cell: (row) => ({ type: "text", text: row.site, muted: true }),
            },
            {
              id: "event",
              header: t("settings.browser.activity.event"),
              minWidth: 120,
              cell: (row) => event(row),
            },
          ]}
        />
      </div>
    </SettingsSection>
  );
}
