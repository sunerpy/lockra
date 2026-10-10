// A group's name on the phone: a field that opens a sheet from the bottom of the screen with the
// groups in use, "no group" and a new group's name, each a 44 px row (the webview's own pickers
// are the old Android dialogs).
import { entryGroups } from "@lockra/shared";
import { Button, Icon, Input, useT, useUiState } from "@lockra/ui";
import { useId, useState } from "react";
import { BottomSheet } from "./BottomSheet";

export function GroupInput({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const t = useT();
  const { entries } = useUiState();
  const groups = entryGroups(entries);
  const labelId = useId();
  const [open, setOpen] = useState(false);
  const [fresh, setFresh] = useState("");
  const name = value.trim();
  const pick = (group: string) => {
    onChange(group);
    setOpen(false);
  };
  const row = (group: string, label: string) => (
    <button
      key={group === "" ? "\u0000" : group}
      type="button"
      role="option"
      aria-selected={group === name}
      onClick={() => pick(group)}
      className="flex min-h-11 w-full items-center gap-3 rounded-10 px-3 text-left text-[15px] text-fg active:bg-inset">
      <Icon name="folder" size={18} className="shrink-0 text-fg-subtle" />
      <span className="min-w-0 flex-1 truncate" {...(group === "" ? {} : { "data-user-text": "" })}>
        {label}
      </span>
      {group === name && <Icon name="check" size={18} className="shrink-0 text-accent" />}
    </button>
  );
  return (
    <div className="flex flex-col gap-1">
      <span id={labelId} className="text-[12px] text-fg-muted">
        {t("entry.group")}
      </span>
      <button
        type="button"
        aria-labelledby={labelId}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          setFresh("");
          setOpen(true);
        }}
        data-testid="group-field"
        className="flex h-11 items-center gap-2 rounded-6 bg-surface px-2.5 text-left text-[15px] hairline">
        <Icon name="folder" size={16} className="shrink-0 text-fg-subtle" />
        <span
          className={
            name === ""
              ? "min-w-0 flex-1 truncate text-fg-subtle"
              : "min-w-0 flex-1 truncate text-fg"
          }
          {...(name === "" ? {} : { "data-user-text": "" })}>
          {name === "" ? t("ui.groupPicker.none") : name}
        </span>
        <Icon name="chevronDown" size={16} className="shrink-0 text-fg-subtle" />
      </button>
      {open && (
        <BottomSheet
          title={t("mobile.groupSheet.title")}
          onClose={() => setOpen(false)}
          data-testid="group-sheet">
          <div role="listbox" aria-label={t("mobile.groupSheet.title")} className="flex flex-col">
            {row("", t("ui.groupPicker.none"))}
            {groups.map((group) => row(group, group))}
          </div>
          {/* No form of its own: the field sits in the page's form, whose submit must not run. */}
          <div className="flex items-end gap-2 border-t border-border px-2 pt-3 pb-2">
            <Input
              size="lg"
              label={t("mobile.groupSheet.newLabel")}
              placeholder={t("mobile.groupSheet.newPlaceholder")}
              value={fresh}
              onChange={(e) => setFresh(e.target.value)}
              onKeyDown={(e) => {
                if (e.key !== "Enter") return;
                e.preventDefault();
                if (fresh.trim() !== "") pick(fresh.trim());
              }}
              autoComplete="off"
              enterKeyHint="done"
              className="min-w-0 flex-1"
            />
            <Button
              variant="primary"
              size="lg"
              disabled={fresh.trim() === ""}
              onClick={() => pick(fresh.trim())}>
              {t("mobile.groupSheet.use")}
            </Button>
          </div>
        </BottomSheet>
      )}
    </div>
  );
}
