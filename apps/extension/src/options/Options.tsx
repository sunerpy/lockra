// The extension's options page (board 35): the connection to Lockra, the shortcuts, site access, the
// browser's own password features and how the menu opens, and what the extension keeps (nothing).
// The policy is Lockra's (Settings › Browser), not set here.
import { Badge, Button, Card, Keycaps, Logo, StatusRow, Toggle } from "@lockra/ui";
import { type ReactNode, useEffect, useState } from "react";
import type { BrowserFeatures, Feature, Holder } from "../popup/browser-features";
import type { PageClient } from "../popup/client";
import { usePage } from "../popup/usePage";
import { useText } from "../shared/text";

export const SOURCE_URL = "https://firlab.app/lockra/";

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card padding="none" className="px-[21px] pt-[18px] pb-3.5">
      <h2 className="mb-2 text-[15px] font-semibold">{title}</h2>
      {children}
    </Card>
  );
}

export function Options({ client, locale }: { client: PageClient; locale: string }) {
  const t = useText();
  const { state, send } = usePage(client);
  const [shortcuts, setShortcuts] = useState({ fill: "", popup: "" });
  const [copied, setCopied] = useState(false);
  const [features, setFeatures] = useState<BrowserFeatures | null>(null);
  const [onFocus, setOnFocus] = useState(true);
  useEffect(() => {
    void client.shortcuts().then(setShortcuts);
    void client.browserFeatures().then(setFeatures);
    void client.menuOnFocus().then(setOnFocus);
  }, [client]);

  const paired =
    state !== null &&
    (state.screen === "ready" || state.screen === "locked" || state.pairedAt !== null);
  const stateLabel =
    state?.screen === "ready"
      ? t("state.unlocked")
      : state?.screen === "locked"
        ? t("state.locked")
        : state?.screen === "not_running"
          ? t("state.notRunning")
          : null;
  const connection = paired
    ? [
        t("options.paired"),
        state?.appVersion ? `Lockra ${state.appVersion}` : null,
        stateLabel,
        state?.pairedAt
          ? t("options.pairedOn", {
              date: new Intl.DateTimeFormat(locale, { month: "long", day: "numeric" }).format(
                state.pairedAt,
              ),
            })
          : null,
      ]
        .filter(Boolean)
        .join(" · ")
    : [t("options.notPaired"), stateLabel].filter(Boolean).join(" · ");

  // In the click, like the site access: taking a feature over asks for the `privacy` permission.
  const feature = (which: Feature, take: boolean) => {
    void client.setBrowserFeature(which, take).then(setFeatures);
  };
  const featureHelp = (holder: Holder, help: string) =>
    holder === "locked" ? t("options.lockedHelp") : help;

  const access = (grant: boolean) => {
    // In the click: the browser asks only from a user's action.
    void (grant ? client.requestAccess() : client.removeAccess()).then(() =>
      send({ kind: "access.changed" }),
    );
  };

  return (
    <div className="app-canvas min-h-screen px-6 py-14 text-fg">
      <div className="mx-auto flex max-w-[640px] flex-col gap-3.5">
        <header className="mb-[7px] flex items-center gap-2.5">
          <Logo size={36} />
          <h1 className="text-[22px] font-bold">{t("name")}</h1>
          {state ? (
            <Badge mono className="ml-1">
              {state.extensionVersion}
            </Badge>
          ) : null}
        </header>
        <Section title={t("options.connection")}>
          <StatusRow label={t("options.lockra")} help={connection}>
            {paired ? (
              <Button
                variant="text-danger"
                size="sm"
                onClick={() => send({ kind: "unpair" })}
                title={t("options.unpairNote")}>
                {t("options.unpair")}
              </Button>
            ) : null}
          </StatusRow>
        </Section>
        <Section title={t("options.shortcuts")}>
          <StatusRow label={t("options.fillShortcut")} help={t("options.fillShortcutHelp")}>
            {shortcuts.fill ? (
              <Keycaps keys={shortcuts.fill} />
            ) : (
              <span className="text-[12px] text-fg-subtle">{t("options.unset")}</span>
            )}
          </StatusRow>
          <StatusRow label={t("options.popupShortcut")}>
            {shortcuts.popup ? (
              <Keycaps keys={shortcuts.popup} />
            ) : (
              <span className="text-[12px] text-fg-subtle">{t("options.unset")}</span>
            )}
          </StatusRow>
          <p className="pt-2 text-[12px] text-fg-subtle">
            {client.target === "firefox"
              ? t("options.shortcutsFirefox")
              : t("options.shortcutsChromium")}
          </p>
        </Section>
        <Section title={t("options.access")}>
          <StatusRow
            label={t("options.accessLabel")}
            help={state?.pageAccess ? t("options.accessOn") : t("options.accessOff")}>
            {state?.pageAccess ? (
              <Button size="sm" onClick={() => access(false)}>
                {t("options.revoke")}
              </Button>
            ) : (
              <Button variant="primary" size="sm" onClick={() => access(true)}>
                {t("options.grant")}
              </Button>
            )}
          </StatusRow>
        </Section>
        <Section title={t("options.browserOwn")}>
          {features ? (
            <>
              <StatusRow
                label={t("options.takePasswords")}
                help={featureHelp(features.passwords, t("options.takePasswordsHelp"))}>
                <Toggle
                  checked={features.passwords === "lockra"}
                  disabled={features.passwords === "locked"}
                  onChange={(take) => feature("passwords", take)}
                  ariaLabel={t("options.takePasswords")}
                />
              </StatusRow>
              <p className="pb-2 text-[12px] text-fg-subtle">{t("options.savedPasswordsNote")}</p>
              {client.openPasswordManager ? (
                <div className="pb-2">
                  <Button size="sm" onClick={client.openPasswordManager}>
                    {t("options.openPasswordManager")}
                  </Button>
                </div>
              ) : null}
              {features.forms === null ? null : (
                <StatusRow
                  label={t("options.takeForms")}
                  help={featureHelp(features.forms, t("options.takeFormsHelp"))}>
                  <Toggle
                    checked={features.forms === "lockra"}
                    disabled={features.forms === "locked"}
                    onChange={(take) => feature("forms", take)}
                    ariaLabel={t("options.takeForms")}
                  />
                </StatusRow>
              )}
            </>
          ) : null}
          <StatusRow label={t("options.menuOnFocus")} help={t("options.menuOnFocusHelp")}>
            <Toggle
              checked={onFocus}
              onChange={(on) => {
                setOnFocus(on);
                void client.setMenuOnFocus(on);
              }}
              ariaLabel={t("options.menuOnFocus")}
            />
          </StatusRow>
        </Section>
        <Section title={t("options.about")}>
          <StatusRow label={t("options.keepsNothing")} help={t("options.keepsNothingHelp")} />
          <StatusRow label={t("options.source")} help="firlab.app/lockra">
            <Button
              variant="text"
              size="sm"
              icon="link"
              onClick={() =>
                void client.copyText(SOURCE_URL).then(
                  () => setCopied(true),
                  () => undefined,
                )
              }>
              {copied ? t("options.linkCopied") : t("options.copyLink")}
            </Button>
          </StatusRow>
        </Section>
      </div>
    </div>
  );
}
