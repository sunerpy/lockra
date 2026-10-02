// Several accounts into one group at once, from the code list's selection: one command for all of
// them, and a notice that says where they went.
import { errorText } from "@lockra/shared";
import { Button, Dialog, useBackend, useT } from "@lockra/ui";
import { type SubmitEvent, useId, useState } from "react";
import { useSubmit } from "../../app/dispatch";
import { useToaster } from "../../app/notices";
import { GroupField } from "./EntryDialogs";

export function MoveGroupDialog({
  ids,
  onClose,
  onMoved,
}: {
  ids: readonly string[];
  onClose: () => void;
  onMoved: () => void;
}) {
  const t = useT();
  const { backend } = useBackend();
  const toaster = useToaster();
  const formId = useId();
  const [group, setGroup] = useState("");
  const submit = useSubmit();
  const accounts = t("common.accounts", { n: ids.length });
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const moved = await submit.run(() =>
      backend.dispatch({ command: "entries_set_group", ids: [...ids], group }),
    );
    if (moved === undefined) return;
    const name = group.trim();
    toaster.info(
      name === ""
        ? t("codes.move.ungrouped", { accounts })
        : t("codes.move.moved", { accounts, group: name }),
    );
    onMoved();
  };
  return (
    <Dialog
      open
      title={t("codes.move.title")}
      onClose={onClose}
      width={420}
      actions={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" type="submit" form={formId} loading={submit.busy}>
            {t("codes.move.submit")}
          </Button>
        </>
      }>
      <form id={formId} onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-3">
        <p className="text-[13px] text-fg-muted">{t("codes.move.prompt", { accounts })}</p>
        <GroupField value={group} onChange={setGroup} autoFocus />
        <p className="text-[12px] text-fg-subtle">{t("codes.move.hint")}</p>
        {submit.error !== undefined && (
          <p role="alert" className="text-[12px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
      </form>
    </Dialog>
  );
}
