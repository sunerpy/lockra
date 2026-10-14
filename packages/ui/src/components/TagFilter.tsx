import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Icon } from "./Icon";
import { Menu } from "./Menu";

export interface TagFilterProps {
  /** Every tag in use, with how many logins carry it. */
  tags: readonly { tag: string; count: number }[];
  /** The tags chosen: the list shows the logins that carry all of them. */
  value: readonly string[];
  onChange: (value: string[]) => void;
  disabled?: boolean;
  "data-testid"?: string;
}

/** The password page's tags (board 04-e): a button the size of a filter chip that opens a menu of
 *  the tags to tick; with tags chosen it shows them, in the chip's "on" look, and a × that clears
 *  them. Tags stack on the filter beside them: a login shows when it carries every tag chosen. */
export function TagFilter({
  tags,
  value,
  onChange,
  disabled = false,
  "data-testid": testId,
}: TagFilterProps) {
  const t = useT();
  const on = value.length > 0;
  const shown =
    value.length === 0
      ? t("passwords.tags.button")
      : value.length === 1
        ? (value[0] ?? "")
        : t("passwords.tags.several", { first: value[0] ?? "", n: value.length - 1 });
  return (
    <span className="inline-flex shrink-0 items-center" data-testid={testId}>
      <Menu
        label={t("passwords.tags.menu")}
        title={t("passwords.tags.menu")}
        disabled={disabled || tags.length === 0}
        sections={[
          {
            label: t("passwords.tags.heading"),
            items: tags.map(({ tag, count }) => ({
              kind: "check" as const,
              id: tag,
              label: `${tag} · ${count}`,
              checked: value.includes(tag),
            })),
          },
        ]}
        onSelect={(tag) =>
          onChange(value.includes(tag) ? value.filter((v) => v !== tag) : [...value, tag])
        }
        triggerClassName={cx(
          "inline-flex h-10 items-center gap-1.5 rounded-pill px-4 text-[13px] font-medium whitespace-nowrap hairline transition-colors disabled:opacity-50",
          on
            ? "bg-chip-on text-chip-on-fg shadow-[var(--card-shadow)]"
            : "bg-surface text-fg-muted hover:text-fg",
          on && "pr-9",
        )}
        trigger={
          <>
            <Icon name="tag" size={14} className={on ? "text-accent-text" : "text-fg-subtle"} />
            <bdi className="max-w-40 truncate">{shown}</bdi>
            {!on && <Icon name="chevronDown" size={12} />}
          </>
        }
        data-testid={testId === undefined ? undefined : `${testId}-menu`}
      />
      {on && (
        <button
          type="button"
          aria-label={t("passwords.tags.clear")}
          title={t("passwords.tags.clear")}
          onClick={() => onChange([])}
          className="-ml-8 inline-flex size-6 items-center justify-center rounded-full text-fg-muted hover:bg-inset hover:text-fg"
          data-testid={testId === undefined ? undefined : `${testId}-clear`}>
          <Icon name="x" size={12} />
        </button>
      )}
    </span>
  );
}
