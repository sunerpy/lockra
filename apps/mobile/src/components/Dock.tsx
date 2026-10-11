// The codes screen's floating dock (board A2): 验证码 (this screen), the scan button raised in the
// middle, and 设置. It sits over the list, clear of the system's gesture bar.
import { Icon, useT } from "@lockra/ui";

export function Dock({
  onCodes,
  onScan,
  onSettings,
}: {
  /** The screen it is on: back to the top of the list. */
  onCodes: () => void;
  onScan: () => void;
  onSettings: () => void;
}) {
  const t = useT();
  const item = "flex w-20 flex-col items-center gap-0.5 text-[10.5px] font-medium";
  return (
    <nav
      aria-label={t("mobile.dock.label")}
      data-dock=""
      data-testid="codes-dock"
      className="fixed inset-x-6 bottom-[calc(env(safe-area-inset-bottom)+0.75rem)] z-30 flex h-18 items-center justify-around rounded-pill bg-overlay shadow-win hairline overlay-frost">
      <button
        type="button"
        aria-current="page"
        onClick={onCodes}
        className={`${item} text-accent-text`}
        data-testid="dock-codes">
        <Icon name="key" size={20} />
        {t("mobile.dock.codes")}
      </button>
      <button
        type="button"
        aria-label={t("mobile.dock.scan")}
        title={t("mobile.dock.scan")}
        onClick={onScan}
        className="inline-flex size-16 items-center justify-center rounded-full bg-primary text-primary-fg shadow-primary active:opacity-90"
        data-testid="dock-scan">
        <Icon name="scan" size={24} />
      </button>
      <button
        type="button"
        onClick={onSettings}
        className={`${item} text-fg-muted`}
        data-testid="dock-settings">
        <Icon name="settings" size={20} />
        {t("mobile.dock.settings")}
      </button>
    </nav>
  );
}
