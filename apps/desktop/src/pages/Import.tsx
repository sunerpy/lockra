// Bring accounts in: the four sources (Google's migration QR codes, Microsoft's PhoneFactor
// database, otpauth links, a Lockra backup) and the preview of what was found, which nothing
// reaches the vault before the user confirms.
import {
  type CandidateAction,
  type CandidateStatus,
  type CandidateView,
  type ImportView,
  errorText,
  parametersText,
  rejectText,
} from "@lockra/shared";
import {
  Badge,
  type BadgeTone,
  Banner,
  Button,
  CardGrid,
  DropZone,
  OptionCard,
  Panel,
  PasswordField,
  Select,
  StepList,
  Table,
  type TableColumn,
  Textarea,
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useDispatch, useGuarded, useSubmit } from "../app/dispatch";

const STATUS_TONE: Record<CandidateStatus["type"], BadgeTone> = {
  new: "info",
  exists: "neutral",
  conflict: "warn",
  duplicate: "neutral",
  unsupported: "danger",
};

/** What the user may do with a found account: a new one is added or skipped; one that shares a
 *  name with an account of another secret can also replace it; the rest are only skipped. */
export function actionsFor(status: CandidateStatus): readonly CandidateAction[] {
  if (status.type === "new") return ["add", "skip"];
  if (status.type === "conflict") return ["add", "replace", "skip"];
  return [];
}

export function Import({ dragging }: { dragging: boolean }) {
  const t = useT();
  const state = useUiState();
  const dispatch = useDispatch();
  return (
    <div className="flex flex-col gap-4" data-testid="page-import">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <p className="text-[13px] text-fg-muted">{t("import.subtitle")}</p>
        <Button icon="clipboard" onClick={() => void dispatch({ command: "import_clipboard" })}>
          {t("import.clipboard")}
        </Button>
      </div>
      {state.import !== null && <Preview view={state.import} />}
      <CardGrid min={320}>
        <GoogleSource dragging={dragging} />
        <MicrosoftSource />
        <TextSource />
        <BackupSource />
      </CardGrid>
    </div>
  );
}

function usePick() {
  const { backend } = useBackend();
  const guarded = useGuarded();
  return (kind: "images" | "text" | "backup" | "any") =>
    void guarded(() => backend.pickImportFiles(kind));
}

function GoogleSource({ dragging }: { dragging: boolean }) {
  const t = useT();
  const pick = usePick();
  return (
    <div data-testid="source-google" className="contents">
      <OptionCard icon="qr" title={t("import.google.title")}>
        <div className="flex flex-col gap-3">
          <StepList
            steps={[
              t("import.google.steps.one"),
              t("import.google.steps.two"),
              t("import.google.steps.three"),
            ]}
          />
          <DropZone
            active={dragging}
            icon="image"
            title={t("import.google.pick")}
            hint={t("import.dropHint")}
            onActivate={() => pick("images")}
          />
        </div>
      </OptionCard>
    </div>
  );
}

function MicrosoftSource() {
  const t = useT();
  const pick = usePick();
  return (
    <div data-testid="source-microsoft" className="contents">
      <OptionCard icon="phone" title={t("import.microsoft.title")}>
        <div className="flex flex-col items-start gap-3">
          {/* The database path is one unbroken word: it may break anywhere rather than overflow. */}
          <p className="text-[12px] leading-[18px] [overflow-wrap:anywhere] text-fg-muted">
            {t("import.microsoft.body")}
          </p>
          <Button icon="folder" onClick={() => pick("any")}>
            {t("import.microsoft.pick")}
          </Button>
        </div>
      </OptionCard>
    </div>
  );
}

function TextSource() {
  const t = useT();
  const pick = usePick();
  const dispatch = useDispatch();
  const [text, setText] = useState("");
  const read = async () => {
    // The links are secrets: they leave React state as soon as the core has them.
    if ((await dispatch({ command: "import_text", text })) !== undefined) setText("");
  };
  return (
    <div data-testid="source-text" className="contents">
      <OptionCard icon="link" title={t("import.text.title")}>
        <div className="flex flex-col gap-3">
          <p className="text-[12px] leading-[18px] text-fg-muted">{t("import.text.body")}</p>
          <Textarea
            mono
            rows={4}
            value={text}
            onChange={(e) => setText(e.target.value)}
            placeholder={t("import.text.placeholder")}
            aria-label={t("import.text.title")}
            spellCheck={false}
          />
          <div className="flex items-center gap-2">
            <Button variant="primary" disabled={text.trim() === ""} onClick={() => void read()}>
              {t("import.text.read")}
            </Button>
            <Button icon="fileText" onClick={() => pick("text")}>
              {t("import.text.pick")}
            </Button>
          </div>
        </div>
      </OptionCard>
    </div>
  );
}

function BackupSource() {
  const t = useT();
  const pick = usePick();
  return (
    <div data-testid="source-backup" className="contents">
      <OptionCard icon="archive" title={t("import.backup.title")}>
        <div className="flex flex-col items-start gap-3">
          <p className="text-[12px] leading-[18px] text-fg-muted">{t("import.backup.body")}</p>
          <Button icon="archive" onClick={() => pick("backup")}>
            {t("import.backup.pick")}
          </Button>
        </div>
      </OptionCard>
    </div>
  );
}

function sourceText(t: ReturnType<typeof useT>, candidate: CandidateView): string {
  const source =
    candidate.source.type === "file"
      ? candidate.source.name
      : candidate.source.type === "clipboard"
        ? t("import.preview.sourceClipboard")
        : t("import.preview.sourceText");
  return candidate.line === null
    ? source
    : `${source} · ${t("import.preview.line", { n: candidate.line })}`;
}

function Preview({ view }: { view: ImportView }) {
  const t = useT();
  const dispatch = useDispatch();
  const [choices, setChoices] = useState<ReadonlyMap<number, CandidateAction>>(new Map());
  const actionOf = (c: CandidateView): CandidateAction => choices.get(c.id) ?? c.default_action;
  const taken = view.candidates.filter(
    (c) => actionsFor(c.status).length > 0 && actionOf(c) !== "skip",
  ).length;
  const choose = (id: number, action: CandidateAction) =>
    setChoices((current) => new Map(current).set(id, action));
  const commit = () => {
    const list = view.candidates
      .filter((c) => actionsFor(c.status).length > 0)
      .map((c) => ({ id: c.id, action: actionOf(c) }));
    void dispatch({ command: "import_commit", choices: list });
  };
  const columns: TableColumn<CandidateView>[] = [
    {
      id: "account",
      header: t("import.preview.account"),
      minWidth: 160,
      cell: (c) => ({
        type: "two",
        primary: c.issuer || c.account || "—",
        secondary: c.issuer ? c.account : "",
      }),
    },
    {
      id: "source",
      header: t("import.preview.source"),
      minWidth: 120,
      cell: (c) => ({ type: "text", text: sourceText(t, c), muted: true }),
    },
    {
      id: "parameters",
      header: t("import.preview.parameters"),
      fit: true,
      cell: (c) => ({
        type: "mono",
        text:
          c.kind && c.algorithm && c.digits
            ? parametersText(t, c.kind, c.algorithm, c.digits)
            : "—",
        muted: true,
      }),
    },
    {
      id: "status",
      header: t("import.preview.status"),
      minWidth: 120,
      cell: (c) => (
        <span className="flex min-w-0 items-center gap-2">
          <Badge tone={STATUS_TONE[c.status.type]}>{t(`import.status.${c.status.type}`)}</Badge>
          {c.status.type === "unsupported" && (
            <span
              className="truncate text-[12px] text-fg-muted"
              title={rejectText(t, c.status.reason)}>
              {rejectText(t, c.status.reason)}
            </span>
          )}
        </span>
      ),
    },
    {
      id: "action",
      header: t("import.preview.action"),
      width: 136,
      cell: (c) => {
        const actions = actionsFor(c.status);
        if (actions.length === 0) return <span className="text-fg-subtle">—</span>;
        return (
          <Select
            size="sm"
            aria-label={`${t("import.preview.action")} · ${c.issuer || c.account}`}
            value={actionOf(c)}
            onChange={(action) => choose(c.id, action)}
            options={actions.map((value) => ({ value, label: t(`import.action.${value}`) }))}
          />
        );
      },
    },
  ];
  return (
    <Panel
      eyebrow={t("import.preview.title")}
      right={
        <span className="mono text-fg-subtle">
          {t("common.accounts", { n: view.candidates.length })}
        </span>
      }
      data-testid="import-preview">
      <div className="flex flex-col gap-3">
        {view.google_batches.map((batch) => (
          <Banner key={batch.id} tone={batch.missing.length > 0 ? "warn" : "info"} marker="bar">
            {t("import.preview.googleBatch", { received: batch.received.length, size: batch.size })}
            {batch.missing.length > 0 &&
              ` · ${t("import.preview.googleMissing", { missing: batch.missing.map((i) => i + 1).join(", ") })}`}
          </Banner>
        ))}
        {view.awaiting_password !== null && <BackupPassword name={view.awaiting_password} />}
        {view.candidates.length > 0 && (
          <Table
            columns={columns}
            rows={view.candidates}
            rowKey={(c) => String(c.id)}
            dense
            label={t("import.preview.title")}
          />
        )}
        <div className="flex items-center justify-end gap-2">
          <Button variant="ghost" onClick={() => void dispatch({ command: "import_cancel" })}>
            {t("import.preview.cancel")}
          </Button>
          <Button
            variant="primary"
            icon="download"
            disabled={taken === 0}
            onClick={commit}
            data-testid="import-commit">
            {taken === 0 ? t("import.preview.nothing") : t("import.preview.commit", { n: taken })}
          </Button>
        </div>
      </div>
    </Panel>
  );
}

/** A Lockra backup in the import waits for its password. */
function BackupPassword({ name }: { name: string }) {
  const t = useT();
  const { backend } = useBackend();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (password === "") return;
    await submit.run(() => backend.dispatch({ command: "import_backup_password", password }));
    setPassword("");
  };
  return (
    <form
      onSubmit={(e) => void onSubmit(e)}
      className="flex flex-wrap items-end gap-2 rounded-10 bg-inset p-3"
      data-testid="backup-password">
      <PasswordField
        label={t("import.preview.awaiting", { name })}
        value={password}
        onChange={setPassword}
        autoFocus
        className="min-w-[16rem] flex-1"
        error={submit.error === undefined ? undefined : errorText(t, submit.error)}
      />
      <Button variant="primary" type="submit" loading={submit.busy} disabled={password === ""}>
        {t("import.preview.awaitingSubmit")}
      </Button>
    </form>
  );
}
