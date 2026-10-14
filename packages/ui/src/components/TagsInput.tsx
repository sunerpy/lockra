import { type KeyboardEvent, useId, useState } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon } from "./Icon";

export interface TagsInputProps {
  label: string;
  value: readonly string[];
  onChange: (value: string[]) => void;
  /** Tags already in use elsewhere, offered while typing. */
  suggestions?: readonly string[];
  placeholder?: string;
  disabled?: boolean;
  "data-testid"?: string;
}

/** A login's tags: pills with a ×, a field that adds what is typed on Enter or a comma (the same tag
 *  once, whatever its case), Backspace in the empty field removes the last one. */
export function TagsInput({
  label,
  value,
  onChange,
  suggestions = [],
  placeholder,
  disabled = false,
  "data-testid": testId,
}: TagsInputProps) {
  const t = useT();
  const [text, setText] = useState("");
  const listId = useId();
  const add = (raw: string) => {
    const tag = raw.trim().replace(/,+$/, "").trim();
    setText("");
    if (tag === "" || value.some((v) => v.toLocaleLowerCase() === tag.toLocaleLowerCase())) return;
    onChange([...value, tag]);
  };
  const remove = (tag: string) => onChange(value.filter((v) => v !== tag));
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter" || event.key === ",") {
      if (text.trim() === "") return;
      event.preventDefault();
      add(text);
    } else if (event.key === "Backspace" && text === "" && value.length > 0) {
      event.preventDefault();
      onChange(value.slice(0, -1));
    }
  };
  const offered = suggestions.filter((s) => !value.includes(s));
  return (
    <div
      className={cx(
        "flex min-h-8 flex-wrap items-center gap-1.5 rounded-6 bg-surface px-1.5 py-1 hairline focus-within:shadow-[0_0_0_2px_var(--accent-text)]",
        disabled && "opacity-50",
      )}
      data-testid={testId}>
      {value.map((tag) => (
        <span
          key={tag}
          className="inline-flex h-6 items-center gap-1 rounded-pill bg-accent-soft pr-1 pl-2.5 text-[12px] text-accent-text"
          data-testid="tag-pill">
          <bdi>{tag}</bdi>
          <button
            type="button"
            aria-label={t("item.tags.remove", { tag })}
            disabled={disabled}
            onClick={() => remove(tag)}
            className="inline-flex size-4 items-center justify-center rounded-full hover:bg-accent-soft">
            <Icon name="x" size={11} />
          </button>
        </span>
      ))}
      <input
        aria-label={label}
        list={offered.length > 0 ? listId : undefined}
        value={text}
        disabled={disabled}
        onChange={(event) => {
          if (event.target.value.endsWith(",")) add(event.target.value);
          else setText(event.target.value);
        }}
        onKeyDown={onKeyDown}
        onBlur={() => {
          if (text.trim() !== "") add(text);
        }}
        placeholder={value.length === 0 ? placeholder : undefined}
        className="h-6 min-w-24 flex-1 bg-transparent px-1 text-[13px] text-fg outline-none placeholder:text-fg-subtle"
        data-testid={testId === undefined ? undefined : `${testId}-input`}
      />
      {offered.length > 0 && (
        <datalist id={listId}>
          {offered.map((s) => (
            <option key={s} value={s} />
          ))}
        </datalist>
      )}
    </div>
  );
}
