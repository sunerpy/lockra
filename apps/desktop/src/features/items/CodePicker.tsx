// The code a login links to (board 07-c), for the login's form and the detail pane: the picker of
// the accounts, or of one made on the spot (a QR code, a link, a key typed in), and the linked
// account's current code as both show it.
import { type EntryView, entryLabel, errorText } from "@lockra/shared";
import {
  Button,
  CountdownRing,
  Dialog,
  EntryAvatar,
  Icon,
  type IconName,
  Input,
  OtpCode,
  WARNING_SECONDS,
  remaining,
  useBackend,
  useClock,
  useCodes,
  useT,
  useUiState,
} from "@lockra/ui";
import { useState } from "react";
import { motionReduced } from "../../app/appearance";
import { useSubmit } from "../../app/dispatch";
import { AddManualDialog, AddUriDialog } from "../entries/EntryDialogs";

/** What the code picker shows: the accounts, or one way of making a new one (board 07-c). */
type Picking = "list" | "scan" | "uri" | "manual";

const MAKE_ICONS: Record<Exclude<Picking, "list">, IconName> = {
  scan: "scan",
  uri: "link",
  manual: "edit",
};

/** The code picker (board 07-c): the accounts to choose from, or one made on the spot (a QR code, a
 *  link, a key typed in), picked once it is added. The login form's and the detail pane's. */
export function CodePicker({
  entries,
  onPick,
  onClose,
}: {
  entries: readonly EntryView[];
  onPick: (id: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const { settings } = useUiState();
  const codes = useCodes();
  const now = useClock();
  const [picking, setPicking] = useState<Picking>("list");
  const [query, setQuery] = useState("");
  const needle = query.trim().toLocaleLowerCase();
  const shown = entries.filter((e) =>
    `${e.issuer} ${e.account}`.toLocaleLowerCase().includes(needle),
  );
  const still = motionReduced(settings);
  return (
    <>
      {picking === "list" && (
        <Dialog
          open
          width={420}
          title={t("item.code.pickTitle")}
          onClose={onClose}
          actions={
            <Button variant="ghost" onClick={onClose}>
              {t("common.cancel")}
            </Button>
          }>
          <div className="flex flex-col gap-2" data-testid="item-form-code-picker">
            <Input
              icon="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("item.code.search")}
              aria-label={t("item.code.search")}
              data-autofocus
            />
            <div className="text-[12px] text-fg-subtle">{t("item.code.existing")}</div>
            {shown.length === 0 ? (
              <p className="text-[12px]">{t("item.code.none")}</p>
            ) : (
              <ul className="flex max-h-72 flex-col overflow-y-auto">
                {shown.map((entry) => (
                  <li key={entry.id}>
                    <button
                      type="button"
                      onClick={() => onPick(entry.id)}
                      className="flex h-12 w-full items-center gap-2.5 rounded-6 px-2 text-left transition-colors hover:bg-inset focus-visible:bg-inset"
                      data-testid="item-form-code-option">
                      <EntryAvatar
                        issuer={entry.issuer}
                        account={entry.account}
                        color={entry.color}
                        mark={entry.mark}
                        size={28}
                      />
                      <span className="min-w-0 flex-1 truncate text-[13px] text-fg">
                        {entryLabel(entry.issuer, entry.account)}
                      </span>
                      <LinkedCode
                        code={codes.get(entry.id)}
                        now={now}
                        still={still}
                        size={14}
                        ring={16}
                      />
                    </button>
                  </li>
                ))}
              </ul>
            )}
            <div className="h-px bg-border" />
            <div className="text-[12px] text-fg-subtle">{t("item.code.create")}</div>
            <div className="flex flex-col">
              {(["scan", "uri", "manual"] as const).map((way) => (
                <button
                  key={way}
                  type="button"
                  onClick={() => setPicking(way)}
                  className="flex h-9 items-center gap-2.5 rounded-6 px-2 text-left text-[13px] text-fg transition-colors hover:bg-inset focus-visible:bg-inset"
                  data-testid={`item-form-code-${way}`}>
                  <Icon name={MAKE_ICONS[way]} size={14} className="text-fg-muted" />
                  {t(`item.code.${way}`)}
                </button>
              ))}
            </div>
          </div>
        </Dialog>
      )}
      {/* Cancelled, each goes back to the picker. Their forms are not the login's: a submit in
          one stays there (React bubbles it through the dialog to the login's form otherwise). */}
      <div className="contents" onSubmit={(e) => e.stopPropagation()}>
        {picking === "scan" && (
          <ScanCodeDialog onClose={() => setPicking("list")} onAdded={onPick} />
        )}
        {picking === "uri" && <AddUriDialog onClose={() => setPicking("list")} onAdded={onPick} />}
        {picking === "manual" && (
          <AddManualDialog onClose={() => setPicking("list")} onAdded={onPick} />
        )}
      </div>
    </>
  );
}

/** 扫描二维码 on the desktop (board 07-c): the QR code of a site's two-factor setup, from a
 *  screenshot on the clipboard or from an image file, read in Rust; the one account it holds is
 *  added at once (several go through the import on the Codes page). */
function ScanCodeDialog({
  onClose,
  onAdded,
}: {
  onClose: () => void;
  onAdded: (id: string) => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const { platform } = useUiState();
  const submit = useSubmit();
  const hint =
    platform === "windows"
      ? t("item.code.scanHintWindows")
      : platform === "macos"
        ? t("item.code.scanHintMac")
        : t("item.code.scanHintLinux");
  const read = async () => {
    const added = await submit.run(() => backend.dispatch({ command: "entry_add_clipboard" }));
    if (added !== undefined) onAdded(added.id);
  };
  const pick = async () => {
    const id = await submit.run(() => backend.addEntryFromImage());
    // `null`: the dialog was cancelled.
    if (typeof id === "string") onAdded(id);
  };
  return (
    <Dialog
      open
      width={420}
      title={t("item.code.scanTitle")}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button icon="image" onClick={() => void pick()} disabled={submit.busy}>
            {t("item.code.pickImage")}
          </Button>
          <Button
            variant="primary"
            icon="clipboard"
            onClick={() => void read()}
            loading={submit.busy}
            data-autofocus>
            {t("item.code.readClipboard")}
          </Button>
        </>
      }>
      <div className="flex flex-col gap-2" data-testid="item-form-code-scan-dialog">
        <p className="text-[13px] text-fg">{t("item.code.scanBody")}</p>
        <p className="text-[12px] text-fg-subtle">{hint}</p>
        {submit.error !== undefined && (
          <p role="alert" className="text-[12px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
      </div>
    </Dialog>
  );
}

/** The linked account's current code and its ring (from the code frames, as the codes page). */
export function LinkedCode({
  code,
  now,
  still,
  size = 20,
  ring = 20,
}: {
  code: ReturnType<ReturnType<typeof useCodes>["get"]>;
  now: number;
  still: boolean;
  size?: number;
  ring?: number;
}) {
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
    <span className="inline-flex items-center gap-2">
      <OtpCode code={current} size={size} weight="semibold" tone={warning ? "warning" : "normal"} />
      {totp && !expired && code.valid_from_ms !== null && code.valid_until_ms !== null && (
        <CountdownRing
          validFromMs={code.valid_from_ms}
          validUntilMs={code.valid_until_ms}
          nowMs={now}
          size={ring}
          stroke={2.5}
          still={still}
        />
      )}
    </span>
  );
}
