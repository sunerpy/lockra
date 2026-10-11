// A page over the codes: a bar with the way back and the title, and a body that alone scrolls,
// clear of the status bar, the navigation bar and the keyboard.
import { IconButton, useT } from "@lockra/ui";
import type { ReactNode } from "react";
import { useNav } from "../app/nav";

export function Page({
  title,
  actions,
  children,
  testId,
}: {
  title: string;
  /** Beside the title, at the end of the bar. */
  actions?: ReactNode;
  children: ReactNode;
  testId: string;
}) {
  const t = useT();
  const nav = useNav();
  return (
    <div className="flex h-full flex-col" data-testid={testId}>
      <header className="flex items-center gap-1 border-b border-border bg-surface px-1.5 pt-[max(env(safe-area-inset-top),0.5rem)] pb-1">
        <IconButton icon="chevronLeft" label={t("common.back")} size={40} onClick={nav.back} />
        <h1 className="min-w-0 flex-1 truncate text-[17px] font-semibold text-fg">{title}</h1>
        {actions}
      </header>
      <main className="min-h-0 flex-1 overflow-y-auto px-4 pt-4 pb-[max(env(safe-area-inset-bottom),1rem)]">
        {children}
      </main>
    </div>
  );
}
