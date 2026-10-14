// A login (boards 24, 25): its sign-in (user name, email, password, the linked account's code), its
// sites, notes and custom fields, how it is organized and protected, and its record. The notes and
// the hidden fields show on request (`item_reveal`, after the proof a 「填充前验证」 login asks for)
// and hide themselves after two minutes; the password shows on a page of its own. Copying is
// Rust's (`item_copy`); the bar's ⋯ opens the login's actions.
import {
  type ItemRevealed,
  type ItemView,
  type PastView,
  REVEAL_SECONDS,
  entryLabel,
  itemAccount,
  itemName,
} from "@lockra/shared";
import {
  Badge,
  Banner,
  Button,
  EntryAvatar,
  FieldRow,
  Icon,
  IconButton,
  KeyValueRow,
  UrlRow,
  useBackend,
  useClock,
  useDispatch,
  useLocale,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { useNav } from "../app/nav";
import { useItemActions } from "../app/verify";
import { DeleteLoginDialog, LoginCode, LoginSection } from "../components/LoginParts";
import { LoginSheet } from "../components/LoginSheet";
import { Page } from "../components/Page";

const DOTS = "••••••••••••";

/** What the page shows of the login's secrets, and since when. */
interface Shown {
  at: number;
  notes?: string;
  fields: Record<string, string>;
  history?: PastView[];
}

/** A copy button of a row (40 px, the phone's touch size). */
function copyButton(label: string, onClick: () => void, testId: string) {
  return <IconButton icon="copy" label={label} size={40} onClick={onClick} data-testid={testId} />;
}

export function Login({ item }: { item: ItemView }) {
  const t = useT();
  const locale = useLocale();
  const nav = useNav();
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const actions = useItemActions();
  const state = useUiState();
  const now = useClock();
  const [shown, setShown] = useState<Shown | null>(null);
  const [sheet, setSheet] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const left =
    shown === null
      ? REVEAL_SECONDS
      : Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - shown.at) / 1000)));
  // Whatever shows hides itself two minutes after the last reveal; the core hears that the secret
  // view ended, however it goes.
  const showing = shown !== null;
  useEffect(() => {
    if (!showing) return undefined;
    return () => {
      void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    };
  }, [showing, backend]);
  useEffect(() => {
    if (shown === null) return undefined;
    const timer = setTimeout(
      () => setShown(null),
      Math.max(0, shown.at + REVEAL_SECONDS * 1000 - Date.now()),
    );
    return () => clearTimeout(timer);
  }, [shown]);
  const show = async (parts: { notes?: boolean; fields?: string[]; history?: boolean }) => {
    const answer: ItemRevealed | undefined = await actions.reveal(item, parts);
    if (answer === undefined) return;
    setShown((current) => ({
      at: Date.now(),
      notes: answer.notes ?? current?.notes,
      fields: {
        ...current?.fields,
        ...Object.fromEntries(answer.fields.map((f) => [f.id, f.value])),
      },
      history: answer.history ?? current?.history,
    }));
  };
  const name = itemName(item);
  const linked =
    item.totp_entry === null ? undefined : state.entries.find((e) => e.id === item.totp_entry);
  const day = (ms: number) => new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(ms);
  const conflict = item.conflicts[0];
  const other =
    conflict === undefined
      ? undefined
      : shown?.history?.find((p) => p.field === conflict.field && p.conflict);
  const settle = (take: string | undefined) => {
    if (conflict === undefined) return;
    const choice = take === undefined ? { field: conflict.field } : { field: conflict.field, take };
    void dispatch({ command: "item_update", id: item.id, patch: { settle: [choice] } }).then(() =>
      setShown(null),
    );
  };
  const subtitle = [item.group ?? "", item.tags.join("、")].filter((s) => s !== "").join(" · ");
  return (
    <Page
      title={name}
      testId="page-login"
      actions={
        <>
          {!item.read_only && (
            <Button
              variant="text"
              onClick={() => nav.open({ name: "loginEdit", id: item.id })}
              data-testid="login-edit">
              {t("passwords.detail.edit")}
            </Button>
          )}
          <IconButton
            icon="more"
            label={t("passwords.detail.more")}
            size={40}
            onClick={() => setSheet(true)}
            data-testid="login-menu"
          />
        </>
      }>
      <div className="flex flex-col gap-5">
        <div className="flex items-center gap-3 px-1">
          <EntryAvatar
            issuer={name}
            account={itemAccount(item)}
            color={item.color}
            mark={item.mark}
            size={44}
          />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5">
              <h2 className="truncate text-[20px] leading-tight font-semibold text-fg">{name}</h2>
              {item.favorite && (
                <Icon name="star" size={16} fill="currentColor" className="shrink-0 text-warning" />
              )}
            </div>
            {subtitle !== "" && (
              <div className="truncate text-[13px] text-fg-muted">{subtitle}</div>
            )}
          </div>
        </div>
        {item.protection === "confirm" && (
          <p
            className="-mt-2 flex items-center gap-1.5 px-1 text-[13px] text-fg-muted"
            data-testid="login-protected">
            <Icon name="shieldCheck" size={16} />
            {t("passwords.detail.protectedNote")}
          </p>
        )}
        {conflict !== undefined && (
          <Banner tone="warn" marker="bar" title={t("passwords.detail.conflictTitle")}>
            <div className="flex flex-col gap-2" data-testid="login-conflict">
              <p>{t("passwords.detail.conflictBody")}</p>
              <div className="flex min-h-10 items-center gap-2 rounded-6 bg-surface px-2 hairline">
                <span className="mono min-w-0 flex-1 truncate text-[14px] text-fg">
                  {other === undefined ? DOTS : other.value}
                </span>
                {other === undefined && (
                  <Button
                    variant="text"
                    onClick={() => void show({ history: true })}
                    data-testid="login-conflict-show">
                    {t("passwords.detail.show")}
                  </Button>
                )}
              </div>
              <div className="flex gap-2">
                <Button
                  variant="primary"
                  size="lg"
                  disabled={other === undefined}
                  onClick={() => settle(other?.id)}
                  data-testid="login-conflict-take">
                  {t("passwords.detail.takeOther")}
                </Button>
                <Button
                  variant="outline"
                  size="lg"
                  onClick={() => settle(undefined)}
                  data-testid="login-conflict-keep">
                  {t("passwords.detail.keepCurrent")}
                </Button>
              </div>
            </div>
          </Banner>
        )}
        {item.read_only && (
          <Banner tone="neutral" marker="icon">
            {t("passwords.detail.readOnlyNote")}
          </Banner>
        )}
        <LoginSection title={t("passwords.detail.login")}>
          {item.username !== "" && (
            <FieldRow
              size="lg"
              label={t("passwords.detail.username")}
              actions={copyButton(
                t("item.copy.username"),
                () => void actions.copy(item, { type: "username" }),
                "login-copy-username",
              )}>
              {item.username}
            </FieldRow>
          )}
          {item.email !== "" && (
            <FieldRow
              size="lg"
              label={t("passwords.detail.email")}
              actions={copyButton(
                t("item.copy.email"),
                () => void actions.copy(item, { type: "email" }),
                "login-copy-email",
              )}>
              {item.email}
            </FieldRow>
          )}
          {item.has_password && (
            <FieldRow
              size="lg"
              secret
              label={t("passwords.detail.password")}
              actions={
                <>
                  <IconButton
                    icon="eye"
                    label={t("mobile.logins.revealTitle")}
                    size={40}
                    onClick={() => nav.open({ name: "loginReveal", id: item.id })}
                    data-testid="login-reveal"
                  />
                  {copyButton(
                    t("item.copy.password"),
                    () => void actions.copy(item, { type: "password" }),
                    "login-copy-password",
                  )}
                </>
              }>
              {DOTS}
            </FieldRow>
          )}
          {linked !== undefined && (
            <div className="flex min-h-18 items-center gap-2 py-2.5" data-testid="login-code-row">
              <div className="min-w-0 flex-1">
                <div className="truncate text-[12px] text-fg-subtle">
                  {t("passwords.detail.code", { name: entryLabel(linked.issuer, linked.account) })}
                </div>
                <LoginCode entryId={linked.id} />
              </div>
              {copyButton(
                t("item.copy.code"),
                () => void actions.copy(item, { type: "totp" }),
                "login-copy-code",
              )}
            </div>
          )}
        </LoginSection>
        {item.urls.length > 0 && (
          <LoginSection title={t("passwords.detail.sites")}>
            {item.urls.map((u, i) => (
              <UrlRow key={`${i}:${u.url}`} size="lg" url={u.url} matching={u.matching} />
            ))}
          </LoginSection>
        )}
        {item.has_notes && (
          <LoginSection
            title={t("passwords.detail.notes")}
            aside={
              shown?.notes === undefined ? undefined : (
                <span className="text-[12px] text-fg-subtle">
                  {t("passwords.detail.hideIn", { s: left })}
                </span>
              )
            }>
            {shown?.notes === undefined ? (
              <div className="flex min-h-[49px] items-center gap-2">
                <span className="mono flex-1 text-[14px] tracking-[1.5px] text-fg-muted">
                  {DOTS}
                </span>
                <Button
                  variant="text"
                  onClick={() => void show({ notes: true })}
                  data-testid="login-notes-show">
                  {t("passwords.detail.show")}
                </Button>
              </div>
            ) : (
              <p
                className="py-3 text-[15px] leading-6 whitespace-pre-wrap text-fg select-text"
                data-testid="login-notes">
                {shown.notes}
              </p>
            )}
          </LoginSection>
        )}
        {item.fields.length > 0 && (
          <LoginSection title={t("passwords.detail.fields")}>
            {item.fields.map((field) => {
              const value = field.hidden ? shown?.fields[field.id] : (field.value ?? "");
              return (
                <FieldRow
                  key={field.id}
                  size="lg"
                  label={field.label}
                  secret={field.hidden}
                  actions={
                    <>
                      {field.hidden && value === undefined && field.has_value && (
                        <IconButton
                          icon="eye"
                          label={t("passwords.detail.show")}
                          size={40}
                          onClick={() => void show({ fields: [field.id] })}
                          data-testid="login-field-show"
                        />
                      )}
                      {field.has_value &&
                        copyButton(
                          t("item.copy.field", { name: field.label }),
                          () => void actions.copy(item, { type: "field", id: field.id }),
                          "login-field-copy",
                        )}
                    </>
                  }>
                  {value ?? DOTS}
                </FieldRow>
              );
            })}
          </LoginSection>
        )}
        <LoginSection title={t("passwords.detail.organize")}>
          <KeyValueRow size="lg" label={t("passwords.detail.group")}>
            {item.group ?? t("passwords.detail.noGroup")}
          </KeyValueRow>
          {item.tags.length > 0 && (
            <KeyValueRow size="lg" label={t("passwords.detail.tags")}>
              <span className="inline-flex flex-wrap justify-end gap-1">
                {item.tags.map((tag) => (
                  <Badge key={tag} tone="neutral">
                    {tag}
                  </Badge>
                ))}
              </span>
            </KeyValueRow>
          )}
        </LoginSection>
        <LoginSection title={t("passwords.detail.fill")}>
          <KeyValueRow size="lg" label={t("passwords.detail.protection")}>
            {t(`item.protection.${item.protection}`)}
          </KeyValueRow>
          <KeyValueRow size="lg" label={t("mobile.logins.autofill")}>
            {t("mobile.logins.autofillDesktop")}
          </KeyValueRow>
        </LoginSection>
        <LoginSection title={t("passwords.detail.record")}>
          <KeyValueRow size="lg" label={t("passwords.detail.history")}>
            {item.history_count === 0
              ? t("passwords.detail.historyNone")
              : t("passwords.detail.historyCount", { n: item.history_count })}
          </KeyValueRow>
          <KeyValueRow size="lg" label={t("passwords.detail.created")}>
            {day(item.created_at_ms)}
          </KeyValueRow>
          <KeyValueRow size="lg" label={t("passwords.detail.modified")}>
            {day(item.updated_at_ms)}
          </KeyValueRow>
          {item.source !== null && (
            <KeyValueRow size="lg" label={t("passwords.detail.source")}>
              {item.source}
            </KeyValueRow>
          )}
        </LoginSection>
      </div>
      {sheet && (
        <LoginSheet
          item={item}
          onClose={() => setSheet(false)}
          onDelete={() => setDeleting(true)}
        />
      )}
      {deleting && (
        <DeleteLoginDialog
          item={item}
          onClose={() => setDeleting(false)}
          onDeleted={() => setDeleting(false)}
        />
      )}
    </Page>
  );
}
