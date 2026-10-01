import { cx } from "../cx";

export interface EntryAvatarProps {
  issuer: string;
  account?: string;
  size?: 24 | 32 | 40;
  className?: string;
}

/** The first letter of the issuer (or the account) on a neutral square: never a brand logo, so
 *  nothing is fetched and no trademark is drawn. */
export function initial(issuer: string, account = ""): string {
  const source = issuer.trim() || account.trim();
  const first = Array.from(source)[0] ?? "?";
  return first.toLocaleUpperCase();
}

export function EntryAvatar({ issuer, account, size = 32, className }: EntryAvatarProps) {
  return (
    <span
      aria-hidden
      data-testid="entry-avatar"
      style={{ width: size, height: size }}
      className={cx(
        "inline-flex shrink-0 items-center justify-center rounded-10 bg-inset2 font-semibold text-fg-muted select-none",
        size === 24 ? "text-[11px]" : size === 32 ? "text-[14px]" : "text-[16px]",
        className,
      )}>
      {initial(issuer, account)}
    </span>
  );
}
