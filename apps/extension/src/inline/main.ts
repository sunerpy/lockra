// inline.html: the frame the content script shows in the page, with its token in the fragment.
import "./inline.css";
import { extensionLocale, extensionTranslator } from "@lockra/shared/i18n/extension";
import { browserApi } from "../browser/api";
import { followScheme } from "../shared/scheme";
import { InlinePage } from "./page";
import { inlinePort } from "./port";

const api = browserApi();
followScheme(document, window);
const locale = extensionLocale(api.i18n.getUILanguage());
document.documentElement.lang = locale;
new InlinePage({
  doc: document,
  token: location.hash.slice(1),
  t: extensionTranslator(locale),
  connect: () => inlinePort(api),
  IntersectionObserver: globalThis.IntersectionObserver,
  ResizeObserver: globalThis.ResizeObserver,
}).start();
