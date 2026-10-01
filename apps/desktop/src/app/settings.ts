// Change some settings: the core takes the whole object, so a change is the current one patched.
import type { Settings } from "@lockra/shared";
import { useUiState } from "@lockra/ui";
import { useCallback } from "react";
import { useDispatch } from "./dispatch";

export function useUpdateSettings(): (patch: Partial<Settings>) => void {
  const { settings } = useUiState();
  const dispatch = useDispatch();
  return useCallback(
    (patch: Partial<Settings>) =>
      void dispatch({ command: "settings_set", settings: { ...settings, ...patch } }),
    [settings, dispatch],
  );
}
