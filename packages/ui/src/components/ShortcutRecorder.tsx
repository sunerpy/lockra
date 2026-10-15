import {
  type Platform,
  heldModifiers,
  isModifierKey,
  shortcutFromKey,
  shortcutLabels,
} from "@lockra/shared";
import { type KeyboardEvent, useId, useRef, useState } from "react";
import { cx } from "../cx";
import { useT } from "../i18n/I18nProvider";
import { IconButton } from "./IconButton";
import { Keycap } from "./Keycap";

export interface ShortcutRecorderProps {
  /** The accelerator (`CommandOrControl+Shift+Space`), or `null` for none. */
  value: string | null;
  /** The accelerator recorded, or `null` once cleared. */
  onChange: (accelerator: string | null) => void;
  /** Whose keys the caps name (⌘ on a Mac, Ctrl elsewhere). */
  platform: Platform;
  /** The field's accessible name. */
  label: string;
  /** The shortcut could not be registered: drawn in the danger colour. */
  invalid?: boolean;
  "data-testid"?: string;
}

/** A global shortcut's field (Settings › 通用 › 快捷搜索): the shortcut as keycaps; focused, it
 *  records the next key pressed with its modifiers (the modifiers show as they are held), Esc
 *  cancels and Tab moves on; the clear button turns the shortcut off. The field draws like an
 *  input, 32 px high. */
export function ShortcutRecorder({
  value,
  onChange,
  platform,
  label,
  invalid = false,
  ...rest
}: ShortcutRecorderProps) {
  const t = useT();
  const field = useRef<HTMLButtonElement>(null);
  const shownId = useId();
  const [recording, setRecording] = useState(false);
  const [held, setHeld] = useState<readonly string[]>([]);
  const [refused, setRefused] = useState(false);

  const start = () => {
    setRecording(true);
    setHeld([]);
    setRefused(false);
  };
  const stop = () => {
    setRecording(false);
    setHeld([]);
    setRefused(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    const modified = event.ctrlKey || event.altKey || event.metaKey;
    // Tab moves on, recording or not; the focus leaving ends the recording.
    if (event.key === "Tab" && !modified) return;
    if (!recording) {
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        start();
      }
      return;
    }
    // Recording: every key is the field's, Esc included (the dialog around it stays open).
    event.preventDefault();
    if (event.key === "Escape" && !modified && !event.shiftKey) {
      stop();
      return;
    }
    if (isModifierKey(event.code)) {
      setHeld(heldModifiers(event, platform));
      setRefused(false);
      return;
    }
    const accelerator = shortcutFromKey(event, platform);
    if (accelerator === null) {
      setHeld([]);
      setRefused(true);
      return;
    }
    stop();
    onChange(accelerator);
  };

  const keys = recording ? held : value === null ? [] : shortcutLabels(value, platform);
  const status = recording
    ? refused
      ? t(platform === "macos" ? "ui.shortcut.needsModifierMac" : "ui.shortcut.needsModifier")
      : held.length === 0
        ? t("ui.shortcut.recording")
        : "…"
    : value === null
      ? t("ui.shortcut.none")
      : null;

  return (
    <span className="inline-flex items-center gap-1.5" data-testid={rest["data-testid"]}>
      <button
        ref={field}
        type="button"
        aria-label={label}
        aria-describedby={shownId}
        aria-invalid={invalid || undefined}
        data-recording={recording || undefined}
        onFocus={start}
        onBlur={stop}
        // WebKit on the Mac does not focus a button it was clicked on: the click does.
        onClick={() => {
          field.current?.focus();
          if (!recording) start();
        }}
        onKeyDown={onKeyDown}
        onKeyUp={(event) => {
          if (recording && isModifierKey(event.code)) setHeld(heldModifiers(event, platform));
        }}
        className={cx(
          "flex h-8 min-w-[168px] items-center rounded-6 bg-surface px-2.5 text-[12px] outline-none hairline transition-colors",
          "focus-visible:border-fg focus-visible:shadow-[0_0_0_1px_var(--fg)]",
          recording && "border-fg shadow-[0_0_0_1px_var(--fg)]",
          invalid && !recording && "border-danger",
        )}>
        <span aria-hidden className="flex min-w-0 items-center gap-1">
          {keys.map((key, i) => (
            <Keycap key={`${key}-${i}`}>{key}</Keycap>
          ))}
          {status !== null && (
            <span
              className={cx(
                "truncate",
                refused ? "text-danger" : recording ? "text-fg-muted" : "text-fg-subtle",
              )}>
              {status}
            </span>
          )}
        </span>
        {/* What the caps show, as words for assistive technology (the field's description). */}
        <span id={shownId} hidden>
          {[...keys, ...(status === null ? [] : [status])].join(" ")}
        </span>
      </button>
      {value !== null && (
        <IconButton icon="x" label={t("ui.shortcut.clear")} onClick={() => onChange(null)} />
      )}
    </span>
  );
}
