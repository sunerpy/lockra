// The phone's app: one screen for each state of the vault (none yet, locked, unlocked), the core's
// notices as toasts, and a vault that locks as the app leaves the screen.
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
} from "@lockra/ui";
import { useEffect } from "react";
import { Codes } from "./screens/Codes";
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
  return <Codes />;
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
 *  turned off. The webview hears it as the page becoming hidden. */
function LockWhenLeaving() {
  const { backend, state } = useBackend();
  const unlocked = state?.phase === "unlocked";
  useEffect(() => {
    if (!unlocked) return undefined;
    const onChange = () => {
      if (document.visibilityState === "hidden")
        void backend.dispatch({ command: "vault_lock" }).catch(() => undefined);
    };
    document.addEventListener("visibilitychange", onChange);
    return () => document.removeEventListener("visibilitychange", onChange);
  }, [backend, unlocked]);
  return null;
}
