// The extension's pages follow the browser's light or dark scheme: 澄空 or 澄空 · 夜, the same
// tokens.css as the app (no theme goes over the protocol).
export function followScheme(doc: Document, win: Window): void {
  const media =
    typeof win.matchMedia === "function" ? win.matchMedia("(prefers-color-scheme: dark)") : null;
  const apply = () => {
    doc.documentElement.dataset.theme = media?.matches ? "glass_night" : "glass";
  };
  apply();
  media?.addEventListener("change", apply);
}
