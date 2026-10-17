// The extension's own update (background/update.ts), as the popup (under its header, in every
// screen) and the options page (in the connection) tell it: a new copy waits for them to close,
// Lockra's folder is behind, or the extension came another way and is older than Lockra.
import { Icon, cx } from "@lockra/ui";
import type { PageClient } from "../popup/client";
import type { PopupState } from "./messages";
import { useText } from "./text";

export function UpdateNote({
  state,
  target,
  className,
}: {
  state: PopupState;
  target: PageClient["target"];
  className?: string;
}) {
  const t = useText();
  const versions = { extension: state.extensionVersion, app: state.appVersion ?? "?" };
  const text =
    state.update === "waiting"
      ? t("selfUpdate.waiting")
      : state.update === "folder"
        ? t("selfUpdate.folder", versions)
        : target === "chromium"
          ? t("selfUpdate.manual", versions)
          : t("selfUpdate.manualFirefox", versions);
  return (
    <p
      role="status"
      className={cx(
        "flex shrink-0 items-start gap-1.5 rounded-md border-l-[3px] border-l-warning bg-warning-soft px-2.5 py-1.5 text-[11px] leading-4 text-fg",
        className,
      )}
      data-testid="popup-update">
      <Icon
        name={state.update === "waiting" ? "refresh" : "puzzle"}
        size={13}
        className="mt-px shrink-0"
      />
      <span>{text}</span>
    </p>
  );
}
