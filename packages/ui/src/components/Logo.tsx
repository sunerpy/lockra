import { cx } from "../cx";

export interface LogoProps {
  /** Rendered width and height in px. */
  size?: number;
  className?: string;
  /** Accessible name; omit for a purely decorative mark next to the product name. */
  label?: string;
}

/** The app mark inside the app: a padlock under its shackle, its body showing a masked code (three
 *  dots), on the theme's own card (frosted in 澄空, paper in 墨白, a dark card at night) rather than
 *  the app icon's tile (user choice 2026-10-11, option B of the Penpot page 「00 · Logo」). The
 *  shackle takes the theme's ink, the body the theme's own blue (`--logo-body`, whatever the accent),
 *  the dots `--logo-dots`. The drawing is the app icon's (apps/desktop/src-tauri/icons/icon.svg),
 *  whose tile is the brand's blue-violet gradient. */
export function Logo({ size = 24, className, label }: LogoProps) {
  return (
    <span
      data-testid="app-logo"
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      style={{ width: size, height: size }}
      className={cx(
        "inline-flex shrink-0 overflow-hidden rounded-[22%] bg-surface hairline panel",
        className,
      )}>
      <svg viewBox="0 0 100 100" width="100%" height="100%" aria-hidden focusable="false">
        <path
          d="M37 45 V36 A13 13 0 0 1 63 36 V45"
          fill="none"
          strokeWidth="7"
          strokeLinecap="round"
          className="stroke-fg"
        />
        <rect x="26" y="43" width="48" height="37" rx="9" className="fill-[var(--logo-body)]" />
        {[38, 50, 62].map((x) => (
          <circle key={x} cx={x} cy="61.5" r="4.2" className="fill-[var(--logo-dots)]" />
        ))}
      </svg>
    </span>
  );
}
