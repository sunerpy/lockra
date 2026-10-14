// The extension's strings in its React pages: the `extension` namespace of @lockra/shared's i18n,
// loaded on its own (the app's whole dictionary and its IPC schemas stay out of the popup).
import { type ExtensionT, extensionTranslator } from "@lockra/shared/i18n/extension";
import { type ReactNode, createContext, useContext } from "react";

const TextContext = createContext<ExtensionT>(extensionTranslator("zh-CN"));

export function TextProvider({ t, children }: { t: ExtensionT; children: ReactNode }) {
  return <TextContext.Provider value={t}>{children}</TextContext.Provider>;
}

export function useText(): ExtensionT {
  return useContext(TextContext);
}
