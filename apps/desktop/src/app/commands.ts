// The command palette's items: every account (Enter copies its code), every login (Enter copies
// its password, Tab moves to its other actions), the pages, and the actions of the unlocked app. A
// development build adds the component showcase. Each copy hands its outcome to `onCopy` (the quick
// search sends the window back after one).
import { entryLabel, itemAccount, itemName } from "@lockra/shared";
import {
  type CommandAction,
  type CommandItem,
  type IconName,
  noteUserLock,
  useBackend,
  useItems,
  useT,
  useUiState,
} from "@lockra/ui";
import { useMemo } from "react";
import { useItemActions } from "../features/items/actions";
import { useDispatch, useGuarded } from "./dispatch";
import { PAGES, type PageId, useShell } from "./shell-state";

const PAGE_ICONS = {
  codes: "key",
  passwords: "lock",
  import: "download",
  export: "upload",
  backup: "archive",
} as const satisfies Record<PageId, IconName>;

/** The palette's items; `onCopy` hears of each copy run from them, with whether it went to the
 *  clipboard. */
export function useCommands(onCopy?: (copied: Promise<boolean>) => void): CommandItem[] {
  const t = useT();
  const state = useUiState();
  const shell = useShell();
  const dispatch = useDispatch();
  const guarded = useGuarded();
  const { backend } = useBackend();
  const logins = useItems();
  const login = useItemActions();
  const { entries } = state;
  const passwords = state.features.passwords;
  return useMemo(() => {
    const copy = (work: Promise<boolean>) => {
      if (onCopy) onCopy(work);
      else void work;
    };
    const accounts = t("palette.group.accounts");
    const pages = t("palette.group.pages");
    const actions = t("palette.group.actions");
    const items: CommandItem[] = entries.map((entry) => ({
      id: `copy:${entry.id}`,
      group: accounts,
      label: entryLabel(entry.issuer, entry.account),
      icon: "copy",
      hint: t("palette.copyCode"),
      run: () =>
        copy(
          dispatch({ command: "entry_copy", id: entry.id }).then((answer) => answer !== undefined),
        ),
    }));
    if (passwords) {
      const group = t("palette.group.logins");
      for (const item of logins ?? []) {
        const account = itemAccount(item);
        const offered: CommandAction[] = [
          ...(item.has_password
            ? [
                {
                  id: "password",
                  label: t("palette.login.copyPassword"),
                  run: () => copy(login.copy(item, { type: "password" })),
                },
              ]
            : []),
          ...(account === ""
            ? []
            : [
                {
                  id: "username",
                  label: t("palette.login.copyUsername"),
                  run: () =>
                    copy(
                      login.copy(item, { type: account === item.username ? "username" : "email" }),
                    ),
                },
              ]),
          ...(item.totp_entry === null
            ? []
            : [
                {
                  id: "code",
                  label: t("palette.login.copyCode"),
                  run: () => copy(login.copy(item, { type: "totp" })),
                },
              ]),
          ...(item.has_password
            ? [
                {
                  id: "reveal",
                  label: t("palette.login.reveal"),
                  run: () => shell.open({ type: "item_reveal", id: item.id }),
                },
              ]
            : []),
        ];
        const name = itemName(item);
        items.push({
          id: `login:${item.id}`,
          group,
          label: account !== "" && account !== name ? `${name} · ${account}` : name,
          icon: "lock",
          actions: offered,
          // Nothing to copy: Enter shows the login on its page.
          run:
            offered[0]?.run ??
            (() => {
              shell.navigate("passwords");
              shell.selectItem(item.id);
            }),
        });
      }
    }
    for (const page of PAGES) {
      if (page === "passwords" && !passwords) continue;
      items.push({
        id: `page:${page}`,
        group: pages,
        label: t(`shell.nav.${page}`),
        icon: PAGE_ICONS[page],
        run: () => shell.navigate(page),
      });
    }
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
  }, [t, entries, passwords, logins, login, shell, dispatch, guarded, backend, onCopy]);
}
