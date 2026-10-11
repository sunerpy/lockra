import { mockEntry } from "@lockra/shared/mock";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PinnedGroupSelect } from "./PinnedGroupSelect";

const entries = [
  mockEntry("GitHub", "me", { group: "Work" }),
  mockEntry("Bank", "me", { group: "Money" }),
  mockEntry("Mail", "me"),
].map((e) => e.view);

describe("PinnedGroupSelect", () => {
  it("offers none, the pinned accounts, each group and no group, and hands on the pin", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    const { rerender } = render(
      <PinnedGroupSelect entries={entries} pinned={null} onChange={onChange} />,
    );
    const select = screen.getByRole("combobox", { name: "置顶分组" });
    expect([...select.querySelectorAll("option")].map((o) => o.textContent)).toEqual([
      "不置顶",
      "收藏",
      "Money",
      "Work",
      "未分组",
    ]);
    await user.selectOptions(select, "收藏");
    expect(onChange).toHaveBeenLastCalledWith({ type: "favorites" });
    await user.selectOptions(select, "Work");
    expect(onChange).toHaveBeenLastCalledWith({ type: "group", name: "Work" });
    await user.selectOptions(select, "未分组");
    expect(onChange).toHaveBeenLastCalledWith({ type: "ungrouped" });
    rerender(
      <PinnedGroupSelect entries={entries} pinned={{ type: "ungrouped" }} onChange={onChange} />,
    );
    await user.selectOptions(select, "不置顶");
    expect(onChange).toHaveBeenLastCalledWith(null);
    // A group no account has any more reads as none.
    rerender(
      <PinnedGroupSelect
        entries={entries}
        pinned={{ type: "group", name: "Gone" }}
        onChange={onChange}
      />,
    );
    expect(screen.getByRole("combobox", { name: "置顶分组" })).toHaveDisplayValue("不置顶");
  });
});
