import "../pages.css";
import { extensionLocale, extensionTranslator } from "@lockra/shared/i18n/extension";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { browserApi } from "../browser/api";
import { PORT_OPTIONS } from "../shared/messages";
import { followScheme } from "../shared/scheme";
import { TextProvider } from "../shared/text";
import { browserClient } from "../popup/client";
import { Options } from "./Options";

const api = browserApi();
followScheme(document, window);
const locale = extensionLocale(api.i18n.getUILanguage());
document.documentElement.lang = locale;
const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <TextProvider t={extensionTranslator(locale)}>
        <Options client={browserClient(api, PORT_OPTIONS, window)} locale={locale} />
      </TextProvider>
    </StrictMode>,
  );
}
