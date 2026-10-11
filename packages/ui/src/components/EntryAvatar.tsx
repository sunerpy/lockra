import { ACCOUNT_COLORS, type AccountColor } from "@lockra/shared";
import { cx } from "../cx";

export interface EntryAvatarProps {
  issuer: string;
  account?: string;
  /** The account's colour; `auto` (the default) takes its name's. */
  color?: AccountColor;
  /** Shown instead of the name's initial: one or two characters the user chose. */
  mark?: string | null;
  /** 28 and 36 are the cards' (验证码视图 › 卡片), on the phone and the desktop. */
  size?: 24 | 28 | 32 | 36 | 40;
  className?: string;
}

/** The colours an automatic account takes: all but grey, which only a choice gives. */
const AUTO_COLORS = ACCOUNT_COLORS.filter(
  (c): c is Exclude<AccountColor, "auto" | "gray"> => c !== "auto" && c !== "gray",
);

/** The colour an `auto` account takes: picked by a hash (FNV-1a) of its name, the issuer else the
 *  account, so a service keeps its colour on every device and two services rarely share one. */
export function autoColor(issuer: string, account = ""): Exclude<AccountColor, "auto"> {
  const name = (issuer.trim() || account.trim()).toLocaleLowerCase();
  let hash = 0x811c9dc5;
  for (const ch of name) {
    hash ^= ch.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return AUTO_COLORS[hash % AUTO_COLORS.length] ?? "blue";
}

/** The first letter of the issuer (or the account): never a brand logo, so nothing is fetched and
 *  no trademark is drawn. */
export function initial(issuer: string, account = ""): string {
  const source = issuer.trim() || account.trim();
  const first = Array.from(source)[0] ?? "?";
  return first.toLocaleUpperCase();
}

/** Characters as people count them (an emoji with its joiners is one). */
function characters(text: string): number {
  return Array.from(new Intl.Segmenter(undefined, { granularity: "grapheme" }).segment(text))
    .length;
}

const TEXT_SIZE: Record<NonNullable<EntryAvatarProps["size"]>, readonly [string, string]> = {
  24: ["text-[11px]", "text-[9px]"],
  28: ["text-[13px]", "text-[11px]"],
  32: ["text-[14px]", "text-[12px]"],
  36: ["text-[16px]", "text-[13px]"],
  40: ["text-[16px]", "text-[14px]"],
};

/** The square's own corner: a theme's card radius (18 in 澄空) would make it a circle. */
const RADIUS: Record<NonNullable<EntryAvatarProps["size"]>, number> = {
  24: 10,
  28: 8,
  32: 10,
  36: 10,
  40: 10,
};

/** The account's square: its colour (DESIGN.md, "Account colours") and its mark, else the initial
 *  of its name. */
export function EntryAvatar({
  issuer,
  account,
  color = "auto",
  mark,
  size = 32,
  className,
}: EntryAvatarProps) {
  const text = mark ?? initial(issuer, account);
  const [one, two] = TEXT_SIZE[size];
  return (
    <span
      aria-hidden
      data-testid="entry-avatar"
      data-tag={color === "auto" ? autoColor(issuer, account) : color}
      style={{ width: size, height: size, borderRadius: RADIUS[size] }}
      className={cx(
        "inline-flex shrink-0 items-center justify-center overflow-hidden bg-tag-bg font-semibold whitespace-nowrap text-tag-fg select-none",
        characters(text) > 1 ? two : one,
        className,
      )}>
      {text}
    </span>
  );
}
