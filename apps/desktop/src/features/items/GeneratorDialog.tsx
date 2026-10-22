// The generator over a login's form (boards 09, 10): 「使用此密码」 puts its password in the form (the
// login then saves it as the generator's), the icon beside 重新生成 copies it, and 生成历史 · n 条 at
// the bottom left opens the history in a view of its own after the proof. The generator page
// (pages/Generator.tsx) shows the same panels on its own.
import { Button, Dialog, useT, useUiState } from "@lockra/ui";
import {
  ClearHistoryButton,
  ClearHistoryDialog,
  GeneratorHistoryList,
  HistoryCountdown,
  useGeneratorHistory,
} from "./GeneratorHistory";
import { GeneratorPanel, useGenerator } from "./GeneratorPanel";

export interface GeneratorDialogProps {
  onClose: () => void;
  /** 「使用此密码」: the password goes to the login's form. */
  onUse: (password: string) => void;
  /** The site it is made for: its rules apply when Lockra knows them. */
  domain?: string;
}

export function GeneratorDialog({ onClose, onUse, domain }: GeneratorDialogProps) {
  const t = useT();
  const { generator: kept } = useUiState();
  const generator = useGenerator(domain);
  const history = useGeneratorHistory();
  const { made } = generator;
  const use = () => {
    if (made === undefined) return;
    generator.keepOptions();
    onUse(made.password);
  };
  if (history.shown !== undefined) {
    return (
      <>
        <Dialog
          open
          title={t("generator.history.title")}
          onClose={history.hide}
          width={520}
          hint={<HistoryCountdown at={history.shown.at} />}
          actions={
            <>
              <ClearHistoryButton history={history} />
              <Button variant="primary" onClick={history.hide}>
                {t("common.done")}
              </Button>
            </>
          }>
          <GeneratorHistoryList list={history.shown.list} />
        </Dialog>
        {/* Over the history, as a dialog of its own. */}
        <ClearHistoryDialog history={history} />
      </>
    );
  }
  return (
    <Dialog
      open
      title={t("generator.title")}
      onClose={onClose}
      width={520}
      hint={
        <Button
          variant="text"
          size="sm"
          icon="history"
          className="font-sans"
          onClick={() => void history.open()}
          data-testid="generator-history-open">
          {t("generator.history.open", { n: kept.history_count })}
        </Button>
      }
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={made === undefined}
            onClick={use}
            data-testid="generator-use">
            {t("generator.use")}
          </Button>
        </>
      }>
      <div data-testid="generator-dialog">
        <GeneratorPanel generator={generator} copy="icon" />
      </div>
    </Dialog>
  );
}
