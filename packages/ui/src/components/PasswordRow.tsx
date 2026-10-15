import { type ItemView, itemAccount, itemHost, itemName } from "@lockra/shared";
import type { KeyboardEvent, MouseEvent, ReactNode, SyntheticEvent } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { keepKey } from "../keys";
import { Badge } from "./Badge";
import { EntryAvatar } from "./EntryAvatar";
import { Icon } from "./Icon";
import { IconButton } from "./IconButton";

export interface PasswordRowProps {
  item: ItemView;
  /** Shown in the detail pane. */
  selected?: boolean;
  onSelect: () => void;
  /** Enter on the row, and its key button: copy the password. */
  onCopyPassword: () => void;
  onCopyUsername: () => void;
  /** A right click (or the context-menu key) on the row. */
  onContextMenu?: (event: MouseEvent<HTMLDivElement>) => void;
  /** The overflow menu. */
  menu?: ReactNode;
  /** Just copied: what the row says in place of its site, in the theme's pill. */
  copied?: string;
  /** Its own card on the canvas (the phone), not a row inside the list's card. */
  card?: boolean;
  className?: string;
}

const stop = (event: SyntheticEvent) => event.stopPropagation();

/** One login of the password list (board 01): avatar, title with its star, protection mark and
 *  badges, the user name, the site on the right, and three buttons (copy the user name, copy the
 *  password, the menu) shown with a mouse only while the row is hovered, focused or selected. A
 *  click selects it (the detail pane shows it), Enter copies its password. */
export function PasswordRow({
  item,
  selected = false,
  onSelect,
  onCopyPassword,
  onCopyUsername,
  onContextMenu,
  menu,
  copied,
  card = false,
  className,
}: PasswordRowProps) {
  const t = useT();
  const name = itemName(item);
  const account = itemAccount(item);
  const host = itemHost(item);
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter") {
      event.preventDefault();
      if (item.has_password) onCopyPassword();
    } else if (event.key === " ") {
      event.preventDefault();
      onSelect();
    }
  };
  return (
    <div
      role="option"
      aria-selected={selected}
      tabIndex={0}
      data-testid="password-row"
      data-item={item.id}
      data-copied={copied !== undefined || undefined}
      onClick={onSelect}
      onKeyDown={onKeyDown}
      onContextMenu={onContextMenu}
      className={cx(
        "group flex items-center gap-3 outline-none transition-colors",
        card
          ? "h-[calc(var(--row-h)+38px)] rounded-14 bg-surface px-3.5 hairline panel"
          : "h-[calc(var(--row-h)+32px)] rounded-10 px-3.5",
        selected
          ? "bg-nav-active shadow-[var(--card-shadow)]"
          : "hover:bg-inset focus-visible:bg-inset",
        className,
      )}>
      <EntryAvatar issuer={name} account={account} color={item.color} mark={item.mark} size={40} />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <span className="truncate text-[15px] font-semibold text-fg" title={name}>
            {name}
          </span>
          {item.favorite && (
            <Icon
              name="star"
              size={13}
              fill="currentColor"
              className="shrink-0 text-warning"
              data-testid="row-favorite-mark"
            />
          )}
          {item.protection === "confirm" && (
            <Icon
              name="shieldCheck"
              size={13}
              className="shrink-0 text-fg-subtle"
              aria-label={t("item.protection.confirm")}
              data-testid="row-protected"
            />
          )}
          {item.totp_entry !== null && (
            <Badge tone="accent" className="shrink-0">
              <Icon name="key" size={11} />
              {t("passwords.badge.code")}
            </Badge>
          )}
          {item.conflicts.length > 0 && (
            <Badge tone="warn" className="shrink-0">
              <Icon name="alert" size={11} />
              {t("passwords.badge.conflict")}
            </Badge>
          )}
          {item.read_only && (
            <Badge tone="neutral" className="shrink-0">
              {t("passwords.badge.readOnly")}
            </Badge>
          )}
        </div>
        {account !== "" && (
          <div
            className={cx("truncate text-fg-muted", card ? "text-[12px]" : "text-[12.5px]")}
            title={account}>
            {account}
          </div>
        )}
      </div>
      {copied !== undefined ? (
        <span
          data-testid="row-copied"
          className="inline-flex h-6 shrink-0 items-center gap-1.5 rounded-pill bg-[var(--row-pill-bg)] px-2.5 text-[13px] font-medium whitespace-nowrap text-[var(--row-pill-fg)]">
          <Icon name="check" size={14} />
          {copied}
        </span>
      ) : (
        host !== null &&
        !card && (
          // The list's own width decides: a narrow one (a 960 px window) drops the sites.
          <span
            className="hidden w-[150px] shrink-0 truncate text-right text-[12px] text-fg-subtle @[480px]:block"
            title={host}
            data-testid="row-host">
            {host}
          </span>
        )
      )}
      <div
        data-testid="row-actions"
        className={cx(
          "flex shrink-0 items-center gap-1",
          !card &&
            !selected &&
            "transition-opacity pointer-fine:opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 has-[[aria-expanded=true]]:opacity-100",
        )}
        onClick={stop}
        onKeyDown={keepKey}>
        {!card && account !== "" && (
          <IconButton
            icon="user"
            label={t("item.copy.username")}
            size={28}
            onClick={onCopyUsername}
            data-testid="row-copy-username"
          />
        )}
        {item.has_password && (
          <IconButton
            icon="field"
            label={t("item.copy.password")}
            size={card ? 40 : 28}
            onClick={onCopyPassword}
            data-testid="row-copy-password"
          />
        )}
        {menu}
      </div>
    </div>
  );
}
