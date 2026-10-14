// Pieces of the phone's login pages: a section of rows in a card (board 24), the linked account's
// code with its ring, and deleting a login after asking.
import { type ItemView, errorText, itemName } from "@lockra/shared";
import {
  Button,
  Card,
  CountdownRing,
  Dialog,
  OtpCode,
  WARNING_SECONDS,
  motionReduced,
  remaining,
  useBackend,
  useClock,
  useCodes,
  useSubmit,
  useT,
  useUiState,
} from "@lockra/ui";
import type { ReactNode } from "react";

/** A section of a login's page: its title, then its rows on hairlines in a card. */
export function LoginSection({
  title,
  aside,
  children,
  testId,
}: {
  title: string;
  aside?: ReactNode;
  children: ReactNode;
  testId?: string;
}) {
  return (
    <section className="flex flex-col gap-2" aria-label={title} data-testid={testId}>
      <div className="flex items-baseline justify-between gap-2 px-1">
        <h2 className="text-[13px] font-medium text-fg-muted">{title}</h2>
        {aside}
      </div>
      <Card padding="none" className="flex flex-col divide-y divide-border px-4">
        {children}
      </Card>
    </section>
  );
}

/** The current code of account `entryId` and its ring, from the code frames (the codes screen's). */
export function LoginCode({
  entryId,
  size = 22,
  ring = 26,
}: {
  entryId: string;
  size?: number;
  ring?: number;
}) {
  const codes = useCodes();
  const now = useClock();
  const { settings } = useUiState();
  const code = codes.get(entryId);
  if (code === undefined) return <span className="mono text-fg-subtle">— — —</span>;
  const totp = code.valid_from_ms !== null && code.valid_until_ms !== null;
  const expired = totp && code.valid_until_ms !== null && now >= code.valid_until_ms;
  const current = expired && code.next_code !== null ? code.next_code : code.code;
  const left =
    totp && !expired
      ? remaining(code.valid_from_ms ?? 0, code.valid_until_ms ?? 0, now)
      : undefined;
  const warning = left !== undefined && left.seconds <= WARNING_SECONDS;
  return (
    <span className="inline-flex items-center gap-2" data-testid="login-code">
      <OtpCode code={current} size={size} weight="semibold" tone={warning ? "warning" : "normal"} />
      {totp && !expired && code.valid_from_ms !== null && code.valid_until_ms !== null && (
        <CountdownRing
          validFromMs={code.valid_from_ms}
          validUntilMs={code.valid_until_ms}
          nowMs={now}
          size={ring}
          stroke={3}
          still={motionReduced(settings)}
        />
      )}
    </span>
  );
}

export function DeleteLoginDialog({
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
          <Button variant="ghost" size="lg" onClick={onClose} data-autofocus>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            size="lg"
            icon="trash"
            loading={submit.busy}
            onClick={() => void remove()}
            data-testid="login-delete-confirm">
            {t("passwords.delete.confirm")}
          </Button>
        </>
      }>
      <p>{t("passwords.delete.body", { name: itemName(item) })}</p>
      {submit.error !== undefined && (
        <p role="alert" className="mt-2 text-[13px] text-danger">
          {errorText(t, submit.error)}
        </p>
      )}
    </Dialog>
  );
}
