// A group's name: typed, or picked from the groups in use in a list drawn by Lockra (not the
// webview's own `<datalist>` popup, a dark native box on WebKitGTK and WebView2). The list opens
// on focus, a click or ↓; typing filters it and offers the typed name as a new group; ↑ ↓ move,
// Enter picks another group (on what the field already holds it submits the form, as in any
// field), Esc closes the list first (a dialog around it hears the next Esc).
import {
  type KeyboardEvent as ReactKeyboardEvent,
  useCallback,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon } from "./Icon";

export interface GroupComboboxProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  /** The groups in use. */
  groups: readonly string[];
  placeholder?: string;
  autoFocus?: boolean;
  className?: string;
}

interface Option {
  key: string;
  label: string;
  value: string;
}

export function GroupCombobox({
  label,
  value,
  onChange,
  groups,
  placeholder,
  autoFocus = false,
  className,
}: GroupComboboxProps) {
  const t = useT();
  const inputId = useId();
  const listId = useId();
  const [open, setOpen] = useState(false);
  // What the user typed since the list opened filters it; the current value alone does not.
  const [typed, setTyped] = useState(false);
  const [active, setActive] = useState(-1);
  const input = useRef<HTMLInputElement>(null);
  const name = value.trim();
  const options = useMemo<Option[]>(() => {
    const query = typed ? name.toLocaleLowerCase() : "";
    const found = groups.filter((g) => query === "" || g.toLocaleLowerCase().includes(query));
    const none: Option[] =
      query === "" ? [{ key: "\u0000", label: t("ui.groupPicker.none"), value: "" }] : [];
    const fresh: Option[] =
      typed && name !== "" && !groups.includes(name)
        ? [{ key: `\u0001${name}`, label: t("ui.groupPicker.create", { name }), value: name }]
        : [];
    return [...none, ...found.map((g) => ({ key: g, label: g, value: g })), ...fresh];
  }, [groups, name, typed, t]);

  const show = (at?: number) => {
    setOpen(true);
    setActive(at ?? options.findIndex((o) => o.value === name));
  };
  const close = useCallback(() => {
    setOpen(false);
    setTyped(false);
    setActive(-1);
  }, []);
  useEffect(() => {
    if (!open) return undefined;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    // On `window`: its capture phase runs before a `Dialog`'s listener on `document` (as
    // `Popover`'s), so the Esc that closes the list does not also close the dialog it sits in.
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, close]);
  const pick = (option: Option) => {
    onChange(option.value);
    close();
  };
  const onKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (!open) {
        show();
        return;
      }
      const by = event.key === "ArrowDown" ? 1 : -1;
      setActive((at) => (options.length === 0 ? -1 : (at + by + options.length) % options.length));
    } else if (event.key === "Enter" && open) {
      const option = options[active];
      close();
      // Nothing other than what the field holds: the Enter submits the form as in any field.
      if (option === undefined || option.value === name) return;
      // A pick, not the form's submit.
      event.preventDefault();
      onChange(option.value);
    } else if (event.key === "Tab") {
      close();
    }
  };
  const activeId = open && active >= 0 ? `${listId}-${active}` : undefined;
  return (
    <div className={cx("relative flex flex-col gap-1", className)}>
      <label htmlFor={inputId} className="text-[12px] text-fg-muted">
        {label}
      </label>
      <div className="flex h-8 items-center gap-2 rounded-6 bg-surface px-2.5 text-[13px] hairline transition-colors focus-within:border-fg focus-within:shadow-[0_0_0_1px_var(--fg)]">
        <Icon name="folder" size={14} className="shrink-0 text-fg-subtle" />
        <input
          ref={input}
          id={inputId}
          role="combobox"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          aria-activedescendant={activeId}
          autoComplete="off"
          spellCheck={false}
          value={value}
          placeholder={placeholder}
          onChange={(e) => {
            onChange(e.target.value);
            setTyped(true);
            setOpen(true);
            setActive(0);
          }}
          onFocus={() => show()}
          onClick={() => {
            if (!open) show();
          }}
          onBlur={close}
          onKeyDown={onKeyDown}
          className="min-w-0 flex-1 bg-transparent outline-none placeholder:text-fg-subtle"
          {...(autoFocus ? { "data-autofocus": true } : {})}
        />
        <Icon name="chevronDown" size={14} className="shrink-0 text-fg-subtle" />
      </div>
      {open && options.length > 0 && (
        <div
          id={listId}
          role="listbox"
          aria-label={t("ui.groupPicker.options")}
          className="absolute top-full right-0 left-0 z-50 mt-1 max-h-60 overflow-y-auto rounded-10 bg-surface p-1 shadow-win hairline"
          data-testid="group-options">
          {options.map((option, at) => {
            const chosen = option.value === name && !option.key.startsWith("\u0001");
            return (
              <div
                key={option.key}
                id={`${listId}-${at}`}
                role="option"
                aria-selected={at === active}
                // Keeps the focus in the field: the pick happens on the click that follows.
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => pick(option)}
                onMouseEnter={() => setActive(at)}
                className={cx(
                  "flex h-8 cursor-pointer items-center gap-2 rounded-6 px-2 text-[13px] text-fg",
                  at === active && "bg-inset",
                  option.value === "" && "text-fg-muted",
                )}>
                <span
                  className="min-w-0 flex-1 truncate"
                  {...(option.value === "" ? {} : { "data-user-text": "" })}>
                  {option.label}
                </span>
                {chosen && <Icon name="check" size={14} className="shrink-0 text-accent" />}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
