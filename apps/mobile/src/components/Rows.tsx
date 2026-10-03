// The rows of the phone's settings: sections of a card each, a control under its label, a switch
// at the end of its row, and a fact.
import { Card, Toggle } from "@lockra/ui";
import type { ReactNode } from "react";

export function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="flex flex-col gap-2" aria-label={title}>
      <h2 className="px-1 text-[13px] font-medium text-fg-muted">{title}</h2>
      <Card padding="none" className="flex flex-col divide-y divide-border">
        {children}
      </Card>
    </section>
  );
}

/** A control under its label, the hint below it. */
export function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2 px-4 py-3">
      <span className="text-[15px] text-fg">{label}</span>
      {children}
      {hint !== undefined && <span className="text-[13px] text-fg-muted">{hint}</span>}
    </div>
  );
}

/** A switch at the end of its row. */
export function SwitchRow({
  label,
  hint,
  checked,
  onChange,
  disabled = false,
  testId,
}: {
  label: string;
  hint?: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  testId?: string;
}) {
  return (
    <div className="flex min-h-14 items-center gap-3 px-4 py-2" data-testid={testId}>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-[15px] text-fg">{label}</span>
        {hint !== undefined && <span className="text-[13px] text-fg-muted">{hint}</span>}
      </span>
      <Toggle
        size="lg"
        checked={checked}
        onChange={onChange}
        disabled={disabled}
        ariaLabel={label}
      />
    </div>
  );
}

/** A fact: its name, and the value at the end. */
export function InfoRow({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId?: string;
}) {
  return (
    <div className="flex min-h-12 items-center gap-3 px-4 py-2">
      <span className="flex-1 text-[15px] text-fg">{label}</span>
      <span className="mono text-[13px] text-fg-muted" data-testid={testId}>
        {value}
      </span>
    </div>
  );
}
