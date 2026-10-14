import "../pages.css";
import { extensionLocale, extensionTranslator } from "@lockra/shared/i18n/extension";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { browserApi } from "../browser/api";
import { PORT_POPUP } from "../shared/messages";
import { followScheme } from "../shared/scheme";
import { TextProvider } from "../shared/text";
import { browserClient } from "./client";
import { Popup } from "./Popup";

const api = browserApi();
followScheme(document, window);
const locale = extensionLocale(api.i18n.getUILanguage());
document.documentElement.lang = locale;
const container = document.getElementById("root");
if (container) {
  createRoot(container).render(
    <StrictMode>
      <TextProvider t={extensionTranslator(locale)}>
        <Popup client={browserClient(api, PORT_POPUP, window)} />
      </TextProvider>
    </StrictMode>,
  );
}
