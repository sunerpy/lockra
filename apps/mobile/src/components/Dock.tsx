// The floating dock of the codes and the passwords screens (boards A2, 20, 21): 验证码, the button
// raised in the middle (scan on the codes, a new login on the passwords), 密码 where password
// management is offered, and 设置. It sits over the list, clear of the system's gesture bar.
import { Icon, type IconName, useT } from "@lockra/ui";

export interface DockProps {
  /** The screen it is on. */
  page: "codes" | "logins";
  /** 验证码: on its screen, back to the top of the list. */
  onCodes: () => void;
  /** 密码; without it the dock has no 密码 (password management is not offered). */
  onLogins?: () => void;
  /** The raised button: scan on the codes, a new login on the passwords. */
  onRaised: () => void;
  onSettings: () => void;
}

export function Dock({ page, onCodes, onLogins, onRaised, onSettings }: DockProps) {
  const t = useT();
  const item = (
    name: "codes" | "logins" | "settings",
    icon: IconName,
    label: string,
    onClick: () => void,
  ) => (
    <button
      type="button"
      aria-current={name === page ? "page" : undefined}
      onClick={onClick}
      className={`flex w-[70px] flex-col items-center gap-0.5 text-[10.5px] font-medium ${name === page ? "text-accent-text" : "text-fg-muted"}`}
      data-testid={`dock-${name}`}>
      <Icon name={icon} size={20} />
      {label}
    </button>
  );
  const raised = page === "codes" ? t("mobile.dock.scan") : t("mobile.dock.newLogin");
  return (
    <nav
      aria-label={t("mobile.dock.label")}
      data-dock=""
      data-testid="codes-dock"
      className="fixed inset-x-[21px] bottom-[calc(env(safe-area-inset-bottom)+0.75rem)] z-30 flex h-18 items-center justify-around rounded-pill bg-overlay shadow-win hairline overlay-frost">
      {item("codes", "key", t("mobile.dock.codes"), onCodes)}
      <button
        type="button"
        aria-label={raised}
        title={raised}
        onClick={onRaised}
        className="inline-flex size-16 items-center justify-center rounded-full bg-primary text-primary-fg shadow-primary active:opacity-90"
        data-testid={page === "codes" ? "dock-scan" : "dock-new-login"}>
        <Icon name={page === "codes" ? "scan" : "plus"} size={24} />
      </button>
      {onLogins !== undefined && item("logins", "lock", t("mobile.dock.passwords"), onLogins)}
      {item("settings", "settings", t("mobile.dock.settings"), onSettings)}
    </nav>
  );
}
