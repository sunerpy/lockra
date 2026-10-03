// The phone's app: one screen for each state of the vault (none yet, locked, unlocked; the
// unlocked vault's pages go over the codes, app/nav.tsx), the core's notices as toasts, and a
// vault that locks as the app leaves the screen.
import { type Backend, resolveLocale } from "@lockra/shared";
import {
  BackendProvider,
  I18nProvider,
  ToasterProvider,
  ToastViewport,
  useAppearance,
  useBackend,
  useToaster,
  useToasts,
  useUiState,
} from "@lockra/ui";
import { useEffect, useRef } from "react";
import { NavProvider, useNav } from "./app/nav";
import { phoneScreenOpen } from "./app/phone-screen";
import { Account } from "./screens/Account";
import { Add } from "./screens/Add";
import { Codes } from "./screens/Codes";
import { Edit } from "./screens/Edit";
import { Manual } from "./screens/Manual";
import { Password } from "./screens/Password";
import { Preview } from "./screens/Preview";
import { Reveal } from "./screens/Reveal";
import { Settings } from "./screens/Settings";
import { Unlock } from "./screens/Unlock";
import { Welcome } from "./screens/Welcome";

export function App({ backend }: { backend: Backend }) {
  return (
    <BackendProvider backend={backend}>
      <Root />
    </BackendProvider>
  );
}

function Root() {
  const { state } = useBackend();
  useAppearance(state?.settings);
  const locale = resolveLocale(
    state?.settings.locale ?? "system",
    typeof navigator === "undefined" ? "zh-CN" : navigator.language,
  );
  const toasts = useToasts();
  return (
    <I18nProvider locale={locale} documentLang>
      <ToasterProvider store={toasts}>
        <NoticeBridge />
        <LockWhenLeaving />
        <Screen />
        <ToastViewport toasts={toasts.toasts} onDismiss={toasts.dismiss} />
      </ToasterProvider>
    </I18nProvider>
  );
}

function Screen() {
  const { state } = useBackend();
  if (state === undefined) return null;
  if (state.phase === "no_vault") return <Welcome />;
  if (state.phase === "locked") return <Unlock />;
  return (
    <NavProvider>
      <Pages />
    </NavProvider>
  );
}

/** The codes, or the page on top of them. */
function Pages() {
  const { route, home } = useNav();
  const { backend } = useBackend();
  const { entries, import: pending } = useUiState();
  // Leaving the import preview any way but its own buttons (the back gesture) discards the import.
  const last = useRef(route);
  useEffect(() => {
    const was = last.current;
    last.current = route;
    if (was?.name === "preview" && route?.name !== "preview" && pending !== null)
      void backend.dispatch({ command: "import_cancel" }).catch(() => undefined);
  }, [route, pending, backend]);
  const id = route !== undefined && "id" in route ? route.id : undefined;
  const entry = id === undefined ? undefined : entries.find((e) => e.id === id);
  // The account went away meanwhile (deleted, replaced by an import, merged away by a sync), or
  // the import ended: back to the codes.
  const gone = (id !== undefined && entry === undefined) || (route?.name === "preview" && !pending);
  useEffect(() => {
    if (gone) home();
  }, [gone, home]);
  switch (route?.name) {
    case undefined:
      return <Codes />;
    case "add":
      return <Add />;
    case "manual":
      return <Manual />;
    case "preview":
      return pending ? <Preview view={pending} /> : null;
    case "settings":
      return <Settings />;
    case "password":
      return <Password />;
    case "account":
      return entry ? <Account entry={entry} /> : null;
    case "edit":
      return entry ? <Edit entry={entry} /> : null;
    case "reveal":
      return entry ? <Reveal entry={entry} /> : null;
  }
}

/** Core notices become toasts (inside the i18n tree, so they are translated). */
function NoticeBridge() {
  const { backend } = useBackend();
  const toaster = useToaster();
  useEffect(
    () => backend.on((event) => event.type === "notice" && toaster.notice(event.notice)),
    [backend, toaster],
  );
  return null;
}

/** The vault locks as soon as the app leaves the screen: another app, the home screen, the screen
 *  turned off. The webview hears it as the page becoming hidden; behind a screen of the phone's
 *  own that Lockra opened (the camera, the photo picker), it is not left (app/phone-screen.ts). */
function LockWhenLeaving() {
  const { backend, state } = useBackend();
  const unlocked = state?.phase === "unlocked";
  useEffect(() => {
    if (!unlocked) return undefined;
    const onChange = () => {
      if (document.visibilityState === "hidden" && !phoneScreenOpen())
        void backend.dispatch({ command: "vault_lock" }).catch(() => undefined);
    };
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  }, [backend, unlocked]);
  return null;
}
