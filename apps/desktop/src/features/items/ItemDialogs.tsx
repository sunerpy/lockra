// The dialogs of a login, opened through the shell's overlay: new, edit (with its 「删除…」), delete,
// show the password (board 08), and the generator on its own (Ctrl G, the add menu). A login that
// went away meanwhile (a sync deleted it, a restore replaced the vault) closes its dialog.
import { type ItemView, REVEAL_SECONDS, errorText, itemAccount, itemName } from "@lockra/shared";
import {
  Banner,
  Button,
  CountdownRing,
  Dialog,
  ScoreMeter,
  useBackend,
  useClock,
  useFlash,
  useItems,
  useLocale,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useRef, useState } from "react";
import { motionReduced } from "../../app/appearance";
import { useSubmit } from "../../app/dispatch";
import { useShell } from "../../app/shell-state";
import { GeneratorDialog } from "./GeneratorDialog";
import { ItemFormDialog } from "./ItemForm";
import { useItemActions } from "./actions";
import { useSecretView } from "./secret-view";

/** The overlay's dialog, if it is one of a login's (or the generator). */
export function ItemDialogs() {
  const shell = useShell();
  const items = useItems();
  const { overlay, close, navigate, selectItem } = shell;
  const id =
    overlay?.type === "item_edit" ||
    overlay?.type === "item_delete" ||
    overlay?.type === "item_reveal"
      ? overlay.id
      : undefined;
  const item = id === undefined ? undefined : items?.find((i) => i.id === id);
  const missing = id !== undefined && items !== undefined && item === undefined;
  useEffect(() => {
    if (missing) close();
  }, [missing, close]);
  if (overlay === null) return null;
  switch (overlay.type) {
    case "item_new":
      return (
        <ItemFormDialog
          key="new"
          title={overlay.title}
          onClose={close}
          onSaved={(saved) => {
            navigate("passwords");
            selectItem(saved);
          }}
        />
      );
    case "item_edit":
      return item ? <EditItem key={item.id} item={item} onClose={close} /> : null;
    case "item_delete":
      return item ? (
        <DeleteItemDialog key={item.id} item={item} onClose={close} onDeleted={close} />
      ) : null;
    case "item_reveal":
      return item ? <RevealItemDialog key={item.id} item={item} onClose={close} /> : null;
    case "generator":
      return <GeneratorDialog onClose={close} />;
    default:
      return null;
  }
}

/** The edit form, and its 「删除…」 over it: cancelling the deletion goes back to the edits. */
function EditItem({ item, onClose }: { item: ItemView; onClose: () => void }) {
  const [deleting, setDeleting] = useState(false);
  return (
    <>
      <ItemFormDialog item={item} onClose={onClose} onDelete={() => setDeleting(true)} />
      {deleting && (
        <DeleteItemDialog item={item} onClose={() => setDeleting(false)} onDeleted={onClose} />
      )}
    </>
  );
}

function DeleteItemDialog({
  item,
  onClose,
  onDeleted,
}: {
  item: ItemView;
  onClose: () => void;
  onDeleted: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const submit = useSubmit();
  const remove = async () => {
    const done = await submit.run(() =>
      backend.dispatch({ command: "item_delete", ids: [item.id] }),
    );
    if (done !== undefined) onDeleted();
  };
  return (
    <Dialog
      open
      title={t("passwords.delete.title")}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" onClick={onClose} data-autofocus>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            icon="trash"
            loading={submit.busy}
            onClick={() => void remove()}
            data-testid="item-delete-confirm">
            {t("passwords.delete.confirm")}
          </Button>
        </>
      }>
      <p data-testid="item-delete">{t("passwords.delete.body", { name: itemName(item) })}</p>
      {submit.error !== undefined && (
        <p role="alert" className="mt-2 text-[12px] text-danger">
          {errorText(t, submit.error)}
        </p>
      )}
    </Dialog>
  );
}

const KINDS = ["letters", "lower", "upper", "digits", "symbols"] as const;

/** What a password is made of, in words: "大小写字母、数字和符号". */
function kindsOf(password: string): (typeof KINDS)[number][] {
  const lower = /[a-z]/.test(password);
  const upper = /[A-Z]/.test(password);
  const kinds: (typeof KINDS)[number][] = [];
  if (lower && upper) kinds.push("letters");
  else if (lower) kinds.push("lower");
  else if (upper) kinds.push("upper");
  if (/\d/.test(password)) kinds.push("digits");
  if (/[^A-Za-z0-9]/.test(password)) kinds.push("symbols");
  return kinds;
}

/** A login's password on the screen (board 08), after the proof a 「填充前验证」 login asks for; it
 *  hides itself after two minutes. */
function RevealItemDialog({ item, onClose }: { item: ItemView; onClose: () => void }) {
  const t = useT();
  const locale = useLocale();
  const { backend } = useBackend();
  const { platform, settings } = useUiState();
  const actions = useItemActions();
  const now = useClock();
  const [shown, setShown] = useState<{ password: string; at: number } | undefined>(undefined);
  const [score, setScore] = useState<number | undefined>(undefined);
  const [copied, flash] = useFlash();
  useSecretView(shown !== undefined);
  // Asked once, for the login the dialog is keyed by; what the asking needs is read when it asks.
  const asking = useRef({ item, actions, onClose });
  useEffect(() => {
    asking.current = { item, actions, onClose };
  });
  useEffect(() => {
    let cancelled = false;
    const { item: login, actions: ask, onClose: cancel } = asking.current;
    void ask.reveal(login, { password: true }).then((answer) => {
      if (cancelled) return;
      if (answer?.password == null) cancel();
      else setShown({ password: answer.password, at: Date.now() });
    });
    return () => {
      cancelled = true;
    };
  }, []);
  const password = shown?.password;
  useEffect(() => {
    if (password === undefined) return undefined;
    let cancelled = false;
    backend.dispatch({ command: "password_score", password }).then(
      (answer) => {
        if (!cancelled) setScore(answer.score);
      },
      () => undefined,
    );
    return () => {
      cancelled = true;
    };
  }, [backend, password]);
  // The shared clock ticks on whole seconds, so it can read just before the moment of the reveal.
  const left =
    shown === undefined
      ? REVEAL_SECONDS
      : Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - shown.at) / 1000)));
  useEffect(() => {
    if (shown !== undefined && left === 0) onClose();
  }, [shown, left, onClose]);
  if (shown === undefined) return null;
  const copy = () =>
    void actions.copy(item, { type: "password" }).then((done) => {
      if (done) flash("password");
    });
  const title =
    item.title !== "" && itemAccount(item) !== ""
      ? `${item.title} · ${itemAccount(item)}`
      : itemName(item);
  const length = Array.from(shown.password).length;
  const kinds = new Intl.ListFormat(locale, { type: "conjunction" }).format(
    kindsOf(shown.password).map((k) => t(`item.reveal.kinds.${k}`)),
  );
  return (
    <Dialog
      open
      title={title}
      onClose={onClose}
      width={520}
      hint={
        <span
          className="flex items-center gap-1.5 font-sans text-[11.5px]"
          data-testid="item-reveal-countdown">
          <CountdownRing
            validFromMs={shown.at}
            validUntilMs={shown.at + REVEAL_SECONDS * 1000}
            nowMs={now}
            size={14}
            stroke={2}
            still={motionReduced(settings)}
          />
          {t("item.reveal.hideIn", { s: left })}
        </span>
      }
      actions={
        <>
          <Button
            icon={copied === undefined ? "copy" : "check"}
            onClick={copy}
            data-testid="item-reveal-copy">
            {copied === undefined ? t("item.reveal.copy") : t("notice.copiedKept")}
          </Button>
          <Button variant="primary" onClick={onClose}>
            {t("common.done")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-3.5" data-testid="item-revealed">
        <Banner tone="warn" marker="icon">
          {t("item.reveal.warning")}
        </Banner>
        <div className="flex min-h-[92px] flex-col justify-center gap-1.5 rounded-10 bg-inset px-4 py-3">
          <span
            className="mono text-[26px] leading-[1.25] font-semibold tracking-[1px] break-all text-fg select-all"
            data-testid="item-revealed-password">
            {shown.password}
          </span>
          <span className="text-[11.5px] text-fg-muted">
            {[t("item.reveal.chars", { n: length }), kinds].filter((s) => s !== "").join(" · ")}
          </span>
        </div>
        <ScoreMeter score={score} />
        {platform === "linux" && (
          <p className="text-[12px] text-fg-subtle">{t("item.reveal.linux")}</p>
        )}
      </div>
    </Dialog>
  );
}
