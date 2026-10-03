// What an import found, before anything reaches the vault: each account with where it came from,
// its parameters and status, and what to do with it; then import, or leave (the discard button or
// the back gesture), which discards the import (App.tsx). Once imported, the codes show.
import {
  type CandidateAction,
  type CandidateStatus,
  type CandidateView,
  type ImportView,
  actionsFor,
  candidateSource,
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
  Segmented,
  useDispatch,
  useT,
} from "@lockra/ui";
import { useState } from "react";
import { useNav } from "../app/nav";
import { Page } from "../components/Page";

const STATUS_TONE: Record<CandidateStatus["type"], BadgeTone> = {
  new: "info",
  exists: "neutral",
  conflict: "warn",
  duplicate: "neutral",
  unsupported: "danger",
};

export function Preview({ view }: { view: ImportView }) {
  const t = useT();
  const nav = useNav();
  const dispatch = useDispatch();
  const [chosen, setChosen] = useState<ReadonlyMap<number, CandidateAction>>(new Map());
  const choices = importChoices(view, chosen);
  const taken = takenCount(choices);
  const choose = (id: number, action: CandidateAction) =>
    setChosen((current) => new Map(current).set(id, action));
  const commit = () => void dispatch({ command: "import_commit", choices });
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
        <ul className="flex flex-col gap-2" aria-label={t("import.preview.title")}>
          {view.candidates.map((candidate) => (
            <Candidate
              key={candidate.id}
              candidate={candidate}
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
          {taken === 0 ? t("import.preview.nothing") : t("import.preview.commit", { n: taken })}
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
  action,
  onChoose,
}: {
  candidate: CandidateView;
  action: CandidateAction;
  onChoose: (action: CandidateAction) => void;
}) {
  const t = useT();
  const actions = actionsFor(candidate.status);
  const name = candidate.issuer || candidate.account || "—";
  const facts = [
    candidateSource(t, candidate),
    candidate.kind && candidate.algorithm && candidate.digits
      ? parametersText(t, candidate.kind, candidate.algorithm, candidate.digits)
      : undefined,
  ].filter((fact) => fact !== undefined);
  return (
    <li data-testid="candidate">
      <Card padding="none" className="flex flex-col gap-2 p-3">
        <div className="flex items-start gap-2">
          <div className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[15px] text-fg">{name}</span>
            {candidate.issuer !== "" && candidate.account !== "" && (
              <span className="truncate text-[13px] text-fg-muted">{candidate.account}</span>
            )}
          </div>
          <Badge tone={STATUS_TONE[candidate.status.type]}>
            {t(`import.status.${candidate.status.type}`)}
          </Badge>
        </div>
        <p className="mono text-[12px] text-fg-subtle">{facts.join(" · ")}</p>
        {candidate.status.type === "unsupported" && (
          <p className="text-[13px] text-fg-muted">{rejectText(t, candidate.status.reason)}</p>
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
