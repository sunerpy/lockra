import { type Backend, resolveLocale } from "@lockra/shared";
import {
  BackendProvider,
  I18nProvider,
  ToastViewport,
  useBackend,
  useT,
  useToasts,
} from "@lockra/ui";
import { type ComponentType, Suspense, lazy, useEffect } from "react";
import { useAppearance } from "./app/appearance";
import { ToasterProvider, useToaster } from "./app/notices";
import { Main } from "./shell/Main";

/** The component showcase, in development builds only (`#showcase`); a release bundle has no
 *  trace of it (scripts/check-web-bundle.sh). */
const Showcase: ComponentType | null = import.meta.env.DEV
  ? lazy(() => import("./pages/Showcase"))
  : null;

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
  const showcase =
    Showcase !== null && typeof location !== "undefined" && location.hash.startsWith("#showcase");
  return (
    <I18nProvider locale={locale} documentLang>
      <ToasterProvider store={toasts}>
        <NoticeBridge />
        <TrayBridge />
        {showcase && Showcase ? (
          <Suspense fallback={null}>
            <Showcase />
          </Suspense>
        ) : (
          <Main />
        )}
        <ToastViewport toasts={toasts.toasts} onDismiss={toasts.dismiss} />
      </ToasterProvider>
    </I18nProvider>
  );
}

/** The system tray while Lockra keeps running in the background (Windows, macOS), its menu in the
 *  app's language; none otherwise, and closing the window quits. */
function TrayBridge() {
  const { backend, state } = useBackend();
  const t = useT();
  const ready = state !== undefined && state !== null;
  const on = ready && state.settings.run_in_background && state.platform !== "linux";
  const [open, lock, quit, tooltip] = [
    t("shell.tray.open"),
    t("shell.tray.lock"),
    t("shell.tray.quit"),
    t("shell.tray.tooltip"),
  ];
  useEffect(() => {
    if (!ready) return;
    void backend.setTray(on ? { open, lock, quit, tooltip } : null).catch(() => undefined);
  }, [backend, ready, on, open, lock, quit, tooltip]);
  return null;
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
