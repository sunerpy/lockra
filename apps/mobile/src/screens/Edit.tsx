// Edit an account: its names, group, pin, colour and avatar text (the desktop's edit dialog).
import { type EntryView, errorText, originText, parametersText } from "@lockra/shared";
import { AccountAppearance, Button, Input, Toggle, useBackend, useSubmit, useT } from "@lockra/ui";
import { type SubmitEvent, useState } from "react";
import { useNav } from "../app/nav";
import { GroupInput } from "../components/GroupInput";
import { Page } from "../components/Page";

export function Edit({ entry }: { entry: EntryView }) {
  const t = useT();
  const nav = useNav();
  const { backend } = useBackend();
  const [issuer, setIssuer] = useState(entry.issuer);
  const [account, setAccount] = useState(entry.account);
  const [group, setGroup] = useState(entry.group ?? "");
  const [favorite, setFavorite] = useState(entry.favorite);
  const [color, setColor] = useState(entry.color);
  const [mark, setMark] = useState(entry.mark ?? "");
  const submit = useSubmit();
  const onSubmit = async (event: SubmitEvent) => {
    event.preventDefault();
    const patch = { issuer, account, group, favorite, color, mark: mark.trim() };
    const saved = await submit.run(() =>
      backend.dispatch({ command: "entry_update", id: entry.id, patch }),
    );
    if (saved !== undefined) nav.back();
  };
  return (
    <Page title={t("entry.editTitle")} testId="page-edit">
      <form onSubmit={(e) => void onSubmit(e)} className="flex flex-col gap-4">
        <p className="mono text-[12px] text-fg-subtle">
          {`${parametersText(t, entry.kind, entry.algorithm, entry.digits)} · ${originText(t, entry.origin)}`}
        </p>
        <Input
          size="lg"
          label={t("entry.issuer")}
          value={issuer}
          onChange={(e) => setIssuer(e.target.value)}
          autoComplete="off"
        />
        <Input
          size="lg"
          label={t("entry.account")}
          value={account}
          onChange={(e) => setAccount(e.target.value)}
          autoComplete="off"
        />
        <GroupInput value={group} onChange={setGroup} />
        <Toggle checked={favorite} onChange={setFavorite} label={t("codes.favorite")} />
        <AccountAppearance
          size="lg"
          issuer={issuer}
          account={account}
          color={color}
          mark={mark}
          onColor={setColor}
          onMark={setMark}
        />
        {submit.error !== undefined && (
          <p role="alert" className="text-[13px] text-danger">
            {errorText(t, submit.error)}
          </p>
        )}
        <Button variant="primary" size="lg" type="submit" loading={submit.busy}>
          {t("entry.save")}
        </Button>
      </form>
    </Page>
  );
}
