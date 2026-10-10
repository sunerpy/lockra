import { mockEntry } from "@lockra/shared/mock";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { I18nProvider } from "../i18n/I18nProvider";
import { ReorderList } from "./ReorderList";

const entries = [
  mockEntry("GitHub", "me", { group: "Work", at: 1 }),
  mockEntry("Bank", "me", { group: "Money", at: 2 }),
  mockEntry("Mail", "me", { at: 3 }),
  mockEntry("Jira", "me", { group: "Work", at: 4 }),
].map((e) => e.view);
const id = (issuer: string) => entries.find((e) => e.issuer === issuer)?.id ?? "";

function setup(grouped = true) {
  const onOrderEntries = vi.fn();
  const onOrderGroups = vi.fn();
  const onToggleFold = vi.fn();
  render(
    <I18nProvider locale="zh-CN">
      <ReorderList
        entries={entries}
        sort="name"
        entryOrder={[]}
        groupOrder={[]}
        grouped={grouped}
        folded={new Set(["Money"])}
        onToggleFold={onToggleFold}
        onOrderEntries={onOrderEntries}
        onOrderGroups={onOrderGroups}
        noGroupLabel="未分组"
      />
    </I18nProvider>,
  );
  return { onOrderEntries, onOrderGroups, onToggleFold };
}

const handle = (name: string) =>
  screen.getByRole("button", { name: `移动「${name}」：拖动，或按上下方向键` });

describe("ReorderList", () => {
  it("moves a group, and an account inside its group, as the whole new order", async () => {
    const user = userEvent.setup();
    const { onOrderEntries, onOrderGroups, onToggleFold } = setup();
    // The sections as shown: by name, no group last and without a handle; Money is folded.
    expect(
      screen
        .getAllByTestId("reorder-section")
        .map((s) => within(s).getAllByRole("button")[0]?.textContent),
    ).toEqual(["Money1", "Work2", "未分组1"]);
    expect(
      screen.queryByRole("button", { name: "移动「未分组」：拖动，或按上下方向键" }),
    ).toBeNull();
    expect(screen.queryByText("Bank")).toBeNull();
    handle("Work").focus();
    await user.keyboard("{ArrowUp}");
    expect(onOrderGroups).toHaveBeenLastCalledWith(["Work", "Money"]);
    // Every account in the list's order, Jira now before GitHub.
    handle("Jira").focus();
    await user.keyboard("{ArrowUp}");
    expect(onOrderEntries).toHaveBeenLastCalledWith([
      id("Bank"),
      id("Jira"),
      id("GitHub"),
      id("Mail"),
    ]);
    // Its folded section unfolds from its header.
    await user.click(screen.getByRole("button", { name: /^Money/, expanded: false }));
    expect(onToggleFold).toHaveBeenLastCalledWith("Money");
  });

  it("orders one list when the accounts are not in groups", async () => {
    const user = userEvent.setup();
    const { onOrderEntries } = setup(false);
    expect(screen.getAllByTestId("reorder-row").map((r) => r.textContent)).toEqual([
      "BBankme",
      "GGitHubme",
      "JJirame",
      "MMailme",
    ]);
    handle("Mail").focus();
    await user.keyboard("{ArrowUp}");
    expect(onOrderEntries).toHaveBeenLastCalledWith([
      id("Bank"),
      id("GitHub"),
      id("Mail"),
      id("Jira"),
    ]);
  });
});
