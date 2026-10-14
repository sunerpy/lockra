import type { Matching } from "@lockra/shared";
import type { ReactNode } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { Badge } from "./Badge";
import { Icon } from "./Icon";

/** A section of a login's details: its heading, then rows on hairlines. */
export function DetailSection({
  title,
  children,
  aside,
  "data-testid": testId,
}: {
  title: ReactNode;
  children: ReactNode;
  aside?: ReactNode;
  "data-testid"?: string;
}) {
  return (
    <section className="mt-4 first:mt-0" data-testid={testId}>
      <div className="mb-1 flex items-center justify-between gap-2">
        <h3 className="text-[12px] font-medium text-fg-muted">{title}</h3>
        {aside}
      </div>
      <div className="flex flex-col divide-y divide-border">{children}</div>
    </section>
  );
}

export interface FieldRowProps {
  label: ReactNode;
  /** What shows: the value, the masked dots, or a code. */
  children: ReactNode;
  /** A secret: shown in mono (the dots spaced as the design has them). */
  secret?: boolean;
  /** The buttons at the right (copy; show and copy for a secret). */
  actions?: ReactNode;
  /** `lg` is the phone's (board 24): a 64 px row, the value in 15 px. */
  size?: "md" | "lg";
  "data-testid"?: string;
}

/** A field of a login (board 05): its label above its value, the buttons at the right. */
export function FieldRow({
  label,
  children,
  secret = false,
  actions,
  size = "md",
  "data-testid": testId,
}: FieldRowProps) {
  const lg = size === "lg";
  return (
    <div
      className={cx("flex items-center gap-2", lg ? "min-h-16 py-2.5" : "min-h-13 py-2")}
      data-testid={testId}>
      <div className="min-w-0 flex-1">
        <div className="text-[12px] text-fg-subtle">{label}</div>
        <div
          className={cx(
            "truncate text-fg",
            secret
              ? cx("mono tracking-[1.5px]", lg ? "text-[14px]" : "text-[13px]")
              : lg
                ? "text-[15px]"
                : "text-[14px]",
          )}>
          {children}
        </div>
      </div>
      {actions !== undefined && <div className="flex shrink-0 items-center gap-1">{actions}</div>}
    </div>
  );
}

/** A label and its value on one line (the protection level, when it was made). */
export function KeyValueRow({
  label,
  children,
  size = "md",
  "data-testid": testId,
}: {
  label: ReactNode;
  children: ReactNode;
  /** `lg` is the phone's: a 49 px row in 15 px. */
  size?: "md" | "lg";
  "data-testid"?: string;
}) {
  const text = size === "lg" ? "text-[15px]" : "text-[13px]";
  return (
    <div
      className={cx(
        "flex items-center justify-between gap-3 py-1.5",
        size === "lg" ? "min-h-[49px]" : "min-h-10",
      )}
      data-testid={testId}>
      <span className={cx("min-w-0 truncate text-fg-muted", text)}>{label}</span>
      <span className={cx("min-w-0 truncate text-right text-fg", text)}>{children}</span>
    </div>
  );
}

/** A site saved with a login and how it is matched. */
export function UrlRow({
  url,
  matching,
  size = "md",
}: {
  url: string;
  matching: Matching;
  /** `lg` is the phone's: a 49 px row. */
  size?: "md" | "lg";
}) {
  const t = useT();
  return (
    <div
      className={cx("flex items-center gap-2 py-1.5", size === "lg" ? "min-h-[49px]" : "min-h-11")}
      data-testid="url-row">
      <Icon name="globe" size={size === "lg" ? 16 : 14} className="shrink-0 text-fg-subtle" />
      <span
        className={cx(
          "mono min-w-0 flex-1 truncate text-fg",
          size === "lg" ? "text-[14px]" : "text-[13px]",
        )}
        title={url}>
        {url}
      </span>
      <Badge tone="neutral" className="shrink-0">
        {t(`item.matching.${matching}`)}
      </Badge>
    </div>
  );
}
