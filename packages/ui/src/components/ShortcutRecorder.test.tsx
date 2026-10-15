import type { Platform } from "@lockra/shared";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { ShortcutRecorder } from "./ShortcutRecorder";

/** The recorder as Settings holds it: the value it shows is the one last chosen. */
function Field({
  initial,
  platform = "linux",
  onChange,
}: {
  initial: string | null;
  platform?: Platform;
  onChange?: (value: string | null) => void;
}) {
  const [value, setValue] = useState(initial);
  return (
    <>
      <ShortcutRecorder
        value={value}
        onChange={(next) => {
          setValue(next);
          onChange?.(next);
        }}
        platform={platform}
        label="快捷搜索的快捷键"
      />
      <button type="button">之后</button>
    </>
  );
}

const field = () => screen.getByRole("button", { name: "快捷搜索的快捷键" });

describe("ShortcutRecorder", () => {
  it("shows the shortcut as keycaps, records the next one pressed, and clears it", async () => {
    const user = userEvent.setup();
    const changes: (string | null)[] = [];
    render(<Field initial="CommandOrControl+Shift+Space" onChange={(v) => changes.push(v)} />);
    expect(field()).toHaveAccessibleDescription("Ctrl Shift Space");
    await user.click(field());
    expect(field()).toHaveFocus();
    expect(field()).toHaveAttribute("data-recording", "true");
    expect(field()).toHaveTextContent("按下快捷键…");
    // The modifiers show as they are held; the key ends the recording.
    await user.keyboard("{Control>}{Alt>}");
    expect(field()).toHaveAccessibleDescription(/^Ctrl Alt/);
    await user.keyboard("[KeyK]{/Alt}{/Control}");
    expect(changes).toEqual(["Control+Alt+KeyK"]);
    expect(field()).not.toHaveAttribute("data-recording");
    expect(field()).toHaveAccessibleDescription("Ctrl Alt K");
    // Enter on the field records again; the clear button turns the shortcut off.
    await user.keyboard("{Enter}");
    expect(field()).toHaveAttribute("data-recording", "true");
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "清除快捷键" }));
    expect(changes).toEqual(["Control+Alt+KeyK", null]);
    expect(field()).toHaveAccessibleDescription("未设置");
    expect(screen.queryByRole("button", { name: "清除快捷键" })).toBeNull();
  });

  it("refuses a key without a modifier and says which one it needs", async () => {
    const user = userEvent.setup();
    const changes: (string | null)[] = [];
    render(<Field initial={null} onChange={(v) => changes.push(v)} />);
    await user.click(field());
    await user.keyboard("[KeyK]");
    expect(field()).toHaveTextContent("要同时按住 Ctrl 或 Alt");
    await user.keyboard("{Shift>}[KeyK]{/Shift}");
    expect(changes).toEqual([]);
    expect(field()).toHaveAttribute("data-recording", "true");
  });

  it("speaks the Mac's keys there", async () => {
    const user = userEvent.setup();
    const changes: (string | null)[] = [];
    render(
      <Field
        initial="CommandOrControl+Shift+Space"
        platform="macos"
        onChange={(v) => changes.push(v)}
      />,
    );
    expect(field()).toHaveAccessibleDescription("⌘ ⇧ Space");
    await user.click(field());
    await user.keyboard("[KeyK]");
    expect(field()).toHaveTextContent("要同时按住 ⌘、⌃ 或 ⌥");
    await user.keyboard("{Meta>}[KeyK]{/Meta}");
    expect(changes).toEqual(["Command+KeyK"]);
  });

  it("Esc cancels the recording and keeps it from the dialog around; Tab moves on", async () => {
    const user = userEvent.setup();
    const changes: (string | null)[] = [];
    const escapes: boolean[] = [];
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") escapes.push(event.defaultPrevented);
    };
    document.addEventListener("keydown", onKey);
    try {
      render(<Field initial="Alt+KeyK" onChange={(v) => changes.push(v)} />);
      await user.click(field());
      await user.keyboard("{Escape}");
      expect(field()).not.toHaveAttribute("data-recording");
      expect(field()).toHaveAccessibleDescription("Alt K");
      // Not recording, Esc is the dialog's again.
      await user.keyboard("{Escape}");
      expect(escapes).toEqual([true, false]);
      // Tab leaves the field: the recording ends, and the focus moves on.
      await user.keyboard("{Enter}");
      await user.tab();
      expect(screen.getByRole("button", { name: "清除快捷键" })).toHaveFocus();
      expect(field()).not.toHaveAttribute("data-recording");
      expect(changes).toEqual([]);
    } finally {
      document.removeEventListener("keydown", onKey);
    }
  });

  it("is drawn in the danger colour while the shortcut cannot be registered", () => {
    render(
      <ShortcutRecorder
        value="Alt+KeyK"
        onChange={() => undefined}
        platform="windows"
        label="快捷搜索的快捷键"
        invalid
      />,
    );
    expect(field()).toHaveClass("border-danger");
    expect(field()).toHaveAttribute("aria-invalid", "true");
  });
});
