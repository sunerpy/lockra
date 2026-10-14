import { cx } from "@lockra/ui";
import { AVATAR_RADIUS, AVATAR_TEXT, autoColor, initial } from "./avatar";

/** A login's square, as Lockra draws an account on its automatic colour (EntryAvatar). */
export function Avatar({
  name,
  fallback = "",
  size = 36,
}: {
  name: string;
  fallback?: string;
  size?: 32 | 36;
}) {
  return (
    <span
      aria-hidden
      data-tag={autoColor(name, fallback)}
      style={{ width: size, height: size, borderRadius: AVATAR_RADIUS[size] }}
      className={cx(
        "inline-flex shrink-0 items-center justify-center bg-tag-bg font-semibold text-tag-fg select-none",
        AVATAR_TEXT[size],
      )}>
      {initial(name, fallback)}
    </span>
  );
}
