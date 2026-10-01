import { Logo, useBackend, useT } from "@lockra/ui";
import { Unlock } from "../pages/Unlock";
import { Welcome } from "../pages/Welcome";
import { Shell } from "./Shell";
import { WindowFrame } from "./WindowFrame";

/** The phase decides the screen: welcome, unlock, or the app itself. */
export function Main() {
  const { state } = useBackend();
  const t = useT();
  if (!state) {
    return (
      <div className="flex h-full items-center justify-center bg-canvas" data-testid="splash">
        <Logo size={48} label={t("shell.product")} />
      </div>
    );
  }
  if (state.phase === "unlocked") return <Shell />;
  return (
    <WindowFrame title={t("shell.product")} platform={state.platform} bare>
      <main className="min-h-0 overflow-y-auto bg-canvas">
        {state.phase === "no_vault" ? <Welcome /> : <Unlock />}
      </main>
    </WindowFrame>
  );
}
