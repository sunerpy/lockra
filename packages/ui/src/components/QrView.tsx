import type { ReactNode } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";

export interface QrViewProps {
  /** The QR code as SVG text (rendered by the Rust core). */
  svg: string;
  /** Accessible name. */
  label?: string;
  size?: number;
  footer?: ReactNode;
  className?: string;
}

/** A QR code on its white plate. The SVG goes through an `<img>` data URL, so nothing inside it
 *  can run, and the plate stays white in dark themes: phones read dark-on-light codes best. */
export function QrView({ svg, label, size = 280, footer, className }: QrViewProps) {
  const t = useT();
  return (
    <figure className={cx("flex flex-col items-center gap-3", className)}>
      <div className="rounded-14 bg-qr-plate p-3 hairline" data-testid="qr-plate">
        <img
          src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`}
          alt={label ?? t("ui.a11y.qr")}
          width={size}
          height={size}
          draggable={false}
          className="block [image-rendering:pixelated]"
        />
      </div>
      {footer && <figcaption className="text-[12px] text-fg-muted">{footer}</figcaption>}
    </figure>
  );
}
