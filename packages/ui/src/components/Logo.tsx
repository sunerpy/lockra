/** The app mark: Voltip's navy rounded square, here holding a pale lock inside three quarters of
 *  an orange ring (a code's countdown). Drawn as SVG so it stays crisp from the 24 px sidebar to the
 *  1024 px icon it is rendered into. Colours are the mark's own, not theme tokens: it must look the
 *  same in every theme, like the window icon does (the one hex-literal exception, DESIGN.md §1). */
export const LOGO_NAVY = "#0B1220";
export const LOGO_PALE = "#E7EDF5";
export const LOGO_ORANGE = "#F97316";

export interface LogoProps {
  /** Rendered width and height in px. */
  size?: number;
  className?: string;
  /** Accessible name; omit for a purely decorative mark next to the product name. */
  label?: string;
}

export function Logo({ size = 24, className, label }: LogoProps) {
  return (
    <svg
      data-testid="app-logo"
      width={size}
      height={size}
      viewBox="0 0 100 100"
      role={label ? "img" : "presentation"}
      aria-label={label}
      aria-hidden={label ? undefined : true}
      className={className}
      focusable="false">
      <rect width="100" height="100" rx="22" fill={LOGO_NAVY} />
      <path
        d="M50 18 A32 32 0 1 1 18 50"
        fill="none"
        stroke={LOGO_ORANGE}
        strokeWidth="8"
        strokeLinecap="round"
      />
      <path
        d="M42 47 V42 A8 8 0 0 1 58 42 V47"
        fill="none"
        stroke={LOGO_PALE}
        strokeWidth="6"
        strokeLinecap="round"
      />
      <rect x="36" y="46" width="28" height="23" rx="5" fill={LOGO_PALE} />
      <circle cx="50" cy="55.5" r="3.5" fill={LOGO_NAVY} />
      <rect x="48.25" y="56" width="3.5" height="7" rx="1.75" fill={LOGO_NAVY} />
    </svg>
  );
}
