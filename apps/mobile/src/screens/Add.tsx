// Add accounts: paste otpauth links, read the clipboard (both go through the import preview,
// where nothing is added before the user confirms), or type one in by hand.
import { Button, Card, Textarea, useDispatch, useT } from "@lockra/ui";
import { useState } from "react";
import { useNav } from "../app/nav";
import { ActionRow } from "../components/ActionRow";
import { Page } from "../components/Page";

export function Add() {
  const t = useT();
  const nav = useNav();
  const dispatch = useDispatch();
  const [links, setLinks] = useState("");
  const read = async () => {
    // The links are secrets: they leave React state as soon as the core has them.
    if ((await dispatch({ command: "import_text", text: links })) === undefined) return;
    setLinks("");
    nav.open({ name: "preview" });
  };
  const readClipboard = async () => {
    if ((await dispatch({ command: "import_clipboard" })) !== undefined)
      nav.open({ name: "preview" });
  };
  return (
    <Page title={t("mobile.add.title")} testId="page-add">
      <div className="flex flex-col gap-4">
        <Card padding="none" className="flex flex-col gap-3 p-4">
          <h2 className="text-[15px] font-medium text-fg">{t("mobile.add.links")}</h2>
          <p className="text-[13px] text-fg-muted">{t("mobile.add.linksBody")}</p>
          <Textarea
            mono
            rows={4}
            value={links}
            onChange={(e) => setLinks(e.target.value)}
            placeholder={t("import.text.placeholder")}
            aria-label={t("mobile.add.links")}
            autoComplete="off"
            spellCheck={false}
            data-testid="add-links"
          />
          <Button
            variant="primary"
            size="lg"
            disabled={links.trim() === ""}
            onClick={() => void read()}>
            {t("import.text.read")}
          </Button>
        </Card>
        <Card padding="none" className="p-1">
          <ActionRow
            icon="clipboard"
            label={t("codes.add.clipboard")}
            hint={t("mobile.add.clipboardHint")}
            onClick={() => void readClipboard()}
            testId="add-clipboard"
          />
          <ActionRow
            icon="key"
            label={t("entry.manualTitle")}
            hint={t("mobile.add.manualHint")}
            opensPage
            onClick={() => nav.open({ name: "manual" })}
            testId="add-manual"
          />
        </Card>
      </div>
    </Page>
  );
}
