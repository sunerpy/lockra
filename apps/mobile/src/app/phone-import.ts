// The imports that open a screen of the phone's own, the camera and the photo picker: what they
// read stays in Rust and goes into the preview; a failure (the camera refused) becomes a toast.
// Each answers `true` when the preview has something new.
import { useBackend, useGuarded, useT } from "@lockra/ui";
import { useCallback } from "react";
import { overPhoneScreen } from "./phone-screen";

export function usePhoneImport() {
  const t = useT();
  const { backend } = useBackend();
  const guarded = useGuarded();
  const lock = useCallback(() => {
    void backend.dispatch({ command: "vault_lock" }).catch(() => undefined);
  }, [backend]);
  const scan = useCallback(
    async () =>
      (await guarded(() =>
        overPhoneScreen(
          () => backend.scanImport({ prompt: t("mobile.scan.prompt"), cancel: t("common.cancel") }),
          lock,
        ),
      )) === true,
    [backend, guarded, lock, t],
  );
  const pickImages = useCallback(
    async () =>
      (await guarded(() => overPhoneScreen(() => backend.pickImportFiles("images"), lock))) ===
      true,
    [backend, guarded, lock],
  );
  return { scan, pickImages };
}
