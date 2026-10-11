// An account's colour and avatar text (the edit dialog on the desktop, the edit page on the
// phone): the swatches are one radio group (the arrow keys move the choice, Tab leaves it), the
// text keeps two characters as people count them, and the avatar beside them shows the result as
// it changes.
import { ACCOUNT_COLORS, type AccountColor, cutMark } from "@lockra/shared";
import { type KeyboardEvent, useId, useRef } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { EntryAvatar, autoColor, initial } from "./EntryAvatar";
import { Icon } from "./Icon";
import { Input } from "./Input";

export interface AccountAppearanceProps {
  issuer: string;
  account: string;
  color: AccountColor;
  mark: string;
  onColor: (color: AccountColor) => void;
  onMark: (mark: string) => void;
  /** `lg` is the phone's: 36 px swatches and a 44 px field. */
  size?: "md" | "lg";
}

export function AccountAppearance({
  issuer,
  account,
  color,
  mark,
  onColor,
  onMark,
  size = "md",
}: AccountAppearanceProps) {
  const t = useT();
  const labelId = useId();
  const swatches = useRef<(HTMLButtonElement | null)[]>([]);
  const onKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const step =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? -1
          : 0;
    if (step === 0) return;
    event.preventDefault();
    const n = ACCOUNT_COLORS.length;
    const next = ACCOUNT_COLORS[(ACCOUNT_COLORS.indexOf(color) + step + n) % n] ?? "auto";
    onColor(next);
    swatches.current[ACCOUNT_COLORS.indexOf(next)]?.focus();
  };
  return (
    <fieldset className="flex flex-col gap-2" data-testid="entry-appearance">
      <legend className="mb-1 text-[12px] text-fg-muted">{t("entry.appearance")}</legend>
      <div className="flex items-start gap-3">
        <EntryAvatar
          issuer={issuer}
          account={account}
          color={color}
          mark={mark.trim() === "" ? null : mark.trim()}
          size={40}
        />
        <div className="flex min-w-0 flex-1 flex-col gap-1">
          <span id={labelId} className="text-[12px] text-fg-muted">
            {t("entry.color")}
          </span>
          <div
            role="radiogroup"
            aria-labelledby={labelId}
            onKeyDown={onKey}
            className={cx("flex flex-wrap", size === "lg" ? "gap-2" : "gap-1.5")}>
            {ACCOUNT_COLORS.map((choice, i) => {
              const checked = choice === color;
              return (
                <button
                  key={choice}
                  ref={(el) => {
                    swatches.current[i] = el;
                  }}
                  type="button"
                  role="radio"
                  aria-checked={checked}
                  aria-label={t(`entry.colors.${choice}`)}
                  title={t(`entry.colors.${choice}`)}
                  tabIndex={checked ? 0 : -1}
                  onClick={() => onColor(choice)}
                  data-tag={choice === "auto" ? autoColor(issuer, account) : choice}
                  className={cx(
                    "inline-flex items-center justify-center rounded-pill bg-tag-bg text-tag-fg outline-offset-2 transition-shadow",
                    size === "lg" ? "h-9 w-9" : "h-6 w-6",
                    checked
                      ? "ring-2 ring-accent-text ring-offset-1 ring-offset-surface"
                      : "hairline",
                  )}>
                  {choice === "auto" && <Icon name="sparkles" size={size === "lg" ? 16 : 12} />}
                </button>
              );
            })}
          </div>
          <span className="text-[12px] text-fg-subtle">{t("entry.colorHint")}</span>
        </div>
      </div>
      <Input
        label={t("entry.mark")}
        size={size}
        value={mark}
        onChange={(e) => onMark(cutMark(e.target.value))}
        placeholder={initial(issuer, account)}
        help={t("entry.markHint")}
        autoComplete="off"
        spellCheck={false}
      />
    </fieldset>
  );
}
