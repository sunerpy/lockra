// Copying and showing a login's parts (@lockra/ui `useLoginActions`), the proof a 「填充前验证」
// login asks for coming from the desktop's verification dialog.
import { type LoginActions, useLoginActions } from "@lockra/ui";
import { useVerify } from "./verify";

export function useItemActions(): LoginActions {
  return useLoginActions(useVerify());
}
