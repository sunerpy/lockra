import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { Dialog } from "./Dialog";
import { GroupCombobox } from "./GroupCombobox";

/** The field in a real dialog, whose Esc listener runs on `document` in the capture phase. */
function Harness({ start = "", onEscape }: { start?: string; onEscape?: () => void }) {
  const [value, setValue] = useState(start);
  return (
    <I18nProvider locale="zh-CN">
      <Dialog open title="编辑" actions={null} onClose={() => onEscape?.()}>
        <GroupCombobox
          label="分组"
          value={value}
          onChange={setValue}
          groups={["Work", "Money", "Home"]}
          placeholder="例如：工作"
        />
        <output data-testid="value">{value}</output>
      </Dialog>
    </I18nProvider>
  );
}

describe("GroupCombobox", () => {
  it("lists the groups in use, no group, and a new one for what was typed", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const input = screen.getByRole("combobox", { name: "分组" });
    expect(input).toHaveAttribute("aria-expanded", "false");
    await user.click(input);
    const list = screen.getByRole("listbox");
    expect(input).toHaveAttribute("aria-expanded", "true");
    expect(
      within(list)
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["不分组", "Work", "Money", "Home"]);
    // Typing filters the groups and offers the new one.
    await user.type(input, "mo");
    expect(
      within(screen.getByRole("listbox"))
        .getAllByRole("option")
        .map((o) => o.textContent),
    ).toEqual(["Money", "新建分组「mo」"]);
    await user.click(screen.getByRole("option", { name: "Money" }));
    expect(screen.getByTestId("value")).toHaveTextContent("Money");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("moves with the arrow keys, picks with Enter and keeps Esc from the dialog", async () => {
    const user = userEvent.setup();
    const onEscape = vi.fn();
    render(<Harness start="Work" onEscape={onEscape} />);
    const input = screen.getByRole("combobox", { name: "分组" });
    // Focus opens the list on the current group; ↓ moves to the next one.
    input.focus();
    expect(await screen.findByRole("option", { selected: true })).toHaveTextContent("Work");
    await user.keyboard("{ArrowDown}{Enter}");
    expect(screen.getByTestId("value")).toHaveTextContent("Money");
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    // Esc closes an open list only; the dialog hears the next one.
    await user.keyboard("{ArrowDown}");
    fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(onEscape).not.toHaveBeenCalled();
    fireEvent.keyDown(input, { key: "Escape" });
    expect(onEscape).toHaveBeenCalledTimes(1);
  });

  it("clears the group with no group", async () => {
    const user = userEvent.setup();
    render(<Harness start="Home" />);
    await user.click(screen.getByRole("combobox", { name: "分组" }));
    await user.click(screen.getByRole("option", { name: "不分组" }));
    expect(screen.getByTestId("value")).toBeEmptyDOMElement();
  });
});
