// A login's form on the phone (boards 26, 27): new or edited, 保存 in the bar. Its model and what
// saving it sends are the desktop's (@lockra/shared `login-form`): the password and the hidden
// fields stay unless something is typed, the notes come only when asked for, a password made by the
// generator is saved as the generator's. The browser autofill settings are the desktop's to change;
// the form keeps them as they are.
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
  entryLabel,
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
  Banner,
  Button,
  EntryAvatar,
  Icon,
  IconButton,
  Input,
  PasswordField,
  ProtectionChoice,
  Segmented,
  TagsInput,
  Textarea,
  useBackend,
  useItems,
  useSubmit,
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
import { useNav } from "../app/nav";
import { useItemActions } from "../app/verify";
import { BottomSheet } from "../components/BottomSheet";
import { GeneratorSheet } from "../components/GeneratorSheet";
import { GroupInput } from "../components/GroupInput";
import { DeleteLoginDialog, LoginCode } from "../components/LoginParts";
import { Page } from "../components/Page";
import { SwitchRow } from "../components/Rows";

export function LoginEdit({ item, title = "" }: { item?: ItemView; title?: string }) {
  const t = useT();
  const nav = useNav();
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
  const [deleting, setDeleting] = useState(false);
  const groups = useMemo(() => itemGroups(items ?? []), [items]);
  const tagSuggestions = useMemo(() => itemTags(items ?? []).map((c) => c.tag), [items]);
  const set = (change: Partial<LoginForm>) => setForm((f) => ({ ...f, ...change }));
  const found = loginProblems(form);
  // The notes shown for editing are a secret view; it ends with the page, however it goes.
  useEffect(() => {
    if (!notesShown) return undefined;
    return () => {
      void backend.dispatch({ command: "secret_view_closed" }).catch(() => undefined);
    };
  }, [notesShown, backend]);
  // A save that found problems goes to the first of them, once they are drawn.
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
    // The generator keeps only the password it made last (the desktop form's reason).
    if (form.password.kind === "generated")
      set({ password: { kind: "set", value: form.password.value } });
    setGenerating(true);
  };
  const save = async (event: SubmitEvent) => {
    event.preventDefault();
    setTried(true);
    if (found.count > 0) {
      setAttempt((n) => n + 1);
      return;
    }
    if (item === undefined) {
      const added = await submit.run(() =>
        backend.dispatch({ command: "item_add", draft: loginDraft(form) }),
      );
      // The new login's page takes the form's place.
      if (added !== undefined) nav.replace({ name: "login", id: added.id });
      return;
    }
    const done = await submit.run(() =>
      backend.dispatch({ command: "item_update", id: item.id, patch: loginPatch(form) }),
    );
    if (done !== undefined) nav.back();
  };
  const linked = form.totp === null ? undefined : state.entries.find((e) => e.id === form.totp);
  return (
    <Page
      title={item === undefined ? t("item.newTitle") : t("item.editTitle")}
      testId="page-login-edit"
      actions={
        <Button
          variant="primary"
          type="submit"
          form={formId}
          loading={submit.busy}
          disabled={form.title.trim() === ""}
          data-testid="login-save">
          {t("item.save")}
        </Button>
      }>
      <form
        ref={formRef}
        id={formId}
        onSubmit={(e) => void save(e)}
        noValidate
        className="flex flex-col gap-5">
        {tried && found.count > 0 && (
          <Banner
            tone="danger"
            marker="bar"
            title={t("item.errors.title", { n: found.count })}
            data-testid="login-errors"
          />
        )}
        {submit.error !== undefined && (
          <Banner tone="danger" marker="bar">
            {errorText(t, submit.error)}
          </Banner>
        )}
        <div className="flex flex-col gap-3">
          <Input
            size="lg"
            label={t("item.title")}
            placeholder={t("item.titlePlaceholder")}
            value={form.title}
            onChange={(e) => set({ title: e.target.value })}
            error={tried && found.title ? t("item.errors.title_required") : undefined}
            autoComplete="off"
            data-testid="login-title"
          />
          <Input
            size="lg"
            label={t("item.username")}
            value={form.username}
            onChange={(e) => set({ username: e.target.value })}
            autoComplete="off"
            data-testid="login-username"
          />
          <Input
            size="lg"
            type="email"
            label={t("item.email")}
            value={form.email}
            onChange={(e) => set({ email: e.target.value })}
            autoComplete="off"
            data-testid="login-email"
          />
          <PasswordField
            size="lg"
            label={t("item.password")}
            value={form.password.kind === "keep" ? "" : form.password.value}
            onChange={(value) => set({ password: typedPassword(value, item !== undefined) })}
            placeholder={form.password.kind === "keep" ? t("item.fields.keep") : undefined}
            strength={form.password.kind !== "keep"}
            autoComplete="new-password"
            data-testid="login-password"
          />
          <Button size="lg" icon="dice" onClick={openGenerator} data-testid="login-generate">
            {t("passwords.add.generate")}
          </Button>
        </div>
        <Part title={t("item.sites.label")}>
          {form.urls.map((row) => (
            <div key={row.key} className="flex flex-col gap-2" data-testid="login-url">
              <div className="flex items-start gap-2">
                <Input
                  size="lg"
                  aria-label={t("item.sites.label")}
                  placeholder={t("item.sites.placeholder")}
                  value={row.url}
                  mono={row.matching === "regex"}
                  inputMode="url"
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
                  className="min-w-0 flex-1"
                />
                <IconButton
                  icon="trash"
                  tone="danger"
                  label={t("item.sites.remove")}
                  size={40}
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
              <MatchingPicker
                value={row.matching}
                onChange={(matching) =>
                  set({ urls: form.urls.map((u) => (u.key === row.key ? { ...u, matching } : u)) })
                }
              />
            </div>
          ))}
          <Button
            variant="text"
            icon="plus"
            className="self-start"
            onClick={() => set({ urls: [...form.urls, blankUrl()] })}
            data-testid="login-add-url">
            {t("item.sites.add")}
          </Button>
        </Part>
        <Part title={t("item.code.label")}>
          <CodePicker linked={linked} entries={state.entries} onChange={(totp) => set({ totp })} />
        </Part>
        <Part title={t("item.notes.label")} aside={t("item.notes.secret")}>
          {form.notes === undefined ? (
            <Button
              icon="eye"
              className="self-start"
              onClick={() => void loadNotes()}
              data-testid="login-notes-load">
              {t("passwords.detail.show")}
            </Button>
          ) : (
            <Textarea
              aria-label={t("item.notes.label")}
              value={form.notes}
              onChange={(e) => set({ notes: e.target.value })}
              className="h-24 text-[15px]"
              data-testid="login-notes"
            />
          )}
        </Part>
        <Part title={t("item.fields.label")}>
          {form.fields
            .filter((f) => !f.removed)
            .map((field) => (
              <CustomField
                key={field.key}
                field={field}
                onChange={(patch) =>
                  set({
                    fields: form.fields.map((f) => (f.key === field.key ? { ...f, ...patch } : f)),
                  })
                }
              />
            ))}
          <Button
            variant="text"
            icon="plus"
            className="self-start"
            onClick={() => set({ fields: [...form.fields, blankField()] })}
            data-testid="login-add-field">
            {t("item.fields.add")}
          </Button>
        </Part>
        <Part title={t("item.organize.label")}>
          <GroupInput value={form.group} onChange={(group) => set({ group })} groups={groups} />
          <div className="flex flex-col gap-1">
            <span className="text-[12px] text-fg-muted">{t("item.organize.tags")}</span>
            <TagsInput
              label={t("item.organize.tags")}
              value={form.tags}
              onChange={(tags) => set({ tags })}
              suggestions={tagSuggestions}
              placeholder={t("item.tags.placeholder")}
              data-testid="login-tags"
            />
          </div>
          <div className="-mx-4">
            <SwitchRow
              label={t("item.organize.favorite")}
              hint={t("item.organize.favoriteHelp")}
              checked={form.favorite}
              onChange={(favorite) => set({ favorite })}
              testId="login-favorite"
            />
          </div>
        </Part>
        <Part title={t("item.protection.label")}>
          <ProtectionChoice
            size="lg"
            value={form.protection}
            onChange={(protection) => set({ protection })}
            data-testid="login-protection"
          />
          <p className="text-[13px] text-fg-subtle">{t("mobile.logins.autofillNote")}</p>
        </Part>
        {item !== undefined && !item.read_only && (
          <Button
            variant="text-danger"
            size="lg"
            className="self-start"
            onClick={() => setDeleting(true)}
            data-testid="login-delete">
            {t("mobile.logins.deleteLogin")}
          </Button>
        )}
      </form>
      {generating && (
        <GeneratorSheet
          domain={loginFormHost(form)}
          onClose={() => setGenerating(false)}
          onUse={(value) => {
            set({ password: { kind: "generated", value } });
            setGenerating(false);
          }}
        />
      )}
      {deleting && item !== undefined && (
        <DeleteLoginDialog
          item={item}
          onClose={() => setDeleting(false)}
          onDeleted={() => nav.popTo("logins")}
        />
      )}
    </Page>
  );
}

/** One part of the form: its title (and a note beside it), then its controls. */
function Part({ title, aside, children }: { title: string; aside?: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-3" aria-label={title}>
      <div className="flex items-baseline justify-between gap-2">
        <h2 className="text-[13px] font-medium text-fg-muted">{title}</h2>
        {aside !== undefined && <span className="text-[12px] text-fg-subtle">{aside}</span>}
      </div>
      {children}
    </section>
  );
}

/** A custom field: its name, text or hidden, its value. Its page field is the desktop's. */
function CustomField({
  field,
  onChange,
}: {
  field: LoginFieldDraft;
  onChange: (patch: Partial<LoginFieldDraft>) => void;
}) {
  const t = useT();
  return (
    <div className="flex flex-col gap-2 rounded-14 bg-inset p-3" data-testid="login-field">
      <div className="flex items-center gap-2">
        <Input
          size="lg"
          aria-label={t("item.fields.name")}
          placeholder={t("item.fields.name")}
          value={field.label}
          onChange={(e) => onChange({ label: e.target.value })}
          className="min-w-0 flex-1"
        />
        <IconButton
          icon="trash"
          tone="danger"
          label={t("item.fields.remove")}
          size={40}
          onClick={() => onChange({ removed: true })}
        />
      </div>
      <Segmented
        label={t("item.fields.label")}
        size="lg"
        value={field.hidden ? "hidden" : "text"}
        onChange={(v) => onChange({ hidden: v === "hidden" })}
        options={[
          { value: "text", label: t("item.fields.text") },
          { value: "hidden", label: t("item.fields.hidden") },
        ]}
      />
      <Input
        size="lg"
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
    </div>
  );
}

/** How a site is matched, chosen in a sheet: the three common ways, then the advanced ones. */
function MatchingPicker({
  value,
  onChange,
}: {
  value: Matching;
  onChange: (value: Matching) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const common: Matching[] = ["domain", "host", "never"];
  const ordered = [...common, ...MATCHINGS.filter((m) => !common.includes(m))];
  return (
    <>
      <button
        type="button"
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen(true)}
        className="flex h-11 items-center gap-2 rounded-6 bg-surface px-3 text-left text-[15px] text-fg hairline"
        data-testid="login-matching">
        <span className="min-w-0 flex-1 truncate">
          {t("mobile.logins.matching", { name: t(`item.matching.${value}`) })}
        </span>
        <Icon name="chevronDown" size={16} className="shrink-0 text-fg-subtle" />
      </button>
      {open && (
        <BottomSheet
          title={t("mobile.logins.matchingTitle")}
          onClose={() => setOpen(false)}
          data-testid="matching-sheet">
          <div
            role="listbox"
            aria-label={t("mobile.logins.matchingTitle")}
            className="flex flex-col pb-1">
            {ordered.map((m, i) => (
              <button
                key={m}
                type="button"
                role="option"
                aria-selected={m === value}
                onClick={() => {
                  onChange(m);
                  setOpen(false);
                }}
                className={`flex min-h-14 w-full items-center gap-3 rounded-10 px-3 py-2 text-left active:bg-inset ${i === common.length ? "mt-1 border-t border-border" : ""}`}>
                <span className="flex min-w-0 flex-1 flex-col">
                  <span className="text-[15px] text-fg">{t(`item.matching.${m}`)}</span>
                  <span className="text-[13px] text-fg-muted">{t(`item.matching.${m}Help`)}</span>
                </span>
                {m === value && <Icon name="check" size={18} className="shrink-0 text-accent" />}
              </button>
            ))}
          </div>
        </BottomSheet>
      )}
    </>
  );
}

/** The account whose codes go with the login: the one linked, or a choice among the accounts. */
function CodePicker({
  linked,
  entries,
  onChange,
}: {
  linked: EntryView | undefined;
  entries: readonly EntryView[];
  onChange: (id: string | null) => void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false);
  return (
    <>
      {linked !== undefined ? (
        <div
          className="flex min-h-16 items-center gap-3 rounded-14 bg-surface px-3 hairline panel"
          data-testid="login-code-card">
          <EntryAvatar
            issuer={linked.issuer}
            account={linked.account}
            color={linked.color}
            mark={linked.mark}
            size={36}
          />
          <span className="flex min-w-0 flex-1 flex-col">
            <span className="truncate text-[15px] font-semibold text-fg">
              {entryLabel(linked.issuer, linked.account)}
            </span>
            <LoginCode entryId={linked.id} size={14} ring={16} />
          </span>
          <Button variant="text" onClick={() => setOpen(true)} data-testid="login-code-change">
            {t("item.code.change")}
          </Button>
        </div>
      ) : (
        <div className="flex flex-col gap-1">
          <Button
            size="lg"
            icon="key"
            onClick={() => setOpen(true)}
            disabled={entries.length === 0}
            data-testid="login-code-link">
            {t("item.code.link")}
          </Button>
          <span className="text-[13px] text-fg-subtle">{t("item.code.linkHelp")}</span>
        </div>
      )}
      {open && (
        <BottomSheet
          title={t("mobile.logins.codeSheet")}
          onClose={() => setOpen(false)}
          data-testid="code-sheet">
          <div
            role="listbox"
            aria-label={t("mobile.logins.codeSheet")}
            className="flex flex-col pb-1">
            {linked !== undefined && (
              <button
                type="button"
                role="option"
                aria-selected={false}
                onClick={() => {
                  onChange(null);
                  setOpen(false);
                }}
                className="flex min-h-12 w-full items-center gap-3 rounded-10 px-3 text-left text-[15px] text-fg active:bg-inset"
                data-testid="code-sheet-none">
                <Icon name="x" size={18} className="shrink-0 text-fg-subtle" />
                {t("mobile.logins.noCode")}
              </button>
            )}
            {entries.map((entry) => (
              <button
                key={entry.id}
                type="button"
                role="option"
                aria-selected={entry.id === linked?.id}
                onClick={() => {
                  onChange(entry.id);
                  setOpen(false);
                }}
                className="flex min-h-14 w-full items-center gap-3 rounded-10 px-3 text-left active:bg-inset"
                data-testid="code-sheet-option">
                <EntryAvatar
                  issuer={entry.issuer}
                  account={entry.account}
                  color={entry.color}
                  mark={entry.mark}
                  size={32}
                />
                <span className="min-w-0 flex-1 truncate text-[15px] text-fg">
                  {entryLabel(entry.issuer, entry.account)}
                </span>
                <LoginCode entryId={entry.id} size={14} ring={16} />
              </button>
            ))}
          </div>
        </BottomSheet>
      )}
    </>
  );
}
