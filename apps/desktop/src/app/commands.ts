// The command palette's items: every account (Enter copies its code), the pages, and the actions
// of the unlocked app. A development build adds the component showcase.
import { entryLabel } from "@lockra/shared";
import {
  type CommandItem,
  type IconName,
  noteUserLock,
  useBackend,
  useT,
  useUiState,
} from "@lockra/ui";
import { useMemo } from "react";
import { useDispatch, useGuarded } from "./dispatch";
import { PAGES, type PageId, useShell } from "./shell-state";

const PAGE_ICONS = {
  codes: "key",
  import: "download",
  export: "upload",
  backup: "archive",
} as const satisfies Record<PageId, IconName>;

export function useCommands(): CommandItem[] {
  const t = useT();
  const state = useUiState();
  const shell = useShell();
  const dispatch = useDispatch();
  const guarded = useGuarded();
  const { backend } = useBackend();
  const { entries } = state;
  return useMemo(() => {
    const accounts = t("palette.group.accounts");
    const pages = t("palette.group.pages");
    const actions = t("palette.group.actions");
    const items: CommandItem[] = entries.map((entry) => ({
      id: `copy:${entry.id}`,
      group: accounts,
      label: entryLabel(entry.issuer, entry.account),
      icon: "copy",
      hint: t("palette.copyCode"),
      run: () => void dispatch({ command: "entry_copy", id: entry.id }),
    }));
    for (const page of PAGES)
      items.push({
        id: `page:${page}`,
        group: pages,
        label: t(`shell.nav.${page}`),
        icon: PAGE_ICONS[page],
        run: () => shell.navigate(page),
      });
    items.push(
      {
        id: "add-manual",
        group: actions,
        label: t("codes.add.manual"),
        icon: "plus",
        keys: "Ctrl N",
        run: () => shell.open({ type: "add_manual" }),
      },
      {
        id: "add-uri",
        group: actions,
        label: t("codes.add.uri"),
        icon: "link",
        run: () => shell.open({ type: "add_uri" }),
      },
      {
        id: "import-clipboard",
        group: actions,
        label: t("codes.add.clipboard"),
        icon: "clipboard",
        run: () => void dispatch({ command: "import_clipboard" }),
      },
      {
        id: "import-image",
        group: actions,
        label: t("codes.add.image"),
        icon: "image",
        run: () => void guarded(() => backend.pickImportFiles("images")),
      },
      {
        id: "lock",
        group: actions,
        label: t("shell.nav.lock"),
        icon: "lock",
        keys: "Ctrl L",
        run: () => {
          noteUserLock();
          void dispatch({ command: "vault_lock" });
        },
      },
      {
        id: "settings",
        group: actions,
        label: t("shell.nav.settings"),
        icon: "settings",
        keys: "Ctrl ,",
        run: () => shell.open({ type: "settings", section: "general" }),
      },
    );
    if (import.meta.env.DEV) {
      items.push({
        id: "showcase",
        group: actions,
        label: t("palette.showcase"),
        icon: "grid",
        run: () => {
          history.replaceState(null, "", "#showcase");
          location.reload();
        },
      });
    }
    return items;
  }, [t, entries, shell, dispatch, guarded, backend]);
}
