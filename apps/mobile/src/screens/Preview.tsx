// What an import found, before anything reaches the vault: each account or login with where it
// came from, its parameters (a login's site) and status, and what to do with it; then import, or
// leave (the discard button or the back gesture), which discards the import (App.tsx). Once
// imported, the codes show.
import {
  type AwaitingView,
  type CandidateAction,
  type CandidateStatus,
  type CandidateView,
  type ImportView,
  actionsFor,
  candidateSource,
  conflictText,
  errorText,
  importChoices,
  parametersText,
  rejectText,
  takenCount,
} from "@lockra/shared";
import {
  Badge,
  type BadgeTone,
  Banner,
  Button,
  Card,
  PasswordField,
  Segmented,
  useBackend,
  useDispatch,
  useSubmit,
  useT,
} from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { usePhoneImport } from "../app/phone-import";
import { overPhoneScreen } from "../app/phone-screen";
import { Page } from "../components/Page";

const STATUS_TONE: Record<CandidateStatus["type"], BadgeTone> = {
  new: "info",
  exists: "neutral",
  conflict: "warn",
  duplicate: "neutral",
  unsupported: "danger",
  login_exists: "neutral",
  login_conflict: "warn",
};

export function Preview({ view }: { view: ImportView }) {
  const t = useT();
  const nav = useNav();
  const dispatch = useDispatch();
  const { scan } = usePhoneImport();
  const [chosen, setChosen] = useState<ReadonlyMap<number, CandidateAction>>(new Map());
  const choices = importChoices(view, chosen);
  const taken = takenCount(choices);
  const choose = (id: number, action: CandidateAction) =>
    setChosen((current) => new Map(current).set(id, action));
  const commit = () => void dispatch({ command: "import_commit", choices });
  const logins = view.candidates.some((c) => c.login !== null);
  return (
    <Page title={t("import.preview.title")} testId="page-preview">
      <div className="flex flex-col gap-3">
        {view.google_batches.map((batch) => (
          <Banner key={batch.id} tone={batch.missing.length > 0 ? "warn" : "info"} marker="bar">
            {t("import.preview.googleBatch", { received: batch.received.length, size: batch.size })}
            {batch.missing.length > 0 &&
              ` · ${t("import.preview.googleMissing", { missing: batch.missing.map((i) => i + 1).join(", ") })}`}
          </Banner>
        ))}
        {view.awaiting?.kind === "backup" && <BackupPassword name={view.awaiting.name} />}
        {view.awaiting?.kind === "kdbx" && <KdbxPassword awaiting={view.awaiting} />}
        {/* A Google export of several codes: the rest join this preview as they are scanned. */}
        {view.google_batches.some((batch) => batch.missing.length > 0) && (
          <Button size="lg" icon="scan" onClick={() => void scan()} data-testid="preview-scan">
            {t("mobile.scan.next")}
          </Button>
        )}
        <ul className="flex flex-col gap-2" aria-label={t("import.preview.title")}>
          {view.candidates.map((candidate) => (
            <Candidate
              key={candidate.id}
              candidate={candidate}
              typed={logins}
              action={chosen.get(candidate.id) ?? candidate.default_action}
              onChoose={(action) => choose(candidate.id, action)}
            />
          ))}
        </ul>
        <Button
          variant="primary"
          size="lg"
          icon="download"
          disabled={taken === 0}
          onClick={commit}
          data-testid="import-commit">
          {taken === 0
            ? t("import.preview.nothing")
            : logins
              ? t("import.preview.commitItems", { n: taken })
              : t("import.preview.commit", { n: taken })}
        </Button>
        <Button variant="ghost" size="lg" onClick={nav.back}>
          {t("import.preview.cancel")}
        </Button>
      </div>
    </Page>
  );
}

function Candidate({
  candidate,
  typed,
  action,
  onChoose,
}: {
  candidate: CandidateView;
  /** The import holds logins too: each row says whether it is a login or a code. */
  typed: boolean;
  action: CandidateAction;
  onChoose: (action: CandidateAction) => void;
}) {
  const t = useT();
  const actions = actionsFor(candidate.status);
  const login = candidate.login;
  const name =
    login === null
      ? candidate.issuer || candidate.account || "—"
      : login.title || login.host || login.username || "—";
  const detail =
    login === null ? (candidate.issuer !== "" ? candidate.account : "") : login.username;
  const facts = [
    typed ? t(login === null ? "import.preview.typeCode" : "import.preview.typeLogin") : undefined,
    candidateSource(t, candidate),
    login !== null
      ? [login.host || undefined, login.totp === null ? undefined : t("import.preview.withCode")]
          .filter((part) => part !== undefined)
          .join(" · ") || undefined
      : candidate.kind && candidate.algorithm && candidate.digits
        ? parametersText(t, candidate.kind, candidate.algorithm, candidate.digits)
        : undefined,
  ].filter((fact) => fact !== undefined);
  return (
    <li data-testid="candidate">
      <Card padding="none" className="flex flex-col gap-2 p-3">
        <div className="flex items-start gap-2">
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[15px] text-fg">{name}</span>
            {detail !== "" && <span className="truncate text-[13px] text-fg-muted">{detail}</span>}
          </div>
          <Badge tone={STATUS_TONE[candidate.status.type]}>
            {t(`import.status.${candidate.status.type}`)}
          </Badge>
        </div>
        <p className="mono text-[12px] text-fg-subtle">{facts.join(" · ")}</p>
        {candidate.status.type === "unsupported" && (
          <p className="text-[13px] text-fg-muted">{rejectText(t, candidate.status.reason)}</p>
        )}
        {candidate.status.type === "login_conflict" && (
          <p className="text-[13px] text-fg-muted">{conflictText(t, candidate.status.reason)}</p>
        )}
        {actions.length > 0 && (
          <Segmented
            size="lg"
            label={`${t("import.preview.action")} · ${name}`}
            value={action}
            onChange={onChoose}
            options={actions.map((value) => ({ value, label: t(`import.action.${value}`) }))}
            className="self-start"
          />
        )}
      </Card>
    </li>
  );
}

/** A Lockra backup in the import waits for its password; opened, its accounts join the preview. */
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
    <Card padding="none" className="p-4">
      <form
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-3"
        data-testid="backup-password">
        <PasswordField
          size="lg"
          label={t("import.preview.awaiting", { name })}
          value={password}
          onChange={setPassword}
          autoComplete="off"
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
        <Button
          variant="primary"
          size="lg"
          type="submit"
          loading={submit.busy}
          disabled={password === ""}>
          {t("import.preview.awaitingSubmit")}
        </Button>
      </form>
    </Card>
  );
}

/** A KeePass database in the import waits for its password, and its key file if it takes one:
 *  the system's file picker hands the file to Rust; only its name comes back. */
function KdbxPassword({ awaiting }: { awaiting: AwaitingView }) {
  const t = useT();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const [password, setPassword] = useState("");
  const submit = useSubmit();
  const pick = useSubmit();
  // A database a key file alone protects opens with the password left empty.
  const ready = password !== "" || awaiting.key_file !== null;
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!ready) return;
    await submit.run(() => backend.dispatch({ command: "import_backup_password", password }));
    setPassword("");
  };
  return (
    <Card padding="none" className="p-4">
      <form
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-3"
        data-testid="kdbx-password">
        <PasswordField
          size="lg"
          label={t("import.preview.awaitingKdbx", { name: awaiting.name })}
          value={password}
          onChange={setPassword}
          autoComplete="off"
          error={submit.error === undefined ? undefined : errorText(t, submit.error)}
        />
        {awaiting.key_file === null ? (
          <p className="text-[13px] text-fg-muted">{t("import.preview.keyFileHint")}</p>
        ) : (
          <p className="truncate text-[13px] text-fg-muted" data-testid="kdbx-key-file">
            {t("import.preview.keyFileChosen", { name: awaiting.key_file })}
          </p>
        )}
        <Button
          size="lg"
          icon="key"
          loading={pick.busy}
          onClick={() => void pick.run(() => overPhoneScreen(() => backend.pickKeyFile()))}>
          {t("import.preview.keyFile")}
        </Button>
        {awaiting.key_file !== null && (
          <Button
            variant="ghost"
            size="lg"
            onClick={() => void dispatch({ command: "import_key_file_clear" })}>
            {t("import.preview.keyFileClear")}
          </Button>
        )}
        {pick.error !== undefined && (
          <p role="alert" className="text-[13px] text-danger">
            {errorText(t, pick.error)}
          </p>
        )}
        <Button variant="primary" size="lg" type="submit" loading={submit.busy} disabled={!ready}>
          {t("import.preview.awaitingKdbxSubmit")}
        </Button>
      </form>
    </Card>
  );
}
