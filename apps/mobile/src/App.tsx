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
import {
  FingerprintChoiceProvider,
  FingerprintOnboarding,
} from "./components/FingerprintOnboarding";
import { Account } from "./screens/Account";
import { Add } from "./screens/Add";
import { Backup } from "./screens/Backup";
import { Codes } from "./screens/Codes";
import { Edit } from "./screens/Edit";
import { Export } from "./screens/Export";
import { ExportView } from "./screens/ExportView";
import { Manual } from "./screens/Manual";
import { Password } from "./screens/Password";
import { Preview } from "./screens/Preview";
import { Restore } from "./screens/Restore";
import { Reveal } from "./screens/Reveal";
import { Settings } from "./screens/Settings";
import { Sync } from "./screens/Sync";
import { SyncInvite } from "./screens/SyncInvite";
import { SyncJoin } from "./screens/SyncJoin";
import { SyncKey } from "./screens/SyncKey";
import { SyncSetup } from "./screens/SyncSetup";
import { SyncStorage } from "./screens/SyncStorage";
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
        <FingerprintChoiceProvider>
          <Screen />
          <FingerprintOnboarding />
        </FingerprintChoiceProvider>
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
  const { route, home, replace } = useNav();
  const { backend } = useBackend();
  const { entries, import: pending, restore, sync } = useUiState();
  // Leaving the import preview or the restore before they are done (their buttons, the back
  // gesture) ends them.
  const last = useRef(route);
  useEffect(() => {
    const was = last.current;
    last.current = route;
    if (was?.name === route?.name) return;
    if (was?.name === "preview" && pending !== null)
      void backend.dispatch({ command: "import_cancel" }).catch(() => undefined);
    if (was?.name === "restore" && restore !== null && route?.name !== "preview")
      void backend.dispatch({ command: "restore_cancel" }).catch(() => undefined);
    // An export's codes go with their page: the session closes however the page is left.
    if (was?.name === "exportView")
      void backend
        .dispatch({ command: "export_close", session: was.started.session })
        .catch(() => undefined);
  }, [route, pending, restore, backend]);
  const id = route !== undefined && "id" in route ? route.id : undefined;
  const entry = id === undefined ? undefined : entries.find((e) => e.id === id);
  // What the top page shows: the account, the import, the backup being restored, the sync space
  // whose storage changes.
  const present =
    route?.name === "preview"
      ? pending !== null
      : route?.name === "restore"
        ? restore !== null
        : route?.name === "syncStorage"
          ? sync.space !== null
          : id === undefined || entry !== undefined;
  // Once that was there and is gone (the account deleted, replaced by an import or merged away by a
  // sync; the import or the restore done), the page closes: a merged restore goes on to the import
  // preview, the rest back to the codes. Before it has been there, its state is still on the way
  // (the command's answer and the state event come by different routes).
  const seen = useRef<typeof route>(undefined);
  useEffect(() => {
    if (route === undefined) return;
    if (present) {
      seen.current = route;
      return;
    }
    if (seen.current !== route) return;
    if (route.name === "restore" && pending !== null) replace({ name: "preview" });
    else home();
  }, [route, present, pending, home, replace]);
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
    case "backup":
      return <Backup />;
    case "restore":
      return restore ? <Restore restore={restore} /> : null;
    case "export":
      return <Export />;
    case "exportView":
      return <ExportView started={route.started} />;
    case "sync":
      return <Sync />;
    case "syncSetup":
      return <SyncSetup />;
    case "syncKey":
      return <SyncKey syncKey={route.syncKey} password={route.password} />;
    case "syncJoin":
      return <SyncJoin />;
    case "syncInvite":
      return <SyncInvite />;
    case "syncStorage":
      return sync.space?.storage ? <SyncStorage storage={sync.space.storage} /> : null;
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
