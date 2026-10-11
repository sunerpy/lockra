import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { type FilterChip, FilterChips } from "./FilterChips";

const OPTIONS: FilterChip[] = [
  { value: "all", label: "全部", count: 7 },
  { value: "fav", label: "收藏", count: 2, icon: "star" },
  { value: "Home", label: "Home", count: 3, userText: true },
];

function Chips({ onChange }: { onChange?: (value: string) => void }) {
  const [value, setValue] = useState("all");
  return (
    <FilterChips
      label="筛选"
      options={OPTIONS}
      value={value}
      onChange={(v) => {
        setValue(v);
        onChange?.(v);
      }}
    />
  );
}

describe("FilterChips", () => {
  it("is a radio group of pills with their counts; a click turns one on", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Chips onChange={onChange} />);
    const group = screen.getByRole("radiogroup", { name: "筛选" });
    expect(group).toHaveClass("flex-wrap");
    expect(screen.getByRole("radio", { name: "全部 7" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "全部 7" })).toHaveClass("bg-chip-on");
    expect(screen.getByRole("radio", { name: "Home 3" })).toHaveAttribute("tabindex", "-1");
    // A group's name is the user's own text.
    expect(screen.getByText("Home")).toHaveAttribute("data-user-text");
    await user.click(screen.getByRole("radio", { name: "收藏 2" }));
    expect(onChange).toHaveBeenLastCalledWith("fav");
    expect(screen.getByRole("radio", { name: "收藏 2" })).toHaveAttribute("aria-checked", "true");
    expect(screen.getByRole("radio", { name: "收藏 2" })).toHaveAttribute("tabindex", "0");
  });

  it("moves with the arrow keys, wrapping, and with Home and End", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(<Chips onChange={onChange} />);
    screen.getByRole("radio", { name: "全部 7" }).focus();
    await user.keyboard("{ArrowLeft}");
    expect(onChange).toHaveBeenLastCalledWith("Home");
    expect(screen.getByRole("radio", { name: "Home 3" })).toHaveFocus();
    await user.keyboard("{ArrowRight}");
    expect(onChange).toHaveBeenLastCalledWith("all");
    await user.keyboard("{End}");
    expect(onChange).toHaveBeenLastCalledWith("Home");
    await user.keyboard("{Home}");
    expect(onChange).toHaveBeenLastCalledWith("all");
    await user.keyboard("a");
    expect(onChange).toHaveBeenCalledTimes(4);
  });

  it("scrolls sideways on the phone, and does nothing while disabled", async () => {
    const user = userEvent.setup();
    const onChange = vi.fn();
    render(
      <FilterChips
        label="筛选"
        options={OPTIONS}
        value="missing"
        onChange={onChange}
        scroll
        size="lg"
        disabled
      />,
    );
    expect(screen.getByRole("radiogroup")).toHaveClass("overflow-x-auto");
    expect(screen.getByRole("radio", { name: "全部 7" })).toHaveClass("h-11");
    // A value no chip has: the first is the one on.
    expect(screen.getByRole("radio", { name: "全部 7" })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("radio", { name: "收藏 2" }));
    expect(onChange).not.toHaveBeenCalled();
  });
});
