import { moveItem } from "@lockra/shared";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { I18nProvider } from "../i18n/I18nProvider";
import { DragHandle, SortableItem, SortableList } from "./Sortable";

function List({ onMove }: { onMove?: (id: string, over: string) => void }) {
  const [ids, setIds] = useState(["a", "b", "c"]);
  return (
    <I18nProvider locale="zh-CN">
      <SortableList
        ids={ids}
        onMove={(id, over) => {
          onMove?.(id, over);
          setIds((now) => moveItem(now, id, over));
        }}>
        {ids.map((id) => (
          <SortableItem key={id} id={id} label={`Item ${id}`}>
            {(handle) => (
              <div data-testid="item">
                {id}
                <DragHandle handle={handle} />
              </div>
            )}
          </SortableItem>
        ))}
      </SortableList>
    </I18nProvider>
  );
}

describe("SortableList", () => {
  it("moves the focused handle's item one place with the arrow keys, and keeps the focus", async () => {
    const user = userEvent.setup();
    const onMove = vi.fn();
    render(<List onMove={onMove} />);
    const order = () => screen.getAllByTestId("item").map((item) => item.textContent);
    const handle = screen.getByRole("button", { name: "移动「Item a」：拖动，或按上下方向键" });
    expect(handle).toHaveAttribute("aria-roledescription", "可排序的项");
    handle.focus();
    await user.keyboard("{ArrowDown}");
    expect(onMove).toHaveBeenLastCalledWith("a", "b");
    expect(order()).toEqual(["b", "a", "c"]);
    expect(document.activeElement).toBe(
      screen.getByRole("button", { name: "移动「Item a」：拖动，或按上下方向键" }),
    );
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(order()).toEqual(["b", "c", "a"]);
    // Past the end nothing moves.
    expect(onMove).toHaveBeenCalledTimes(2);
    await user.keyboard("{ArrowUp}");
    expect(order()).toEqual(["b", "a", "c"]);
  });
});
