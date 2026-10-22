// The generator's history (board 10), shared by the generator page and the dialog of a login's form:
// the passwords copied or saved with a login, and those the browser extension made on sign-up
// forms, kept by the core on this device only. They show after the master password or the system
// check (`generator_history_reveal`, a secret answer) and hide themselves after two minutes; while
// they show, they are a secret view. 「清空历史…」 asks first, as the app's other destructive actions
// do.
import { type GeneratedPastView, REVEAL_SECONDS, relativeTime } from "@lockra/shared";
import { Button, type ButtonSize, Dialog, useBackend, useClock, useNow, useT } from "@lockra/ui";
import { useEffect, useState } from "react";
import { useDispatch } from "../../app/dispatch";
import { useSecretView } from "./secret-view";
import { useVerify } from "./verify";

/** The history as one view of it holds it. */
export interface GeneratorHistory {
  /** The passwords on the screen and when they came, `undefined` while hidden. */
  shown: { list: GeneratedPastView[]; at: number } | undefined;
  /** Ask for the proof, then show them. */
  open: () => Promise<void>;
  hide: () => void;
  /** 「清空历史…」 was chosen: its question is on the screen. */
  clearing: boolean;
  askClear: () => void;
  cancelClear: () => void;
  /** The answer to the question: forget them all; what was shown goes too. */
  clear: () => Promise<void>;
}

export function useGeneratorHistory(): GeneratorHistory {
  const t = useT();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const verify = useVerify();
  const [shown, setShown] = useState<GeneratorHistory["shown"]>(undefined);
  const [clearing, setClearing] = useState(false);
  useSecretView(shown !== undefined);
  // Hidden, the history takes its question with it.
  const hide = () => {
    setShown(undefined);
    setClearing(false);
  };
  // The history hides itself two minutes after it showed.
  useEffect(() => {
    if (shown === undefined) return undefined;
    const timer = setTimeout(
      () => {
        setShown(undefined);
        setClearing(false);
      },
      Math.max(0, shown.at + REVEAL_SECONDS * 1000 - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [shown]);
  return {
    shown,
    open: async () => {
      const list = await verify("view", t("generator.history.title"), (proof) =>
        backend.dispatch({ command: "generator_history_reveal", ...proof }),
      );
      if (list !== undefined) setShown({ list, at: Date.now() });
    },
    hide,
    clearing,
    askClear: () => setClearing(true),
    cancelClear: () => setClearing(false),
    clear: async () => {
      setClearing(false);
      if ((await dispatch({ command: "generator_history_clear" })) !== undefined) hide();
    },
  };
}

/** The seconds before the history shown at `at` hides. */
export function HistoryCountdown({ at, className }: { at: number; className?: string }) {
  const t = useT();
  const now = useClock();
  // The shared clock ticks on whole seconds, so it can read just before the moment they came.
  const left = Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - at) / 1000)));
  return (
    <span className={className} data-testid="generator-history-countdown">
      {t("passwords.detail.hideIn", { s: left })}
    </span>
  );
}

/** The passwords, newest first, each with when it was made and the site it was made for. */
export function GeneratorHistoryList({ list }: { list: readonly GeneratedPastView[] }) {
  const t = useT();
  const now = useNow();
  if (list.length === 0)
    return <p className="text-[13px] text-fg-muted">{t("generator.history.empty")}</p>;
  return (
    <ul className="flex flex-col divide-y divide-border" data-testid="generator-history">
      {list.map((entry) => (
        <li
          key={`${entry.at_ms}:${entry.password}`}
          className="flex min-h-13 flex-col justify-center py-1.5">
          <span className="mono text-[13px] font-medium break-all text-fg select-all">
            {entry.password}
          </span>
          <span className="text-[11.5px] text-fg-subtle">
            {[
              relativeTime(t, entry.at_ms, now),
              entry.site === null ? "" : t("generator.history.site", { site: entry.site }),
            ]
              .filter((s) => s !== "")
              .join(" · ")}
          </span>
        </li>
      ))}
    </ul>
  );
}

/** 「清空历史…」 for the history on the screen, which asks first; none to clear, it is off. */
export function ClearHistoryButton({
  history,
  size,
}: {
  history: GeneratorHistory;
  size?: ButtonSize;
}) {
  const t = useT();
  return (
    <Button
      variant="text-danger"
      size={size}
      onClick={history.askClear}
      disabled={history.shown === undefined || history.shown.list.length === 0}
      data-testid="generator-history-clear">
      {t("generator.history.clear")}
    </Button>
  );
}

/** The question 「清空历史…」 asks: 取消 keeps the history, 清空 forgets it. Drawn beside the view that
 *  asked, never inside a frosted card or dialog, whose blur would hold a fixed dialog inside it. */
export function ClearHistoryDialog({ history }: { history: GeneratorHistory }) {
  const t = useT();
  return (
    <Dialog
      open={history.clearing}
      title={t("generator.history.clearConfirm")}
      onClose={history.cancelClear}
      actions={
        <>
          <Button variant="ghost" onClick={history.cancelClear} data-autofocus>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            icon="trash"
            onClick={() => void history.clear()}
            data-testid="generator-history-clear-confirm">
            {t("generator.history.clearSubmit")}
          </Button>
        </>
      }
    />
  );
}
