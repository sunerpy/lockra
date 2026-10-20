// The detail pane of the passwords page (board 05): the login selected, its sign-in (user name,
// email, password, the linked account's code), its sites, notes and custom fields, how it is
// organized and protected, and its record. Secrets show only on request (`item_reveal`, after the
// proof a 「填充前验证」 login asks for), hide themselves after two minutes, and end the secret view
// however they go; copying is Rust's (`item_copy`).
import {
  type ItemRevealed,
  type ItemView,
  type PastView,
  REVEAL_SECONDS,
  type Tri,
  entryLabel,
  itemAccount,
  itemName,
} from "@lockra/shared";
import {
  Badge,
  Banner,
  Button,
  Card,
  DetailSection,
  EmptyState,
  EntryAvatar,
  FieldRow,
  Icon,
  IconButton,
  KeyValueRow,
  Menu,
  type MenuSection,
  UrlRow,
  useClock,
  useCodes,
  useLocale,
  useT,
  useUiState,
} from "@lockra/ui";
import { useEffect, useState } from "react";
import { motionReduced } from "../../app/appearance";
import { useDispatch } from "../../app/dispatch";
import { useShell } from "../../app/shell-state";
import { CodePicker, LinkedCode } from "./CodePicker";
import { useItemActions } from "./actions";
import { useSecretView } from "./secret-view";

const DOTS = "••••••••••••";

/** The details of `item`, or what to do while none is selected. */
export function ItemDetail({ item }: { item: ItemView | undefined }) {
  const t = useT();
  if (item === undefined) {
    return (
      <Card data-testid="item-detail-empty">
        <EmptyState compact icon="lock" title={t("passwords.detail.emptyTitle")}>
          {t("passwords.detail.emptyBody")}
        </EmptyState>
      </Card>
    );
  }
  // A new login, a new pane: nothing revealed of the one before.
  return <DetailCard key={item.id} item={item} />;
}

/** What a login's pane shows of its secrets, and since when. */
interface Shown {
  at: number;
  notes?: string;
  fields: Record<string, string>;
  history?: PastView[];
}

function DetailCard({ item }: { item: ItemView }) {
  const t = useT();
  const locale = useLocale();
  const state = useUiState();
  const shell = useShell();
  const dispatch = useDispatch();
  const actions = useItemActions();
  const codes = useCodes();
  const now = useClock();
  const [shown, setShown] = useState<Shown | null>(null);
  const left =
    shown === null
      ? REVEAL_SECONDS
      : Math.max(0, REVEAL_SECONDS - Math.max(0, Math.floor((now - shown.at) / 1000)));
  // Whatever was shown hides itself two minutes after the last reveal, and the secret view ends
  // with it.
  useSecretView(shown !== null);
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
  // The code picker of the login's form, here without the form: a pick links at once. The shell
  // holds it, so an extension that asks to link a code to this login opens it too (Shell.tsx).
  const pickingCode = shell.codePicker === item.id && !item.read_only;
  const setPickingCode = (open: boolean) => shell.setCodePicker(open ? item.id : null);
  const linkCode = (entry: string | null) =>
    void dispatch({ command: "item_update", id: item.id, patch: { totp_entry: entry } });
  const code = linked === undefined ? undefined : codes.get(linked.id);
  const day = (ms: number) => new Intl.DateTimeFormat(locale, { dateStyle: "medium" }).format(ms);
  const tri = (own: Tri, policy: boolean) =>
    own === "default"
      ? t("item.autofill.defaultIs", {
          value: t(policy ? "item.autofill.on" : "item.autofill.off"),
        })
      : t(`item.autofill.${own}`);
  const policy = state.browser.policy;
  const conflict = item.conflicts[0];
  const other =
    conflict === undefined
      ? undefined
      : shown?.history?.find((p) => p.field === conflict.field && p.conflict);
  const settle = (take: string | undefined) => {
    if (conflict === undefined) return;
    void dispatch({
      command: "item_update",
      id: item.id,
      patch: {
        settle: [take === undefined ? { field: conflict.field } : { field: conflict.field, take }],
      },
    }).then(() => setShown(null));
  };
  const menuSections: MenuSection[] = [
    {
      items: [
        ...(item.has_password
          ? [
              {
                kind: "action" as const,
                id: "reveal",
                label: t("passwords.menu.reveal"),
                icon: "eye" as const,
              },
            ]
          : []),
        ...(item.read_only
          ? []
          : [
              {
                kind: "action" as const,
                id: "favorite",
                label: item.favorite
                  ? t("passwords.menu.unfavorite")
                  : t("passwords.menu.favorite"),
                icon: "star" as const,
              },
              {
                kind: "action" as const,
                id: "move",
                label: t("passwords.menu.move"),
                icon: "folder" as const,
              },
            ]),
      ],
    },
    { items: [{ kind: "action", id: "delete", label: t("passwords.menu.delete"), icon: "trash" }] },
  ];
  const onMenu = (id: string) => {
    if (id === "reveal") shell.open({ type: "item_reveal", id: item.id });
    else if (id === "favorite")
      void dispatch({ command: "item_update", id: item.id, patch: { favorite: !item.favorite } });
    else if (id === "move") shell.open({ type: "item_move", id: item.id });
    else shell.open({ type: "item_delete", id: item.id });
  };
  const subtitle = [item.group ?? "", item.tags.join("、")].filter((s) => s !== "").join(" · ");
  return (
    // A container: in the narrow pane of a 960 px window (board 03) the header is the compact one.
    <Card
      padding="none"
      className="@container max-h-[calc(100vh-40px-14px-16px)] overflow-y-auto p-4 [scrollbar-color:color-mix(in_srgb,var(--fg-subtle)_35%,transparent)_transparent] [scrollbar-width:thin]"
      data-testid="item-detail">
      <div aria-label={t("passwords.detail.label")} role="region">
        <div className="flex items-start gap-3">
          <span className="hidden shrink-0 @[300px]:block">
            <EntryAvatar
              issuer={name}
              account={itemAccount(item)}
              color={item.color}
              mark={item.mark}
              size={44}
            />
          </span>
          <span className="shrink-0 @[300px]:hidden">
            <EntryAvatar
              issuer={name}
              account={itemAccount(item)}
              color={item.color}
              mark={item.mark}
              size={36}
            />
          </span>
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5">
              <h2
                className="truncate text-[16px] leading-[1.3] font-semibold text-fg @[300px]:text-[18px]"
                title={name}>
                {name}
              </h2>
              {item.favorite && (
                <Icon name="star" size={14} fill="currentColor" className="shrink-0 text-warning" />
              )}
            </div>
            {subtitle !== "" && (
              <div className="truncate text-[12.5px] text-fg-muted">{subtitle}</div>
            )}
          </div>
          {!item.read_only && (
            <>
              <span className="hidden shrink-0 @[300px]:block">
                <Button
                  variant="outline"
                  size="sm"
                  icon="edit"
                  onClick={() => shell.open({ type: "item_edit", id: item.id })}
                  data-testid="item-edit">
                  {t("passwords.detail.edit")}
                </Button>
              </span>
              <span className="shrink-0 @[300px]:hidden">
                <IconButton
                  icon="edit"
                  label={t("passwords.detail.edit")}
                  size={28}
                  bordered
                  onClick={() => shell.open({ type: "item_edit", id: item.id })}
                  data-testid="item-edit-compact"
                />
              </span>
            </>
          )}
          <Menu
            label={t("passwords.detail.more")}
            title={t("passwords.detail.more")}
            align="end"
            sections={menuSections}
            onSelect={onMenu}
            data-testid="item-detail-menu"
            triggerClassName="inline-flex h-7 w-7 items-center justify-center rounded-6 text-fg-muted transition-colors hover:bg-inset2 hover:text-fg"
            trigger={<Icon name="more" size={16} />}
          />
        </div>
        {item.protection === "confirm" && (
          <p
            className="mt-2 flex items-center gap-1.5 text-[12px] text-fg-muted"
            data-testid="item-protected">
            <Icon name="shieldCheck" size={14} />
            {t("passwords.detail.protectedNote")}
          </p>
        )}
        {conflict !== undefined && (
          <Banner
            tone="warn"
            marker="bar"
            className="mt-3"
            title={t("passwords.detail.conflictTitle")}>
            <div className="flex flex-col gap-2" data-testid="item-conflict">
              <p>{t("passwords.detail.conflictBody")}</p>
              <div className="flex h-[30px] items-center gap-2 rounded-6 bg-surface px-2 hairline">
                <span className="mono min-w-0 flex-1 truncate text-[13px] text-fg">
                  {other === undefined ? DOTS : other.value}
                </span>
                {other === undefined && (
                  <Button
                    variant="text"
                    size="sm"
                    onClick={() => void show({ history: true })}
                    data-testid="item-conflict-show">
                    {t("passwords.detail.show")}
                  </Button>
                )}
              </div>
              <div className="flex gap-2">
                <Button
                  variant="primary"
                  size="sm"
                  disabled={other === undefined}
                  onClick={() => settle(other?.id)}
                  data-testid="item-conflict-take">
                  {t("passwords.detail.takeOther")}
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => settle(undefined)}
                  data-testid="item-conflict-keep">
                  {t("passwords.detail.keepCurrent")}
                </Button>
              </div>
            </div>
          </Banner>
        )}
        {item.read_only && (
          <Banner tone="neutral" marker="icon" className="mt-3">
            {t("passwords.detail.readOnlyNote")}
          </Banner>
        )}
        <div className="mt-4">
          <DetailSection title={t("passwords.detail.login")}>
            {item.username !== "" && (
              <FieldRow
                label={t("passwords.detail.username")}
                actions={
                  <IconButton
                    icon="copy"
                    label={t("item.copy.username")}
                    size={24}
                    onClick={() => void actions.copy(item, { type: "username" })}
                    data-testid="detail-copy-username"
                  />
                }>
                {item.username}
              </FieldRow>
            )}
            {item.email !== "" && (
              <FieldRow
                label={t("passwords.detail.email")}
                actions={
                  <IconButton
                    icon="copy"
                    label={t("item.copy.email")}
                    size={24}
                    onClick={() => void actions.copy(item, { type: "email" })}
                    data-testid="detail-copy-email"
                  />
                }>
                {item.email}
              </FieldRow>
            )}
            {item.has_password && (
              <FieldRow
                label={t("passwords.detail.password")}
                secret
                actions={
                  <>
                    <IconButton
                      icon="eye"
                      label={t("passwords.detail.show")}
                      size={24}
                      onClick={() => shell.open({ type: "item_reveal", id: item.id })}
                      data-testid="detail-reveal"
                    />
                    <IconButton
                      icon="copy"
                      label={t("item.copy.password")}
                      size={24}
                      onClick={() => void actions.copy(item, { type: "password" })}
                      data-testid="detail-copy-password"
                    />
                  </>
                }>
                {DOTS}
              </FieldRow>
            )}
            {linked !== undefined ? (
              <FieldRow
                label={t("passwords.detail.code", {
                  name: entryLabel(linked.issuer, linked.account),
                })}
                actions={
                  <>
                    <IconButton
                      icon="copy"
                      label={t("item.copy.code")}
                      size={24}
                      onClick={() => void actions.copy(item, { type: "totp" })}
                      data-testid="detail-copy-code"
                    />
                    {!item.read_only && (
                      <>
                        <IconButton
                          icon="link"
                          label={t("passwords.detail.codeChange")}
                          size={24}
                          onClick={() => setPickingCode(true)}
                          data-testid="detail-code-change"
                        />
                        <IconButton
                          icon="x"
                          label={t("passwords.detail.codeUnlink")}
                          size={24}
                          onClick={() => linkCode(null)}
                          data-testid="detail-code-unlink"
                        />
                      </>
                    )}
                  </>
                }
                data-testid="detail-code">
                <LinkedCode code={code} now={now} still={motionReduced(state.settings)} />
              </FieldRow>
            ) : (
              !item.read_only && (
                <FieldRow
                  label={t("item.code.label")}
                  actions={
                    <Button
                      size="sm"
                      variant="ghost"
                      icon="key"
                      onClick={() => setPickingCode(true)}
                      data-testid="detail-code-link">
                      {t("item.code.link")}
                    </Button>
                  }
                  data-testid="detail-no-code">
                  <span className="text-[12px] text-fg-subtle">{t("passwords.detail.noCode")}</span>
                </FieldRow>
              )
            )}
            {pickingCode && (
              <CodePicker
                entries={state.entries}
                onPick={(id) => {
                  setPickingCode(false);
                  linkCode(id);
                }}
                onClose={() => setPickingCode(false)}
              />
            )}
          </DetailSection>
          {item.urls.length > 0 && (
            <DetailSection title={t("passwords.detail.sites")}>
              {item.urls.map((u, i) => (
                <UrlRow key={`${i}:${u.url}`} url={u.url} matching={u.matching} />
              ))}
            </DetailSection>
          )}
          {item.has_notes && (
            <DetailSection
              title={t("passwords.detail.notes")}
              aside={
                shown?.notes !== undefined ? (
                  <span className="text-[11px] text-fg-subtle">
                    {t("passwords.detail.hideIn", { s: left })}
                  </span>
                ) : undefined
              }>
              {shown?.notes === undefined ? (
                <div className="flex min-h-11 items-center gap-2 py-1.5">
                  <span className="mono flex-1 text-[13px] tracking-[1.5px] text-fg-muted">
                    {DOTS}
                  </span>
                  <Button
                    variant="text"
                    size="sm"
                    onClick={() => void show({ notes: true })}
                    data-testid="detail-notes-show">
                    {t("passwords.detail.show")}
                  </Button>
                </div>
              ) : (
                <p
                  className="py-2 text-[13px] leading-5 whitespace-pre-wrap text-fg select-text"
                  data-testid="detail-notes">
                  {shown.notes}
                </p>
              )}
            </DetailSection>
          )}
          {item.fields.length > 0 && (
            <DetailSection title={t("passwords.detail.fields")}>
              {item.fields.map((field) => {
                const value = field.hidden ? shown?.fields[field.id] : (field.value ?? "");
                return (
                  <FieldRow
                    key={field.id}
                    label={field.label}
                    secret={field.hidden}
                    actions={
                      <>
                        {field.hidden && value === undefined && field.has_value && (
                          <IconButton
                            icon="eye"
                            label={t("passwords.detail.show")}
                            size={24}
                            onClick={() => void show({ fields: [field.id] })}
                            data-testid="detail-field-show"
                          />
                        )}
                        {field.has_value && (
                          <IconButton
                            icon="copy"
                            label={t("item.copy.field", { name: field.label })}
                            size={24}
                            onClick={() => void actions.copy(item, { type: "field", id: field.id })}
                            data-testid="detail-field-copy"
                          />
                        )}
                      </>
                    }>
                    {value ?? DOTS}
                  </FieldRow>
                );
              })}
            </DetailSection>
          )}
          <DetailSection title={t("passwords.detail.organize")}>
            <KeyValueRow label={t("passwords.detail.group")}>
              {item.group ?? t("passwords.detail.noGroup")}
            </KeyValueRow>
            {item.tags.length > 0 && (
              <KeyValueRow label={t("passwords.detail.tags")}>
                <span className="inline-flex flex-wrap justify-end gap-1">
                  {item.tags.map((tag) => (
                    <Badge key={tag} tone="neutral">
                      {tag}
                    </Badge>
                  ))}
                </span>
              </KeyValueRow>
            )}
          </DetailSection>
          <DetailSection title={t("passwords.detail.fill")}>
            <KeyValueRow label={t("passwords.detail.protection")}>
              {t(`item.protection.${item.protection}`)}
            </KeyValueRow>
            <KeyValueRow label={t("item.autofill.on_page_load")}>
              {tri(item.autofill.on_page_load, policy.on_page_load)}
            </KeyValueRow>
            <KeyValueRow label={t("item.autofill.auto_submit")}>
              {tri(item.autofill.auto_submit, policy.auto_submit)}
            </KeyValueRow>
            <KeyValueRow label={t("item.autofill.fill_otp")}>
              {tri(item.autofill.fill_otp, policy.fill_otp)}
            </KeyValueRow>
          </DetailSection>
          <DetailSection title={t("passwords.detail.record")}>
            <KeyValueRow label={t("passwords.detail.history")}>
              {item.history_count === 0
                ? t("passwords.detail.historyNone")
                : t("passwords.detail.historyCount", { n: item.history_count })}
            </KeyValueRow>
            <KeyValueRow label={t("passwords.detail.created")}>
              {day(item.created_at_ms)}
            </KeyValueRow>
            <KeyValueRow label={t("passwords.detail.modified")}>
              {day(item.updated_at_ms)}
            </KeyValueRow>
            {item.source !== null && (
              <KeyValueRow label={t("passwords.detail.source")}>{item.source}</KeyValueRow>
            )}
          </DetailSection>
        </div>
      </div>
    </Card>
  );
}
