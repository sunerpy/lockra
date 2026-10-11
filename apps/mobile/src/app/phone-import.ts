// The imports that open a screen of the phone's own, the camera, the photo picker and the file
// picker: what they read stays in Rust and goes into the preview; a failure (the camera refused)
// becomes a toast. Each answers `true` when the preview has something new.
import { useBackend, useGuarded, useT } from "@lockra/ui";
import { useCallback } from "react";
import { overPhoneScreen } from "./phone-screen";

export function usePhoneImport() {
  const t = useT();
  const { backend } = useBackend();
  const guarded = useGuarded();
  const scan = useCallback(
    async () =>
      (await guarded(() =>
        overPhoneScreen(() =>
          backend.scanImport({ prompt: t("mobile.scan.prompt"), cancel: t("common.cancel") }),
        ),
      )) === true,
    [backend, guarded, t],
  );
  const pickImages = useCallback(
    async () =>
      (await guarded(() => overPhoneScreen(() => backend.pickImportFiles("images")))) === true,
    [backend, guarded],
  );
  const pickFiles = useCallback(
    async () =>
      (await guarded(() => overPhoneScreen(() => backend.pickImportFiles("any")))) === true,
    [backend, guarded],
  );
  return { scan, pickImages, pickFiles };
}
