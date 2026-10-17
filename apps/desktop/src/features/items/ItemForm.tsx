// A login's form (boards 06, 07): new or edited, in one dialog whose title and buttons stay while
// the body scrolls. The secrets of a login being edited never come to the form by themselves: the
// password and the hidden fields stay unless something is typed, and the notes come only when the
// user asks to edit them (`item_reveal`, after the proof a 「填充前验证」 login asks for). A password
// made by the generator is saved as the generator's (`{"type": "generated"}`), so it does not go
// back from the webview.
import {
  type EntryView,
  type ItemView,
  type LoginFieldDraft,
  type LoginForm,
  MATCHINGS,
  type Matching,
  blankField,
  blankLoginForm,
  blankUrl,
  errorText,
  itemGroups,
  itemTags,
  loginDraft,
  loginFormHost,
  loginFormOf,
  loginPatch,
  loginProblems,
  typedPassword,
} from "@lockra/shared";
import {
  AccountAppearance,
  Banner,
  Button,
  Dialog,
  EntryAvatar,
  GroupCombobox,
  IconButton,
  Input,
  Menu,
  PasswordField,
  ProtectionChoice,
  Segmented,
  TagsInput,
  Textarea,
  Toggle,
  useBackend,
  useClock,
  useCodes,
  useItems,
  useT,
  useUiState,
} from "@lockra/ui";
import {
  type ReactNode,
  type SubmitEvent,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { motionReduced } from "../../app/appearance";
import { useSubmit } from "../../app/dispatch";
import { CodePicker, LinkedCode } from "./CodePicker";
import { GeneratorDialog } from "./GeneratorDialog";
import { useItemActions } from "./actions";
import { useSecretView } from "./secret-view";

const AUTOFILL_PREFS = ["on_page_load", "auto_submit", "fill_otp"] as const;
const TRIS = ["default", "on", "off"] as const;

export interface ItemFormDialogProps {
  /** The login being edited; none for a new one. */
  item?: ItemView;
  /** A new login's title (from a search that found nothing). */
  title?: string;
  onClose: () => void;
  /** The new login's id, once saved. */
  onSaved?: (id: string) => void;
  /** 「删除…」 of a login being edited. */
  onDelete?: () => void;
}

export function ItemFormDialog({
  item,
  title = "",
  onClose,
  onSaved,
  onDelete,
}: ItemFormDialogProps) {
  const t = useT();
  const { backend } = useBackend();
  const state = useUiState();
  const items = useItems();
  const actions = useItemActions();
  const formId = useId();
  const submit = useSubmit();
  const formRef = useRef<HTMLFormElement>(null);
  const [form, setForm] = useState<LoginForm>(() =>
    item ? loginFormOf(item) : blankLoginForm(title),
  );
  const [tried, setTried] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const [generating, setGenerating] = useState(false);
  const [notesShown, setNotesShown] = useState(false);
  const groups = useMemo(() => itemGroups(items ?? []), [items]);
  const tagSuggestions = useMemo(() => itemTags(items ?? []).map((c) => c.tag), [items]);
  const set = (change: Partial<LoginForm>) => setForm((f) => ({ ...f, ...change }));
  const found = loginProblems(form);
  const { count } = found;
  const firstHost = loginFormHost(form);

  // The notes shown for editing are a secret view; it ends with the dialog, however it goes.
  useSecretView(notesShown);
  // A save that found problems goes to the first of them, once they are drawn (the focus scrolls
  // the dialog's body to it).
  useEffect(() => {
    if (attempt === 0) return;
    formRef.current?.querySelector<HTMLElement>('[aria-invalid="true"]')?.focus();
  }, [attempt]);

  const loadNotes = async () => {
    if (item === undefined) return;
    const answer = await actions.reveal(item, { notes: true });
    if (answer === undefined) return;
    setNotesShown(true);
    set({ notes: answer.notes ?? "" });
  };
  const openGenerator = () => {
    // The generator keeps only the password it made last: one made before is typed in now, so
    // that a second visit that ends without 「使用此密码」 cannot change what is saved.
    if (form.password.kind === "generated")
      set({ password: { kind: "set", value: form.password.value } });
    setGenerating(true);
  };

  const save = async (event: SubmitEvent) => {
    event.preventDefault();
    setTried(true);
    if (count > 0) {
      setAttempt((n) => n + 1);
      return;
    }
    if (item === undefined) {
      const added = await submit.run(() =>
        backend.dispatch({ command: "item_add", draft: loginDraft(form) }),
      );
      if (added === undefined) return;
      onSaved?.(added.id);
      onClose();
      return;
    }
    const done = await submit.run(() =>
      backend.dispatch({ command: "item_update", id: item.id, patch: loginPatch(form) }),
    );
    if (done !== undefined) onClose();
  };

  const linked = form.totp === null ? undefined : state.entries.find((e) => e.id === form.totp);
  const keeping = form.password.kind === "keep";
  return (
    <>
      <Dialog
        open
        scroll
        width={600}
        title={item === undefined ? t("item.newTitle") : t("item.editTitle")}
        onClose={onClose}
        actions={
          <>
            {item !== undefined && onDelete !== undefined && (
              <Button variant="text-danger" onClick={onDelete} data-testid="item-form-delete">
                {t("item.delete")}
              </Button>
            )}
            <Button variant="ghost" onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              type="submit"
              form={formId}
              loading={submit.busy}
              disabled={form.title.trim() === ""}
              data-testid="item-form-save">
              {t("item.save")}
            </Button>
          </>
        }>
        <form
          ref={formRef}
          id={formId}
          onSubmit={(e) => void save(e)}
          noValidate
          className="flex flex-col gap-3.5 text-fg"
          data-testid="item-form">
          {tried && count > 0 && (
            <Banner
              tone="danger"
              marker="bar"
              title={t("item.errors.title", { n: count })}
              data-testid="item-form-errors"
            />
          )}
          {submit.error !== undefined && (
            <Banner tone="danger" marker="bar">
              {errorText(t, submit.error)}
            </Banner>
          )}
          <div className="flex items-start gap-3">
            <EntryAvatar
              issuer={form.title || form.username || "?"}
              account={form.username}
              color={form.color}
              mark={form.mark === "" ? null : form.mark}
              size={40}
            />
            <Input
              aria-label={t("item.title")}
              placeholder={t("item.titlePlaceholder")}
              value={form.title}
              onChange={(e) => set({ title: e.target.value })}
              error={tried && found.title ? t("item.errors.title_required") : undefined}
              className="flex-1 self-center"
              data-autofocus
              data-testid="item-form-title"
            />
          </div>
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3.5">
            <Input
              label={t("item.username")}
              value={form.username}
              onChange={(e) => set({ username: e.target.value })}
              autoComplete="off"
              data-testid="item-form-username"
            />
            <Input
              label={t("item.email")}
              type="email"
              value={form.email}
              onChange={(e) => set({ email: e.target.value })}
              autoComplete="off"
              data-testid="item-form-email"
            />
          </div>
          <div className="flex items-end gap-2">
            <PasswordField
              label={t("item.password")}
              value={form.password.kind === "keep" ? "" : form.password.value}
              // Emptied while editing: the password stays as it is, never lost to a slip of the
              // backspace key.
              onChange={(value) => set({ password: typedPassword(value, item !== undefined) })}
              placeholder={keeping ? t("item.fields.keep") : undefined}
              strength={!keeping}
              autoComplete="new-password"
              className="flex-1"
              data-testid="item-form-password"
            />
            <Button icon="dice" onClick={openGenerator} data-testid="item-form-generate">
              {t("item.generate")}
            </Button>
          </div>
          <FormGroup title={t("item.sites.label")} help={t("item.sites.help")}>
            {form.urls.map((row) => (
              <div key={row.key} className="flex items-start gap-2" data-testid="item-form-url">
                <Input
                  aria-label={t("item.sites.label")}
                  placeholder={t("item.sites.placeholder")}
                  value={row.url}
                  mono={row.matching === "regex"}
                  onChange={(e) =>
                    set({
                      urls: form.urls.map((u) =>
                        u.key === row.key ? { ...u, url: e.target.value } : u,
                      ),
                    })
                  }
                  error={
                    tried && found.urls.has(row.key) ? t("item.errors.url_invalid") : undefined
                  }
                  className="flex-1"
                />
                <MatchingMenu
                  value={row.matching}
                  onChange={(matching) =>
                    set({
                      urls: form.urls.map((u) => (u.key === row.key ? { ...u, matching } : u)),
                    })
                  }
                />
                <IconButton
                  icon="trash"
                  tone="danger"
                  label={t("item.sites.remove")}
                  size={28}
                  disabled={form.urls.length === 1 && row.url === ""}
                  onClick={() =>
                    set({
                      urls:
                        form.urls.length === 1
                          ? [blankUrl()]
                          : form.urls.filter((u) => u.key !== row.key),
                    })
                  }
                />
              </div>
            ))}
            <Button
              variant="text"
              size="sm"
              icon="plus"
              className="self-start"
              onClick={() => set({ urls: [...form.urls, blankUrl()] })}
              data-testid="item-form-add-url">
              {t("item.sites.add")}
            </Button>
          </FormGroup>
          <FormGroup title={t("item.code.label")}>
            <CodeLink linked={linked} entries={state.entries} onChange={(totp) => set({ totp })} />
          </FormGroup>
          <FormGroup title={t("item.notes.label")} aside={t("item.notes.secret")}>
            {form.notes === undefined ? (
              <Button
                variant="outline"
                size="sm"
                icon="eye"
                className="self-start"
                onClick={() => void loadNotes()}
                data-testid="item-form-notes-load">
                {t("passwords.detail.show")}
              </Button>
            ) : (
              <Textarea
                aria-label={t("item.notes.label")}
                value={form.notes}
                onChange={(e) => set({ notes: e.target.value })}
                className="h-[72px]"
                data-testid="item-form-notes"
              />
            )}
          </FormGroup>
          <FormGroup title={t("item.fields.label")}>
            {form.fields
              .filter((f) => !f.removed)
              .map((field) => (
                <CustomField
                  key={field.key}
                  field={field}
                  onChange={(patch) =>
                    set({
                      fields: form.fields.map((f) =>
                        f.key === field.key ? { ...f, ...patch } : f,
                      ),
                    })
                  }
                />
              ))}
            <Button
              variant="text"
              size="sm"
              icon="plus"
              className="self-start"
              onClick={() =>
                set({
                  fields: [...form.fields, blankField()],
                })
              }
              data-testid="item-form-add-field">
              {t("item.fields.add")}
            </Button>
          </FormGroup>
          <FormGroup title={t("item.organize.label")}>
            <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3.5">
              <GroupCombobox
                label={t("item.organize.group")}
                value={form.group}
                onChange={(group) => set({ group })}
                groups={groups}
                placeholder={t("entry.groupPlaceholder")}
              />
              <div>
                <div className="mb-1 text-[12px] text-fg-muted">{t("item.organize.tags")}</div>
                <TagsInput
                  label={t("item.organize.tags")}
                  value={form.tags}
                  onChange={(tags) => set({ tags })}
                  suggestions={tagSuggestions}
                  placeholder={t("item.tags.placeholder")}
                  data-testid="item-form-tags"
                />
              </div>
            </div>
            <div className="flex min-h-11 items-center justify-between gap-3">
              <div className="flex flex-col">
                <span className="text-[13px] text-fg">{t("item.organize.favorite")}</span>
                <span className="text-[12px] text-fg-subtle">
                  {t("item.organize.favoriteHelp")}
                </span>
              </div>
              <Toggle
                checked={form.favorite}
                onChange={(favorite) => set({ favorite })}
                ariaLabel={t("item.organize.favorite")}
              />
            </div>
          </FormGroup>
          <FormGroup title={t("item.appearance")}>
            <AccountAppearance
              hideLegend
              issuer={form.title}
              account={form.username}
              color={form.color}
              mark={form.mark}
              onColor={(color) => set({ color })}
              onMark={(mark) => set({ mark })}
            />
          </FormGroup>
          <FormGroup title={t("item.protection.label")}>
            <ProtectionChoice
              value={form.protection}
              onChange={(protection) => set({ protection })}
              data-testid="item-form-protection"
            />
          </FormGroup>
          <FormGroup title={t("item.autofill.label")}>
            {AUTOFILL_PREFS.map((pref) => (
              <div key={pref} className="flex min-h-11 items-center justify-between gap-3">
                <div className="flex flex-col">
                  <span className="text-[13px] text-fg">{t(`item.autofill.${pref}`)}</span>
                  <span className="text-[12px] text-fg-subtle">
                    {t("item.autofill.defaultIs", {
                      value: t(
                        state.browser.policy[pref] ? "item.autofill.on" : "item.autofill.off",
                      ),
                    })}
                  </span>
                </div>
                <Segmented
                  label={t(`item.autofill.${pref}`)}
                  size="sm"
                  value={form.autofill[pref]}
                  onChange={(value) => set({ autofill: { ...form.autofill, [pref]: value } })}
                  options={TRIS.map((value) => ({ value, label: t(`item.autofill.${value}`) }))}
                  data-testid={`item-form-${pref}`}
                />
              </div>
            ))}
          </FormGroup>
        </form>
      </Dialog>
      {generating && (
        <GeneratorDialog
          domain={firstHost}
          onClose={() => setGenerating(false)}
          onUse={(value) => {
            set({ password: { kind: "generated", value } });
            setGenerating(false);
          }}
        />
      )}
    </>
  );
}

/** One group of the form: a rule above, its title (and a note beside it), then its controls. */
function FormGroup({
  title,
  help,
  aside,
  children,
}: {
  title: string;
  help?: string;
  aside?: string;
  children: ReactNode;
}) {
  return (
    <section className="flex flex-col gap-2 border-t border-border pt-2.5">
      <div className="flex items-baseline justify-between gap-2">
        <h3 className="text-[13px] font-semibold text-fg">{title}</h3>
        {aside !== undefined && <span className="text-[11.5px] text-fg-subtle">{aside}</span>}
      </div>
      {help !== undefined && <p className="text-[12px] text-fg-subtle">{help}</p>}
      {children}
    </section>
  );
}

/** A custom field: its name, text or hidden, its value and the page's field it fills. */
function CustomField({
  field,
  onChange,
}: {
  field: LoginFieldDraft;
  onChange: (patch: Partial<LoginFieldDraft>) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-1.5 rounded-10 bg-inset p-2.5" data-testid="item-form-field">
      <div className="flex items-center gap-2">
        <Input
          aria-label={t("item.fields.name")}
          placeholder={t("item.fields.name")}
          value={field.label}
          onChange={(e) => onChange({ label: e.target.value })}
          className="flex-1"
        />
        <Segmented
          label={t("item.fields.label")}
          size="sm"
          value={field.hidden ? "hidden" : "text"}
          onChange={(v) => onChange({ hidden: v === "hidden" })}
          options={[
            { value: "text", label: t("item.fields.text") },
            { value: "hidden", label: t("item.fields.hidden") },
          ]}
        />
        <IconButton
          icon="trash"
          tone="danger"
          label={t("item.fields.remove")}
          size={28}
          onClick={() => onChange({ removed: true })}
        />
      </div>
      <div className="grid grid-cols-[minmax(0,62fr)_minmax(0,38fr)] gap-2">
        <Input
          aria-label={t("item.fields.value")}
          type={field.hidden ? "password" : "text"}
          mono={field.hidden}
          placeholder={field.value === undefined ? t("item.fields.keep") : t("item.fields.value")}
          value={field.value ?? ""}
          // A hidden value emptied while editing stays as it was, as the password does.
          onChange={(e) =>
            onChange({
              value:
                e.target.value === "" && field.id !== undefined && field.hidden
                  ? undefined
                  : e.target.value,
            })
          }
          autoComplete="off"
        />
        <Input
          aria-label={t("item.fields.pageField")}
          placeholder={t("item.fields.pageField")}
          mono
          icon="field"
          value={field.pageField}
          onChange={(e) => onChange({ pageField: e.target.value })}
        />
      </div>
    </div>
  );
}

/** How a site is matched: the three common ways, then the advanced ones (board 07-b). */
function MatchingMenu({
  value,
  onChange,
}: {
  value: Matching;
  onChange: (value: Matching) => void;
}) {
  const t = useT();
  const common: Matching[] = ["domain", "host", "never"];
  const advanced = MATCHINGS.filter((m) => !common.includes(m));
  const option = (m: Matching) => ({
    kind: "radio" as const,
    id: m,
    label: `${t(`item.matching.${m}`)} · ${t(`item.matching.${m}Help`)}`,
    checked: m === value,
  });
  return (
    <Menu
      label={t("item.sites.matching")}
      title={t("item.sites.matching")}
      align="end"
      sections={[
        { label: t("item.matching.common"), items: common.map(option) },
        { label: t("item.matching.advanced"), items: advanced.map(option) },
      ]}
      onSelect={(id) => {
        const picked = MATCHINGS.find((m) => m === id);
        if (picked !== undefined) onChange(picked);
      }}
      data-testid="item-form-matching"
      triggerClassName="inline-flex h-8 w-32 shrink-0 items-center justify-between gap-1 rounded-6 bg-surface px-2.5 text-[13px] text-fg hairline transition-colors hover:bg-inset"
      trigger={
        <>
          <span className="truncate">{t(`item.matching.${value}`)}</span>
          <span aria-hidden className="text-fg-subtle">
            ▾
          </span>
        </>
      }
    />
  );
}

/** The account whose codes go with the login: the one linked, or a choice among the accounts, or
 *  one made on the spot (a QR code, a link, a key typed in), linked once it is added. */
function CodeLink({
  linked,
  entries,
  onChange,
}: {
  linked: EntryView | undefined;
  entries: readonly EntryView[];
  onChange: (id: string | null) => void;
}) {
  const t = useT();
  const { settings } = useUiState();
  const codes = useCodes();
  const now = useClock();
  const [picking, setPicking] = useState(false);
  const still = motionReduced(settings);
  return (
    <div className="flex flex-col gap-2">
      {linked !== undefined ? (
        <div
          className="flex h-13 items-center gap-2.5 rounded-10 bg-inset px-3"
          data-testid="item-form-code">
          <EntryAvatar
            issuer={linked.issuer}
            account={linked.account}
            color={linked.color}
            mark={linked.mark}
            size={28}
          />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[13px] font-semibold text-fg">
              {linked.issuer || linked.account}
            </span>
            {linked.issuer !== "" && (
              <span className="truncate text-[11.5px] text-fg-muted">{linked.account}</span>
            )}
          </span>
          <LinkedCode code={codes.get(linked.id)} now={now} still={still} size={16} ring={18} />
          <Button variant="text" size="sm" onClick={() => setPicking(true)}>
            {t("item.code.change")}
          </Button>
          <Button
            variant="text-muted"
            size="sm"
            onClick={() => onChange(null)}
            data-testid="item-form-code-unlink">
            {t("item.code.unlink")}
          </Button>
        </div>
      ) : (
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            icon="key"
            onClick={() => setPicking(true)}
            data-testid="item-form-code-link">
            {t("item.code.link")}
          </Button>
          <span className="text-[12px] text-fg-subtle">{t("item.code.linkHelp")}</span>
        </div>
      )}
      {picking && (
        <CodePicker
          entries={entries}
          onPick={(id) => {
            onChange(id);
            setPicking(false);
          }}
          onClose={() => setPicking(false)}
        />
      )}
    </div>
  );
}
