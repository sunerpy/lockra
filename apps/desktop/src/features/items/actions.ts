// Copying and showing a login's parts, as the core decides: a login of the 「填充前验证」 level asks
// the user to prove it is them before a secret (the password, the notes, a hidden field) is shown
// or copied; the others go straight through. Rust writes the clipboard; the webview only gets
// what `item_reveal` answers.
import {
  type CopyPart,
  type ItemRevealed,
  type ItemView,
  type RevealParts,
  itemName,
} from "@lockra/shared";
import { useBackend } from "@lockra/ui";
import { useCallback, useMemo } from "react";
import { useDispatch } from "../../app/dispatch";
import { useVerify } from "./verify";

/** The part copies a secret: the password, or a hidden custom field. */
export function copiesSecret(item: ItemView, part: CopyPart): boolean {
  if (part.type === "password") return true;
  if (part.type === "field") return item.fields.some((f) => f.id === part.id && f.hidden);
  return false;
}

/** This login asks for proof before its secrets: the `confirm` level. */
export function asksFirst(item: ItemView): boolean {
  return item.protection === "confirm";
}

export interface ItemActions {
  /** Copy `part`; whether it went to the clipboard. */
  copy: (item: ItemView, part: CopyPart) => Promise<boolean>;
  /** Show `parts`; the answer, or `undefined` (cancelled, or a toast told why). */
  reveal: (item: ItemView, parts: RevealParts) => Promise<ItemRevealed | undefined>;
}

export function useItemActions(): ItemActions {
  const { backend } = useBackend();
  const dispatch = useDispatch();
  const verify = useVerify();
  const copy = useCallback(
    async (item: ItemView, part: CopyPart) => {
      if (asksFirst(item) && copiesSecret(item, part)) {
        const done = await verify("copy", itemName(item), (proof) =>
          backend.dispatch({ command: "item_copy", id: item.id, part, ...proof }),
        );
        return done !== undefined;
      }
      return (await dispatch({ command: "item_copy", id: item.id, part })) !== undefined;
    },
    [backend, dispatch, verify],
  );
  const reveal = useCallback(
    async (item: ItemView, parts: RevealParts) => {
      if (asksFirst(item))
        return verify("show", itemName(item), (proof) =>
          backend.dispatch({ command: "item_reveal", id: item.id, parts, ...proof }),
        );
      return dispatch({ command: "item_reveal", id: item.id, parts });
    },
    [backend, dispatch, verify],
  );
  return useMemo(() => ({ copy, reveal }), [copy, reveal]);
}
