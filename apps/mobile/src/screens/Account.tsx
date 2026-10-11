// One account's actions (a long press on its row, or its ⋯): edit, pin or unpin, show the secret,
// and delete after asking.
import { type EntryView, entryLabel, errorText, parametersText } from "@lockra/shared";
import {
  Button,
  Card,
  Dialog,
  EntryAvatar,
  useBackend,
  useDispatch,
  useSubmit,
  useT,
} from "@lockra/ui";
import { useState } from "react";
import { useNav } from "../app/nav";
import { ActionRow } from "../components/ActionRow";
import { Page } from "../components/Page";

export function Account({ entry }: { entry: EntryView }) {
  const t = useT();
  const nav = useNav();
  const dispatch = useDispatch();
  const [deleting, setDeleting] = useState(false);
  const name = entryLabel(entry.issuer, entry.account);
  const pin = async () => {
    const patch = { favorite: !entry.favorite };
    if ((await dispatch({ command: "entry_update", id: entry.id, patch })) !== undefined)
      nav.back();
  };
  return (
    <Page title={t("codes.actions")} testId="page-account">
      <div className="flex flex-col gap-4">
        <div className="flex items-center gap-3 px-1">
          <EntryAvatar
            issuer={entry.issuer}
            account={entry.account}
            color={entry.color}
            mark={entry.mark}
            size={40}
          />
          <div className="flex min-w-0 flex-col">
            <span className="truncate text-[16px] font-medium text-fg">
              {entry.issuer || entry.account}
            </span>
            <span className="truncate mono text-[12px] text-fg-muted">
              {entry.issuer !== "" && entry.account !== "" ? `${entry.account} · ` : ""}
              {parametersText(t, entry.kind, entry.algorithm, entry.digits)}
            </span>
          </div>
        </div>
        <Card padding="none" className="p-1">
          <ActionRow
            icon="edit"
            label={t("codes.edit")}
            opensPage
            onClick={() => nav.open({ name: "edit", id: entry.id })}
            testId="account-edit"
          />
          <ActionRow
            icon="star"
            label={entry.favorite ? t("codes.unfavorite") : t("codes.favorite")}
            onClick={() => void pin()}
            testId="account-pin"
          />
          <ActionRow
            icon="eye"
            label={t("codes.reveal")}
            opensPage
            onClick={() => nav.open({ name: "reveal", id: entry.id })}
            testId="account-reveal"
          />
          <ActionRow
            icon="trash"
            label={t("codes.remove")}
            danger
            onClick={() => setDeleting(true)}
            testId="account-delete"
          />
        </Card>
      </div>
      {deleting && <DeleteDialog entry={entry} name={name} onClose={() => setDeleting(false)} />}
    </Page>
  );
}

function DeleteDialog({
  entry,
  name,
  onClose,
}: {
  entry: EntryView;
  name: string;
  onClose: () => void;
}) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const submit = useSubmit();
  const remove = async () => {
    const done = await submit.run(() =>
      backend.dispatch({ command: "entry_delete", id: entry.id }),
    );
    if (done !== undefined) nav.home();
  };
  return (
    <Dialog
      open
      title={t("entry.deleteTitle", { name })}
      onClose={onClose}
      actions={
        <>
          <Button variant="ghost" size="lg" onClick={onClose} data-autofocus>
            {t("common.cancel")}
          </Button>
          <Button
            variant="danger"
            size="lg"
            icon="trash"
            loading={submit.busy}
            onClick={() => void remove()}>
            {t("entry.deleteConfirm")}
          </Button>
        </>
      }>
      <p>{t("entry.deleteBody")}</p>
      {submit.error !== undefined && (
        <p role="alert" className="mt-2 text-[13px] text-danger">
          {errorText(t, submit.error)}
        </p>
      )}
    </Dialog>
  );
}
