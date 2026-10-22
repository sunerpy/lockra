// The generator page (sidebar › 生成器, Ctrl G, 「添加登录项 › 生成密码…」): the generator, making
// passwords for no site, with 「复制」 as its main action, then its history: how much it holds and
// how long, and the passwords themselves inline after the proof, hidden after two minutes or as
// soon as the page is left. A login's form keeps its own generator dialog, which hands the password
// to the form (features/items/GeneratorDialog.tsx); both draw the same panels.
import { GENERATOR_HISTORY_DAYS, GENERATOR_HISTORY_MAX } from "@lockra/shared";
import { Button, PageHeader, Panel, useT, useUiState } from "@lockra/ui";
import {
  ClearHistoryButton,
  GeneratorHistoryList,
  HistoryCountdown,
  useGeneratorHistory,
} from "../features/items/GeneratorHistory";
import { GeneratorPanel, useGenerator } from "../features/items/GeneratorPanel";

export function Generator() {
  const t = useT();
  const generator = useGenerator();
  return (
    <div className="flex flex-col gap-4" data-testid="page-generator">
      <PageHeader title={t("generator.page.title")} subtitle={t("generator.page.subtitle")} />
      <Panel eyebrow={t("generator.title")} data-testid="generator-panel">
        <GeneratorPanel generator={generator} copy="button" />
      </Panel>
      <HistoryPanel />
    </div>
  );
}

/** 生成历史: what it holds and keeps, then the passwords themselves after the proof. */
function HistoryPanel() {
  const t = useT();
  const { generator: kept } = useUiState();
  const history = useGeneratorHistory();
  const { shown } = history;
  return (
    <Panel
      eyebrow={t("generator.history.title")}
      right={
        shown === undefined ? (
          <span className="mono text-fg-subtle" data-testid="generator-history-count">
            {t("generator.history.count", { n: kept.history_count })}
          </span>
        ) : (
          <>
            <HistoryCountdown at={shown.at} className="text-fg-subtle" />
            <Button
              variant="text-muted"
              size="sm"
              onClick={history.hide}
              data-testid="generator-history-hide">
              {t("generator.history.hide")}
            </Button>
            <ClearHistoryButton history={history} size="sm" />
          </>
        )
      }
      data-testid="generator-history-panel">
      <div className="flex flex-col gap-3">
        <p
          className="text-[12px] leading-[18px] text-fg-muted"
          data-testid="generator-history-kept">
          {t("generator.history.kept", {
            max: GENERATOR_HISTORY_MAX,
            days: GENERATOR_HISTORY_DAYS,
          })}
        </p>
        {shown === undefined ? (
          <Button
            icon="history"
            className="self-start"
            disabled={kept.history_count === 0}
            onClick={() => void history.open()}
            data-testid="generator-history-view">
            {t("generator.history.view")}
          </Button>
        ) : (
          <GeneratorHistoryList list={shown.list} />
        )}
      </div>
    </Panel>
  );
}
