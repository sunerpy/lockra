// The toolbar popup (boards 30–32): 360 × 540 in every state, following the browser's light or dark
// scheme. It shows what the background says (the connection to Lockra, the current site's logins)
// and asks it to act; it holds no secret: a fill goes from Lockra to the page through the
// background, a copy is Lockra's (which clears the clipboard in time).
import {
  Button,
  Card,
  Icon,
  IconButton,
  type IconName,
  Input,
  Keycap,
  Keycaps,
  Lamp,
  type LampTone,
  Logo,
  Menu,
  cx,
} from "@lockra/ui";
import { type KeyboardEvent, type ReactNode, useEffect, useRef, useState } from "react";
import type { ItemSummary } from "../protocol/schemas";
import type { PopupState, Screen } from "../shared/messages";
import { Avatar } from "../shared/Avatar";
import { refusalText } from "../shared/refusal";
import { useText } from "../shared/text";
import type { PageClient } from "./client";
import { type Page, usePage } from "./usePage";

const LAMP: Record<Screen, LampTone> = {
  loading: "idle",
  no_host: "off",
  not_running: "off",
  update_app: "warn",
  update_extension: "warn",
  no_vault: "off",
  locked: "idle",
  unpaired: "off",
  pairing: "warn",
  ready: "ok",
};

export function Popup({ client, now = Date.now }: { client: PageClient; now?: () => number }) {
  const page = usePage(client);
  const t = useText();
  const state = page.state;
  const screen = state?.screen ?? "loading";
  const label = {
    loading: t("state.connecting"),
    no_host: t("state.unpaired"),
    not_running: t("state.notRunning"),
    update_app: t("state.incompatible"),
    update_extension: t("state.incompatible"),
    no_vault: t("state.noVault"),
    locked: t("state.locked"),
    unpaired: t("state.unpaired"),
    pairing: t("state.pairing"),
    ready: t("state.unlocked"),
  }[screen];
  const paired = screen === "ready" || screen === "locked";
  const hint = !state?.shortcut
    ? t("popup.footerNoShortcut")
    : paired
      ? t("popup.footerFill")
      : t("popup.footerPair");
  return (
    <div className="app-canvas flex h-[540px] w-[360px] flex-col overflow-hidden text-fg">
      <header className="flex h-[52px] shrink-0 items-center gap-2.5 px-4">
        <Logo size={24} />
        <span className="text-[15px] font-semibold">Lockra</span>
        <span className="flex-1" />
        <Lamp tone={LAMP[screen]} />
        <span className="text-[12px] text-fg-muted" data-testid="popup-state">
          {label}
        </span>
        <IconButton
          icon="settings"
          label={t("popup.settings")}
          size={28}
          onClick={() => client.openOptions()}
        />
      </header>
      <main className="flex min-h-0 flex-1 flex-col overflow-y-auto px-4 pt-2 pb-3">
        <Body page={page} client={client} now={now} />
      </main>
      <footer className="flex h-11 shrink-0 items-center gap-2 border-t border-border px-4">
        {state?.shortcut ? <Keycaps keys={state.shortcut} /> : null}
        {/* The board's longer hint ran into 「打开 Lockra」: shorter now, and cut with an ellipsis. */}
        <span className="min-w-0 flex-1 truncate text-[11px] text-fg-subtle" title={hint}>
          {hint}
        </span>
        <Button variant="text" size="sm" onClick={() => page.send({ kind: "start", show: true })}>
          {t("popup.openLockra")}
        </Button>
      </footer>
    </div>
  );
}

function Body({ page, client, now }: { page: Page; client: PageClient; now: () => number }) {
  const t = useText();
  const state = page.state;
  if (!state || state.screen === "loading") {
    return <p className="m-auto text-[13px] text-fg-muted">{t("popup.loading")}</p>;
  }
  const send = page.send;
  switch (state.screen) {
    case "ready":
      return <Ready page={page} state={state} client={client} />;
    case "unpaired":
      return <Unpaired state={state} onConnect={() => send({ kind: "pair" })} />;
    case "pairing":
      return <Pairing state={state} now={now} onCancel={() => send({ kind: "pair.cancel" })} />;
    case "locked":
      return (
        <Centered
          icon="lock"
          title={t("locked.title")}
          body={
            state.site?.site ? t("locked.body", { site: state.site.site }) : t("locked.bodyNoSite")
          }
          action={
            <Button
              variant="primary"
              size="lg"
              icon="unlock"
              onClick={() => send({ kind: "unlock" })}>
              {t("locked.unlock")}
            </Button>
          }
          note={state.unlockAsked ? t("locked.asked") : t("locked.note")}
        />
      );
    case "not_running":
      return (
        <Centered
          icon="power"
          title={t("notRunning.title")}
          body={t("notRunning.body")}
          action={
            <Button
              variant="primary"
              size="lg"
              icon="power"
              loading={state.starting}
              onClick={() => send({ kind: "start", show: false })}>
              {state.starting ? t("notRunning.starting") : t("notRunning.start")}
            </Button>
          }
          note={t("notRunning.note")}
        />
      );
    case "no_host":
      return (
        <Centered
          icon="browser"
          title={t("noHost.title")}
          body={t("noHost.body")}
          action={
            <Button
              variant="primary"
              size="lg"
              icon="refresh"
              onClick={() => send({ kind: "retry" })}>
              {t("noHost.retry")}
            </Button>
          }
        />
      );
    case "no_vault":
      return (
        <Centered
          icon="lock"
          title={t("noVault.title")}
          body={t("noVault.body")}
          action={
            <Button variant="primary" size="lg" onClick={() => send({ kind: "start", show: true })}>
              {t("popup.openLockra")}
            </Button>
          }
        />
      );
    case "update_app":
      return (
        <Centered
          icon="refresh"
          title={t("updateApp.title")}
          body={t("updateApp.body", {
            extension: state.extensionVersion,
            app: state.appVersion ?? "?",
          })}
          action={
            <Button variant="primary" size="lg" onClick={() => send({ kind: "start", show: true })}>
              {t("popup.openLockra")}
            </Button>
          }
          note={t("updateApp.note")}
        />
      );
    case "update_extension":
      return (
        <Centered
          icon="puzzle"
          title={t("updateExtension.title")}
          body={t("updateExtension.body", {
            extension: state.extensionVersion,
            app: state.appVersion ?? "?",
          })}
          action={
            // Firefox opens no about:addons for an extension: its note says where to go.
            client.target === "chromium" ? (
              <Button variant="primary" size="lg" onClick={() => send({ kind: "extensions" })}>
                {t("updateExtension.open")}
              </Button>
            ) : null
          }
          note={t("updateExtension.note")}
        />
      );
  }
}

/** A state in the middle: an icon on the theme's card, a title, a line, an action, a note. */
function Centered({
  icon,
  title,
  body,
  action,
  note,
}: {
  icon: IconName;
  title: string;
  body: string;
  action?: ReactNode;
  note?: string;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3.5 px-2 text-center">
      <span className="flex size-14 items-center justify-center rounded-[18px] bg-surface text-fg-muted hairline panel">
        <Icon name={icon} size={24} />
      </span>
      <div className="flex flex-col gap-2">
        <h1 className="text-[17px] font-semibold">{title}</h1>
        <p className="text-[13px] leading-5 text-fg-muted">{body}</p>
      </div>
      {action}
      {note ? <p className="text-[12px] leading-normal text-fg-subtle">{note}</p> : null}
    </div>
  );
}

function Unpaired({ state, onConnect }: { state: PopupState; onConnect: () => void }) {
  const t = useText();
  const steps = [t("unpaired.step1"), t("unpaired.step2"), t("unpaired.step3")];
  return (
    <div className="flex flex-col gap-3.5 pt-4">
      <h1 className="text-[20px] font-semibold">{t("unpaired.title")}</h1>
      <p className="text-[13px] leading-5 text-fg-muted">{t("unpaired.body")}</p>
      <ol className="flex flex-col gap-2.5">
        {steps.map((step, index) => (
          <li key={step} className="flex gap-3 text-[13px] leading-5">
            <span className="mono flex size-6 shrink-0 items-center justify-center rounded-full border border-border bg-inset text-[11px] text-fg-muted">
              {index + 1}
            </span>
            <span className="pt-0.5">{step}</span>
          </li>
        ))}
      </ol>
      {state.pairingFailed ? (
        <p role="alert" className="text-[12px] text-danger">
          {t("unpaired.failed", {
            reason:
              state.pairingFailed === "ended"
                ? t("refusal.ended")
                : refusalText(t, state.pairingFailed),
          })}
        </p>
      ) : null}
      <Button variant="primary" size="lg" className="w-full" onClick={onConnect}>
        {t("unpaired.connect")}
      </Button>
    </div>
  );
}

function useNow(now: () => number): number {
  const [time, setTime] = useState(now);
  useEffect(() => {
    const timer = setInterval(() => setTime(now()), 1_000);
    return () => clearInterval(timer);
  }, [now]);
  return time;
}

function Pairing({
  state,
  now,
  onCancel,
}: {
  state: PopupState;
  now: () => number;
  onCancel: () => void;
}) {
  const t = useText();
  const time = useNow(now);
  const code = state.pairing?.code ?? null;
  const expiresAt = state.pairing?.expiresAt ?? null;
  const left = expiresAt === null ? null : Math.max(0, Math.ceil((expiresAt - time) / 1_000));
  return (
    <div className="flex flex-col gap-3.5 pt-4">
      <h1 className="text-[20px] font-semibold">{t("pairing.title")}</h1>
      <p className="text-[13px] leading-5 text-fg-muted">{t("pairing.body")}</p>
      <Card padding="none" className="flex flex-col items-center gap-3 py-5">
        <span
          className="mono text-[40px] leading-tight font-semibold tracking-[3px]"
          data-testid="pairing-code"
          aria-label={code ? Array.from(code).join(" ") : undefined}>
          {code ? `${code.slice(0, 3)} ${code.slice(3)}` : "··· ···"}
        </span>
        <span className="flex items-center gap-2 text-[12px] text-fg-muted">
          {left === null ? t("pairing.waiting") : <Ring fraction={left / 120} />}
          {left === null ? null : t("pairing.expires", { n: left })}
        </span>
      </Card>
      <p className="text-[12px] text-fg-subtle">{t("pairing.mismatch")}</p>
      <Button size="lg" className="w-full" onClick={onCancel}>
        {t("pairing.cancel")}
      </Button>
    </div>
  );
}

/** The request's time left, as a ring in the theme's meter colour. */
function Ring({ fraction }: { fraction: number }) {
  const radius = 6;
  const length = 2 * Math.PI * radius;
  return (
    <svg width={14} height={14} viewBox="0 0 14 14" aria-hidden>
      <circle
        cx={7}
        cy={7}
        r={radius}
        fill="none"
        strokeWidth={2}
        className="stroke-[var(--track)]"
      />
      <circle
        cx={7}
        cy={7}
        r={radius}
        fill="none"
        strokeWidth={2}
        strokeLinecap="round"
        strokeDasharray={length}
        strokeDashoffset={length * (1 - Math.min(1, Math.max(0, fraction)))}
        transform="rotate(-90 7 7)"
        className="stroke-[var(--meter)]"
      />
    </svg>
  );
}

// ---- the current site --------------------------------------------------------------------------

function Ready({ page, state, client }: { page: Page; state: PopupState; client: PageClient }) {
  const t = useText();
  const [text, setText] = useState("");
  const [active, setActive] = useState(0);
  /** The fills whose http warning the user answered (filled anyway, or cancelled). */
  const [answered, setAnswered] = useState<ReadonlySet<string>>(new Set());
  const [denied, setDenied] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const { send, results } = page;
  const site = state.site;
  const searching = text.trim() !== "";
  const items = searching
    ? page.search?.text === text
      ? page.search.items
      : []
    : (site?.items ?? []);

  useEffect(() => {
    if (!searching) return;
    const timer = setTimeout(() => send({ kind: "search", text }), 150);
    return () => clearTimeout(timer);
  }, [searching, text, send]);

  // A fill that went through closes the popup.
  useEffect(() => {
    if (Object.values(results).some((r) => r.action === "fill" && r.outcome === "done"))
      client.close();
  }, [results, client]);
  // One that met an http page asks first.
  const httpId = Object.entries(results).find(
    ([id, r]) => r.action === "fill" && r.outcome === "http_downgrade" && !answered.has(id),
  )?.[0];
  const http = httpId ? (site?.items.find((i) => i.item_id === httpId) ?? null) : null;
  const answer = (item: ItemSummary) => setAnswered((now) => new Set([...now, item.item_id]));

  const fill = (item: ItemSummary, allowHttp = false) => {
    if (allowHttp) answer(item);
    send({ kind: "fill", itemId: item.item_id, allowHttp });
  };
  const copy = (item: ItemSummary, part: "username" | "password" | "totp") =>
    send({ kind: "copy", itemId: item.item_id, part: { type: part } });

  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const target = event.target;
    const typing = target instanceof HTMLInputElement;
    if (event.key === "/" && !typing) {
      event.preventDefault();
      box.current?.querySelector<HTMLInputElement>('input[type="search"]')?.focus();
      return;
    }
    if (items.length === 0) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const step = event.key === "ArrowDown" ? 1 : -1;
      setActive((now) => (now + step + items.length) % items.length);
    } else if (event.key === "Enter") {
      const item = items[Math.min(active, items.length - 1)];
      if (!item) return;
      event.preventDefault();
      if (item.matches && site?.fillable) fill(item);
      else copy(item, "password");
    }
  };

  const grant = () => {
    // In the click: the browser asks only from a user's action.
    void client.requestAccess().then((granted) => {
      if (granted) send({ kind: "access.changed" });
      else setDenied(true);
    });
  };

  return (
    <div ref={box} className="flex min-h-0 flex-col gap-3.5" onKeyDown={onKey}>
      {state.pageAccess ? null : (
        <div className="flex gap-3 rounded-10 bg-surface px-4 py-3 hairline panel" role="status">
          <Icon name="puzzle" size={16} className="mt-0.5 shrink-0 text-accent-text" />
          <div className="flex min-w-0 flex-col gap-2">
            <p className="text-[13px] font-medium">{t("access.title")}</p>
            <p className="text-[12px] leading-5 text-fg-muted">
              {denied ? t("access.denied") : t("access.body")}
            </p>
            <Button variant="primary" size="sm" className="self-start" onClick={grant}>
              {t("access.grant")}
            </Button>
          </div>
        </div>
      )}
      {state.paused ? (
        <p role="alert" className="text-[12px] leading-5 text-danger">
          {t("popup.paused")}
        </p>
      ) : null}
      <Input
        type="search"
        size="lg"
        pill
        icon="search"
        keys={searching ? undefined : "/"}
        placeholder={t("popup.searchPlaceholder")}
        aria-label={t("popup.searchPlaceholder")}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          setActive(0);
        }}
      />
      {http ? (
        <HttpConfirm onFill={() => fill(http, true)} onCancel={() => answer(http)} />
      ) : searching ? (
        <SearchResults
          text={text}
          items={items}
          active={active}
          results={results}
          error={page.search?.text === text ? page.search.error : null}
          onFill={fill}
          onCopy={copy}
          fillable={site?.fillable ?? false}
        />
      ) : (
        <SiteMatches
          state={state}
          active={active}
          results={results}
          onFill={fill}
          onCopy={copy}
          onOpen={() => send({ kind: "start", show: true })}
        />
      )}
    </div>
  );
}

function HttpConfirm({ onFill, onCancel }: { onFill: () => void; onCancel: () => void }) {
  const t = useText();
  return (
    <div
      className="flex gap-3 rounded-10 bg-surface p-4 hairline panel"
      role="alertdialog"
      aria-label={t("http.title")}>
      <Icon name="alert" size={16} className="mt-0.5 shrink-0 text-warning" />
      <div className="flex min-w-0 flex-col gap-2">
        <p className="text-[13px] font-semibold">{t("http.title")}</p>
        <p className="text-[12px] leading-5 text-fg-muted">{t("http.body")}</p>
        <div className="flex gap-2">
          <Button size="sm" onClick={onFill}>
            {t("http.fill")}
          </Button>
          <Button size="sm" variant="ghost" onClick={onCancel}>
            {t("http.cancel")}
          </Button>
        </div>
      </div>
    </div>
  );
}

interface RowsProps {
  active: number;
  results: Page["results"];
  onFill: (item: ItemSummary) => void;
  onCopy: (item: ItemSummary, part: "username" | "password" | "totp") => void;
}

function SiteMatches({
  state,
  onOpen,
  ...rows
}: RowsProps & { state: PopupState; onOpen: () => void }) {
  const t = useText();
  const site = state.site;
  if (!site)
    return <p className="py-6 text-center text-[13px] text-fg-muted">{t("popup.loading")}</p>;
  if (!site.fillable)
    return <p className="py-6 text-center text-[13px] text-fg-muted">{t("popup.noSite")}</p>;
  const header = (
    <div className="flex h-6 items-center gap-2">
      <Icon name="globe" size={16} className="shrink-0 text-fg-muted" />
      <span className="min-w-0 truncate text-[14px] font-semibold" title={site.site}>
        {site.site}
      </span>
      <span className="ml-auto shrink-0 text-[12px] text-fg-subtle">
        {site.items.length === 0
          ? t("popup.noMatch")
          : t("popup.matches", { n: site.items.length })}
      </span>
    </div>
  );
  if (site.neverFill || site.items.length === 0) {
    return (
      <div className="flex flex-1 flex-col gap-3">
        {header}
        {site.neverFill ? (
          <p className="text-[12px] text-fg-muted">{t("popup.neverFill")}</p>
        ) : (
          <Centered
            icon="lock"
            title={t("noMatch.title")}
            body={t("noMatch.body")}
            action={
              <Button variant="primary" icon="plus" onClick={onOpen}>
                {t("popup.openLockra")}
              </Button>
            }
          />
        )}
      </div>
    );
  }
  const hasCode = site.items.some((item) => item.has_totp);
  return (
    <div className="flex flex-col gap-2">
      {header}
      {site.crossOrigin && state.policy?.cross_origin_frames !== "ask" ? (
        <p className="text-[12px] text-fg-muted">{t("popup.crossOrigin")}</p>
      ) : null}
      <Card padding="none" className="p-1.5">
        {site.items.map((item, index) => (
          <MatchRow
            key={item.item_id}
            item={item}
            mode="fill"
            isActive={index === rows.active}
            {...rows}
          />
        ))}
      </Card>
      {state.pageAccess && hasCode && state.policy?.fill_otp ? (
        <p className="pt-2 text-[12px] text-fg-subtle">{t("popup.otpHint")}</p>
      ) : null}
      <Outcomes items={site.items} results={rows.results} />
    </div>
  );
}

function SearchResults({
  text,
  items,
  error,
  fillable,
  ...rows
}: RowsProps & { text: string; items: ItemSummary[]; error: string | null; fillable: boolean }) {
  const t = useText();
  if (error) return <p className="text-[12px] text-danger">{refusalText(t, error)}</p>;
  if (items.length === 0)
    return (
      <p className="py-6 text-center text-[13px] text-fg-muted">
        {t("popup.searchEmpty", { text })}
      </p>
    );
  return (
    <div className="flex flex-col gap-2">
      <p className="text-[12px] font-medium text-fg-muted">
        {t("popup.results", { n: items.length })}
      </p>
      <Card padding="none" className="p-1.5">
        {items.map((item, index) => (
          <MatchRow
            key={item.item_id}
            item={item}
            mode={item.matches && fillable ? "fill" : "copy"}
            isActive={index === rows.active}
            {...rows}
          />
        ))}
      </Card>
      <p className="pt-1 text-[12px] leading-normal text-fg-subtle">{t("popup.searchHint")}</p>
      <Outcomes items={items} results={rows.results} />
    </div>
  );
}

/** What became of the last fill or copy: Lockra asks first, it copied, or why not. */
function Outcomes({ items, results }: { items: ItemSummary[]; results: Page["results"] }) {
  const t = useText();
  const lines = items.flatMap((item) => {
    const result = results[item.item_id];
    if (!result || (result.outcome === "done" && result.action === "fill")) return [];
    if (result.outcome === "waiting")
      return [{ id: item.item_id, text: t("popup.waiting"), tone: "muted" }];
    if (result.outcome === "done")
      return [{ id: item.item_id, text: t("popup.copied"), tone: "muted" }];
    if (result.outcome === "http_downgrade") return [];
    return [{ id: item.item_id, text: refusalText(t, result.outcome), tone: "danger" }];
  });
  return (
    <>
      {lines.map((line) => (
        <p
          key={line.id}
          role="status"
          className={cx("text-[12px]", line.tone === "danger" ? "text-danger" : "text-fg-muted")}>
          {line.text}
        </p>
      ))}
    </>
  );
}

function MatchRow({
  item,
  mode,
  isActive,
  onFill,
  onCopy,
}: RowsProps & { item: ItemSummary; mode: "fill" | "copy"; isActive: boolean }) {
  const t = useText();
  const user = item.username || item.email;
  const actions = [
    ...(user
      ? [
          {
            kind: "action" as const,
            id: "username",
            label: t("popup.copyUsername"),
            icon: "user" as const,
          },
        ]
      : []),
    ...(item.has_password
      ? [
          {
            kind: "action" as const,
            id: "password",
            label: t("popup.copyPassword"),
            icon: "field" as const,
          },
        ]
      : []),
    ...(item.has_totp
      ? [{ kind: "action" as const, id: "totp", label: t("popup.copyCode"), icon: "key" as const }]
      : []),
  ];
  return (
    <div
      className={cx(
        "flex h-[60px] items-center gap-3 rounded-6 px-2.5",
        isActive && "bg-nav-active panel",
      )}
      data-testid="match-row">
      <Avatar name={item.title} fallback={user} size={36} />
      <div className="min-w-0 flex-1">
        <p className="truncate text-[14px] font-semibold" title={item.title}>
          {item.title}
        </p>
        <p className="truncate text-[12px] text-fg-muted" title={user}>
          {mode === "copy" ? t("popup.notThisSite", { user }) : user}
        </p>
      </div>
      {mode === "fill" ? (
        <>
          {isActive ? <Keycap>↵</Keycap> : null}
          <Button variant={isActive ? "primary" : "outline"} size="sm" onClick={() => onFill(item)}>
            {t("popup.fill")}
          </Button>
        </>
      ) : (
        <>
          {user ? (
            <IconButton
              icon="user"
              label={t("popup.copyUsername")}
              onClick={() => onCopy(item, "username")}
            />
          ) : null}
          {item.has_password ? (
            <IconButton
              icon="field"
              label={t("popup.copyPassword")}
              onClick={() => onCopy(item, "password")}
            />
          ) : null}
        </>
      )}
      {actions.length > 0 ? (
        <Menu
          label={t("popup.more")}
          trigger={<Icon name="more" size={14} />}
          triggerClassName="inline-flex size-6 items-center justify-center rounded-6 text-fg-muted hover:bg-inset hover:text-fg"
          align="end"
          sections={[{ items: actions }]}
          onSelect={(id) =>
            onCopy(item, id === "username" ? "username" : id === "totp" ? "totp" : "password")
          }
        />
      ) : null}
    </div>
  );
}
