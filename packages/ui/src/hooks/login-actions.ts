// Copying and showing a login's parts, as the core decides, on the desktop and the phone alike: a
// login of the 「填充前验证」 level asks the user to prove it is them before a secret (the password,
// the notes, a hidden field) is shown or copied; the others go straight through. Rust writes the
// clipboard; the webview only gets what `item_reveal` answers. Each app asks for the proof its own
// way (a dialog on the desktop, a sheet on the phone) and hands that in as `verify`.
import {
  type CopyPart,
  type ItemRevealed,
  type ItemView,
  type RevealParts,
  itemName,
} from "@lockra/shared";
import { useCallback, useMemo } from "react";
import { useBackend } from "../backend/BackendProvider";
import { useDispatch } from "./dispatch";

/** What the core takes as proof: the master password, or the words of the system check. */
export interface Proof {
  password?: string;
  reason?: string;
}

/** What the user is about to do, as the verification says it. */
export type VerifyAction = "copy" | "show" | "view";

/** Ask the user to prove it is them, then run `run` with the proof: its answer, or `undefined`
 *  when the user cancelled. */
export type Verify = <T>(
  action: VerifyAction,
  name: string,
  run: (proof: Proof) => Promise<T>,
) => Promise<T | undefined>;

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

export interface LoginActions {
  /** Copy `part`; whether it went to the clipboard. */
  copy: (item: ItemView, part: CopyPart) => Promise<boolean>;
  /** Show `parts`; the answer, or `undefined` (cancelled, or a toast told why). */
  reveal: (item: ItemView, parts: RevealParts) => Promise<ItemRevealed | undefined>;
}

export function useLoginActions(verify: Verify): LoginActions {
  const { backend } = useBackend();
  const dispatch = useDispatch();
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
