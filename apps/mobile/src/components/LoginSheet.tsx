// A login's actions in a sheet from the bottom (board 23): copy the user name (shown beside it), the
// password, the code (the current one beside it), show the password, edit, delete. A copy runs
// while the sheet is up (a 「填充前验证」 login's proof comes in a sheet over it) and closes it; a
// page opens once the sheet's own history entry is gone, so the back gesture then closes the page.
import { type CopyPart, type ItemView, itemAccount, itemName } from "@lockra/shared";
import { useCodes, useT } from "@lockra/ui";
import { useNav } from "../app/nav";
import { useItemActions } from "../app/verify";
import { ActionRow } from "./ActionRow";
import { BottomSheet } from "./BottomSheet";

/** Run `work` once the sheet that is closing has taken its history entry away. */
export function afterSheet(close: () => void, work: () => void): void {
  window.addEventListener("popstate", () => work(), { once: true });
  close();
}

export function LoginSheet({
  item,
  onClose,
  onDelete,
  onCopied,
}: {
  item: ItemView;
  onClose: () => void;
  /** 删除…: the caller asks first. */
  onDelete: () => void;
  /** A part went to the clipboard. */
  onCopied?: (part: CopyPart) => void;
}) {
  const t = useT();
  const nav = useNav();
  const actions = useItemActions();
  const codes = useCodes();
  const account = itemAccount(item);
  const code = item.totp_entry === null ? undefined : codes.get(item.totp_entry);
  const copy = async (part: CopyPart) => {
    if (await actions.copy(item, part)) onCopied?.(part);
    onClose();
  };
  const name = account !== "" && item.title !== "" ? `${item.title} · ${account}` : itemName(item);
  return (
    <BottomSheet title={name} onClose={onClose} data-testid="login-sheet">
      <div className="flex flex-col pb-1">
        {account !== "" && (
          <ActionRow
            icon="user"
            label={t("passwords.menu.copyUsername")}
            hint={account}
            onClick={() => void copy({ type: account === item.username ? "username" : "email" })}
            testId="sheet-copy-username"
          />
        )}
        {item.has_password && (
          <ActionRow
            icon="field"
            label={t("passwords.menu.copyPassword")}
            onClick={() => void copy({ type: "password" })}
            testId="sheet-copy-password"
          />
        )}
        {item.totp_entry !== null && (
          <ActionRow
            icon="key"
            label={t("passwords.menu.copyCode")}
            hint={code?.code}
            onClick={() => void copy({ type: "totp" })}
            testId="sheet-copy-code"
          />
        )}
        {item.has_password && (
          <ActionRow
            icon="eye"
            label={t("mobile.logins.revealTitle")}
            opensPage
            onClick={() =>
              afterSheet(onClose, () => nav.open({ name: "loginReveal", id: item.id }))
            }
            testId="sheet-reveal"
          />
        )}
        {!item.read_only && (
          <ActionRow
            icon="edit"
            label={t("passwords.detail.edit")}
            opensPage
            onClick={() => afterSheet(onClose, () => nav.open({ name: "loginEdit", id: item.id }))}
            testId="sheet-edit"
          />
        )}
        <ActionRow
          icon="trash"
          label={t("passwords.menu.delete")}
          danger
          onClick={() => afterSheet(onClose, onDelete)}
          testId="sheet-delete"
        />
      </div>
    </BottomSheet>
  );
}
