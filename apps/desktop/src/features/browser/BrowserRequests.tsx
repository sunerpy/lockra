// The browser extension's requests that wait for the user (boards 17–19): a pairing to approve,
// then a fill or a copy to confirm, one at a time, the oldest first. The shell brings the window
// up as each one comes (lib.rs, `forward_events`); the core decides what each needs: a pairing
// always the master password or the system check that unlocks the vault, a fill of a
// 「填充前验证」 login the same, a frame of another site a click.
import {
  CONFIRM_TTL_MS,
  type ConfirmView,
  PAIR_TTL_MS,
  type PairRequestView,
  browserName,
  errorText,
  isLockraError,
  pairedName,
  pairingCode,
} from "@lockra/shared";
import {
  Button,
  CountdownRing,
  Dialog,
  Icon,
  Keycap,
  PasswordField,
  type Proof,
  biometricFellThrough,
  remaining,
  unlockBiometric,
  useBackend,
  useClock,
  useT,
  useUiState,
} from "@lockra/ui";
import { type ReactNode, type SubmitEvent, useId, useState } from "react";
import { useDispatch, useSubmit } from "../../app/dispatch";

export function BrowserRequests() {
  const { browser } = useUiState();
  const now = useClock();
  // Requests answered here: a state that still lists one (it comes a moment later) shows it no more.
  const [done, setDone] = useState<readonly string[]>([]);
  // The pairing on screen, held while it is there: one that leaves the state as it expires still
  // says so until closed.
  const [held, setHeld] = useState<PairRequestView | null>(null);
  const waiting = browser.pairing.filter((p) => !done.includes(p.id));
  const fresh = waiting.find((p) => p.expires_at_ms > now) ?? null;
  if (held === null && fresh !== null) setHeld(fresh);
  // Gone before it expired (the vault locked, the core dropped it): nothing to say.
  if (held !== null && now < held.expires_at_ms && !browser.pairing.some((p) => p.id === held.id))
    setHeld(null);
  const finish = (id: string) => {
    setDone((list) => [...list, id]);
    setHeld(null);
  };
  if (held !== null) {
    const queued = waiting.filter((p) => p.id !== held.id && p.expires_at_ms > now).length;
    return (
      <PairDialog
        key={held.id}
        request={held}
        now={now}
        queued={queued}
        onDone={() => finish(held.id)}
      />
    );
  }
  const confirm = browser.confirms.find((c) => !done.includes(c.id) && c.expires_at_ms > now);
  if (confirm === undefined) return null;
  return (
    <ConfirmDialog
      key={confirm.id}
      request={confirm}
      now={now}
      onDone={() => setDone((list) => [...list, confirm.id])}
    />
  );
}

function PairDialog({
  request,
  now,
  queued,
  onDone,
}: {
  request: PairRequestView;
  now: number;
  queued: number;
  onDone: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const name = pairedName(request.browser, request.label);
  const [refusedLate, setRefusedLate] = useState(false);
  const expired = refusedLate || now >= request.expires_at_ms;
  const { seconds } = remaining(request.expires_at_ms - PAIR_TTL_MS, request.expires_at_ms, now);
  const deny = () => {
    onDone();
    void dispatch({ command: "browser_pair_deny", id: request.id });
  };
  const facts = (
    <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 text-[12px] leading-4">
      <dt className="text-fg-subtle">{t("browser.pair.browser")}</dt>
      <dd className="text-fg">{name}</dd>
      <dt className="text-fg-subtle">{t("browser.pair.extension")}</dt>
      <dd className="mono break-all text-fg">{request.extension}</dd>
    </dl>
  );
  if (expired)
    return (
      <Dialog
        open
        title={t("browser.pair.expiredTitle")}
        onClose={onDone}
        width={460}
        actions={
          <Button variant="primary" onClick={onDone} data-autofocus>
            {t("common.close")}
          </Button>
        }>
        <div className="flex flex-col gap-4">
          <p>{t("browser.pair.body")}</p>
          <div className="flex flex-col items-center gap-2 rounded-10 bg-inset px-4 py-5">
            <span className="mono text-[40px] leading-none font-semibold tracking-[3px] text-fg-subtle">
              ––– –––
            </span>
            <span className="text-[12px] text-fg-muted">{t("browser.pair.expiredNote")}</span>
          </div>
          {facts}
        </div>
      </Dialog>
    );
  return (
    <ProofDialog
      title={t("browser.pair.title", { browser: browserName(request.browser) })}
      proof
      confirm={t("browser.pair.approve")}
      confirmWith={(kind) => t(`browser.pair.approveWith.${kind}`)}
      reason={t("browser.pair.reason", { name })}
      onConfirm={async (proof) => {
        try {
          await backend.dispatch({ command: "browser_pair_approve", id: request.id, ...proof });
          onDone();
        } catch (error: unknown) {
          // Approved a moment too late: the request is gone.
          if (isLockraError(error) && error.code === "browser_pair_expired") setRefusedLate(true);
          throw error;
        }
      }}
      onDeny={deny}
      hint={
        <>
          <Keycap>Esc</Keycap> {t("browser.deny")}
        </>
      }>
      <p>{t("browser.pair.body")}</p>
      <div className="flex flex-col items-center gap-2 rounded-10 bg-inset px-4 py-5">
        <span
          className="mono text-[40px] leading-none font-semibold tracking-[3px] text-fg"
          data-testid="pair-code">
          {pairingCode(request.code)}
        </span>
        <span className="flex items-center gap-1.5 text-[12px] text-fg-muted">
          <CountdownRing
            validFromMs={request.expires_at_ms - PAIR_TTL_MS}
            validUntilMs={request.expires_at_ms}
            nowMs={now}
            size={14}
            stroke={2}
          />
          {t("browser.pair.expiresIn", { n: seconds })}
        </span>
      </div>
      {facts}
      {queued > 0 && (
        <p className="flex items-center gap-1.5 text-[12px] text-fg-muted">
          <Icon name="info" size={13} />
          {t("browser.pair.queued", { n: queued })}
        </p>
      )}
    </ProofDialog>
  );
}

function ConfirmDialog({
  request,
  now,
  onDone,
}: {
  request: ConfirmView;
  now: number;
  onDone: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const { browser, title, action } = request;
  const name = pairedName(browser, request.label);
  const frame = request.reasons.includes("cross_origin");
  const login = request.username === "" ? title : `${title} · ${request.username}`;
  const { seconds } = remaining(request.expires_at_ms - CONFIRM_TTL_MS, request.expires_at_ms, now);
  // The title names the browser; the facts, which of its paired extensions asks.
  const asker = browserName(browser);
  const heading = frame
    ? t("browser.confirm.titleFrame", { browser: asker })
    : action === "copy"
      ? t("browser.confirm.titleCopy", { browser: asker, title })
      : t("browser.confirm.titleFill", { browser: asker, title });
  const rows: [string, ReactNode, boolean][] =
    request.top_url === null
      ? [
          [t("browser.confirm.site"), request.frame_url, true],
          [t("browser.confirm.login"), login, false],
          [t("browser.confirm.browser"), name, false],
        ]
      : [
          [t("browser.confirm.frameSite"), request.frame_url, true],
          [t("browser.confirm.topSite"), request.top_url, true],
          [t("browser.confirm.login"), login, false],
        ];
  return (
    <ProofDialog
      title={heading}
      proof={request.needs_proof}
      confirm={
        request.needs_proof
          ? t(action === "copy" ? "browser.confirm.copy" : "browser.confirm.fill")
          : t(frame ? "browser.confirm.fillAnyway" : "browser.confirm.fill")
      }
      confirmWith={(kind) =>
        t(
          action === "copy"
            ? `browser.confirm.copyWith.${kind}`
            : `browser.confirm.fillWith.${kind}`,
        )
      }
      reason={t(action === "copy" ? "browser.confirm.reasonCopy" : "browser.confirm.reasonFill", {
        title,
      })}
      onConfirm={async (proof) => {
        await backend.dispatch({ command: "browser_fill_confirm", id: request.id, ...proof });
        onDone();
      }}
      onDeny={() => {
        onDone();
        void dispatch({ command: "browser_fill_deny", id: request.id });
      }}
      hint={
        <span className="flex items-center gap-1.5 text-[11px] text-fg-muted">
          <CountdownRing
            validFromMs={request.expires_at_ms - CONFIRM_TTL_MS}
            validUntilMs={request.expires_at_ms}
            nowMs={now}
            size={14}
            stroke={2}
          />
          {t("browser.confirm.autoDeny", { n: seconds })}
        </span>
      }>
      {frame && <p>{t("browser.confirm.frame")}</p>}
      {request.reasons.includes("protected") && (
        <p>
          {t(
            action === "copy" ? "browser.confirm.protectedCopy" : "browser.confirm.protectedFill",
            {
              title,
            },
          )}
        </p>
      )}
      <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-1.5 rounded-10 bg-inset px-4 py-3 text-[12px] leading-4">
        {rows.map(([label, value, mono]) => (
          <div key={label} className="contents">
            <dt className="text-fg-subtle">{label}</dt>
            <dd className={mono ? "mono break-all text-fg" : "text-fg"}>{value}</dd>
          </div>
        ))}
      </dl>
      {action === "fill" && (
        <p className="text-[12px] text-fg-muted">{t("browser.confirm.fillNote")}</p>
      )}
    </ProofDialog>
  );
}

/** A request's dialog: deny, or go ahead — with the system check that unlocks the vault or the
 *  master password where `proof` is asked for, with a click where it is not. Esc denies. */
function ProofDialog({
  title,
  proof,
  confirm,
  confirmWith,
  reason,
  onConfirm,
  onDeny,
  hint,
  children,
}: {
  title: string;
  proof: boolean;
  /** The button that goes ahead (with the master password, or without proof). */
  confirm: string;
  /** The button that goes ahead with the system check. */
  confirmWith: (kind: NonNullable<ReturnType<typeof unlockBiometric>>) => string;
  /** What the system check's prompt says. */
  reason: string;
  /** Goes ahead; an error is shown here. */
  onConfirm: (proof: Proof) => Promise<void>;
  onDeny: () => void;
  hint: ReactNode;
  children: ReactNode;
}) {
  const t = useT();
  const { lock } = useUiState();
  const biometric = unlockBiometric(lock);
  const formId = useId();
  const submit = useSubmit();
  const [chosePassword, setChosePassword] = useState(false);
  const [password, setPassword] = useState("");
  // After the system check failed or was cancelled, the master password is what is left.
  const withPassword =
    proof && (biometric === null || chosePassword || biometricFellThrough(submit.error));
  const go = async (given: Proof) => {
    await submit.run(() => onConfirm(given));
  };
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    if (!proof) return go({});
    if (!withPassword || password === "") return;
    await go({ password });
    setPassword("");
  };
  const failure =
    submit.error === undefined || biometricFellThrough(submit.error)
      ? undefined
      : errorText(t, submit.error);
  return (
    <Dialog
      open
      title={title}
      onClose={onDeny}
      width={460}
      hint={hint}
      actions={
        <>
          <Button variant="ghost" onClick={onDeny}>
            {t("browser.deny")}
          </Button>
          {proof && !withPassword && biometric !== null ? (
            <>
              <Button variant="outline" onClick={() => setChosePassword(true)}>
                {t("browser.usePassword")}
              </Button>
              <Button
                variant="primary"
                icon="fingerprint"
                loading={submit.busy}
                onClick={() => void go({ reason })}
                data-autofocus>
                {confirmWith(biometric)}
              </Button>
            </>
          ) : (
            <Button
              variant="primary"
              type="submit"
              form={formId}
              loading={submit.busy}
              disabled={withPassword && password === ""}
              data-autofocus={withPassword ? undefined : true}>
              {confirm}
            </Button>
          )}
        </>
      }>
      <form
        id={formId}
        onSubmit={(e) => void onSubmit(e)}
        className="flex flex-col gap-4"
        data-testid="browser-request">
        {children}
        {withPassword ? (
          <PasswordField
            label={t("sync.masterPassword")}
            value={password}
            onChange={setPassword}
            autoComplete="current-password"
            error={failure}
            // Focused as the dialog opens, and as it turns to the password after the system check.
            autoFocus
            data-autofocus
          />
        ) : (
          failure !== undefined && <p className="text-[12px] text-danger">{failure}</p>
        )}
      </form>
    </Dialog>
  );
}
