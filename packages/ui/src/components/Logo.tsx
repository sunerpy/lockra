/** The app mark: the navy rounded square the family's apps share, holding a blue padlock under a
 *  pale shackle, its body showing a masked code (three dots). Drawn as SVG so it stays crisp from
 *  the 24 px sidebar to the 1024 px icon it is rendered into (apps/desktop/src-tauri/icons/icon.svg
 *  is the same drawing). Colours are the mark's own, not theme tokens: it must look the same in
 *  every theme, like the window icon does (the one hex-literal exception, DESIGN.md §1). */
export const LOGO_NAVY = "#0B1220";
export const LOGO_PALE = "#E7EDF5";
export const LOGO_BLUE = "#339CFF";

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
        d="M37 45 V36 A13 13 0 0 1 63 36 V45"
        fill="none"
        stroke={LOGO_PALE}
        strokeWidth="7"
        strokeLinecap="round"
      />
      <rect x="26" y="43" width="48" height="37" rx="9" fill={LOGO_BLUE} />
      <circle cx="38" cy="61.5" r="4.2" fill={LOGO_PALE} />
      <circle cx="50" cy="61.5" r="4.2" fill={LOGO_PALE} />
      <circle cx="62" cy="61.5" r="4.2" fill={LOGO_PALE} />
    </svg>
  );
}
