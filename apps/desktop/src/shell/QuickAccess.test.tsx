import { MockBackend, sampleEntries, sampleItems } from "@lockra/shared/mock";
import { act, screen, within } from "@testing-library/react";
import { ready, renderApp } from "../test/render";

/** The app with password management on (where the quick search is offered) and the samples. */
function renderQuick() {
  const entries = sampleEntries();
  const backend = new MockBackend({
    entries,
    passwords: true,
    items: sampleItems(entries),
    settings: { locale: "zh-cn" },
  });
  return renderApp({ backend });
}

/** As the shell does on the shortcut, the tray's 「快捷搜索」 or `lockra-desktop --palette`. */
function summon(backend: MockBackend): void {
  act(() => {
    backend.summonQuickAccess();
  });
}

const palette = () => screen.queryByRole("dialog", { name: "命令菜单" });

describe("the quick search", () => {
  it("opens the command palette when the shell asks; a copy from it sends the window back", async () => {
    const { user, backend } = renderQuick();
    await ready();
    summon(backend);
    expect(await screen.findByRole("dialog", { name: "命令菜单" })).toBeInTheDocument();
    await user.keyboard("Cloudflare{Enter}");
    expect(palette()).toBeNull();
    expect(backend.calls.at(-1)).toMatchObject({ command: "entry_copy" });
    await vi.waitFor(() => expect(backend.quickAccessEnds).toEqual([true]));
    // A login's password, which Rust copies: the shell hears once the copy is done.
    summon(backend);
    await screen.findByRole("dialog", { name: "命令菜单" });
    await user.keyboard("Figma{Enter}");
    await vi.waitFor(() => expect(backend.quickAccessEnds).toEqual([true, true]));
    const copies = backend.calls.filter((c) => c.command === "item_copy");
    expect(copies[copies.length - 1]).toMatchObject({ part: { type: "password" } });
  });

  it("closed without a copy, the window stays where the user is", async () => {
    const { user, backend } = renderQuick();
    await ready();
    summon(backend);
    await screen.findByRole("dialog", { name: "命令菜单" });
    await user.keyboard("{Escape}");
    expect(backend.quickAccessEnds).toEqual([false]);
    // Another action than a copy (a page) keeps the window too.
    summon(backend);
    await screen.findByRole("dialog", { name: "命令菜单" });
    await user.keyboard("备份{Enter}");
    expect(await screen.findByTestId("page-backup")).toBeInTheDocument();
    expect(backend.quickAccessEnds).toEqual([false, false]);
    // A login that asks for proof first, and the proof not given: nothing was copied.
    summon(backend);
    await screen.findByRole("dialog", { name: "命令菜单" });
    await user.keyboard("招商银行{Enter}");
    const verify = await screen.findByRole("dialog", { name: "验证身份" });
    await user.keyboard("{Escape}");
    expect(verify).not.toBeInTheDocument();
    await vi.waitFor(() => expect(backend.quickAccessEnds).toEqual([false, false, false]));
  });

  it("the palette the user opened is not the quick search's: a copy leaves the window be", async () => {
    const { user, backend } = renderQuick();
    await ready();
    await user.keyboard("{Control>}k{/Control}");
    await screen.findByRole("dialog", { name: "命令菜单" });
    await user.keyboard("Cloudflare{Enter}");
    expect(backend.calls.at(-1)).toMatchObject({ command: "entry_copy" });
    await act(async () => {});
    expect(backend.quickAccessEnds).toEqual([]);
    // Asked for while it is open, the palette stays as it is and becomes the quick search's.
    await user.keyboard("{Control>}k{/Control}");
    await user.keyboard("Cloud");
    summon(backend);
    expect(within(palette() ?? document.body).getByRole("combobox")).toHaveValue("Cloud");
    await user.keyboard("flare{Enter}");
    await vi.waitFor(() => expect(backend.quickAccessEnds).toEqual([true]));
  });

  it("does not open over a dialog: the window shows what is open", async () => {
    const { user, backend } = renderQuick();
    await ready();
    await user.keyboard("{Control>}n{/Control}");
    const dialog = await screen.findByRole("dialog", { name: "手动添加账号" });
    summon(backend);
    expect(palette()).toBeNull();
    expect(dialog).toBeInTheDocument();
    expect(backend.quickAccessEnds).toEqual([false]);
  });

  it("opens nothing while locked: the window shows the unlock screen", async () => {
    const { user, backend } = renderQuick();
    await ready();
    await user.keyboard("{Control>}l{/Control}");
    expect(await screen.findByTestId("page-unlock")).toBeInTheDocument();
    expect(backend.summonQuickAccess()).toBe(false);
    // Even told to, the unlock screen has no palette to open.
    act(() => backend.emitNotice({ type: "open_palette" }));
    expect(palette()).toBeNull();
    expect(screen.getByTestId("page-unlock")).toBeInTheDocument();
  });
});
