// The few icons the inline frame draws without React: the paths of @lockra/ui's `Icon` (24 grid,
// 1.75 stroke, round caps), checked against it by avatar.test.tsx.
export const ICONS = {
  x: "M6 6l12 12M18 6 6 18",
  refresh: "M20 12a8 8 0 1 1-2.3-5.7M20 4v5h-5",
  copy: "M9 9h10v10H9zM5 15V5h10",
  check: "M5 12l5 5L20 7",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM12 11v5M12 8h.01",
  alert: "M12 3 2 21h20zM12 10v5M12 18h.01",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  eyeOff:
    "M3 3l18 18M10.6 5.1A10.6 10.6 0 0 1 12 5c6 0 10 7 10 7a17.6 17.6 0 0 1-3.2 3.9M6.6 6.6C3.8 8.4 2 12 2 12s4 7 10 7a9.7 9.7 0 0 0 5.4-1.6M9.9 9.9a3 3 0 0 0 4.2 4.2",
  dice: "M5 3h14a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 8h.01M16 8h.01M12 12h.01M8 16h.01M16 16h.01",
  lock: "M6 11h12v9H6zM9 11V7a3 3 0 0 1 6 0v4",
  search: "M11 4a7 7 0 1 0 0 14 7 7 0 0 0 0-14zM20 20l-4-4",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6",
  key: "M15 3a6 6 0 1 0 0 12 6 6 0 0 0 0-12zM9.5 14.5 3 21M6 18l2 2M4 20l2 2",
} as const;

export type IconName = keyof typeof ICONS;

const SVG = "http://www.w3.org/2000/svg";

/** An icon as SVG, its colour `currentColor`. */
export function icon(doc: Document, name: IconName, size: number, className = ""): SVGSVGElement {
  const svg = doc.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.75");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  if (className) svg.setAttribute("class", className);
  const path = doc.createElementNS(SVG, "path");
  path.setAttribute("d", ICONS[name]);
  svg.append(path);
  return svg;
}

/** The in-app logo (@lockra/ui `Logo`): the padlock on the theme's card, its colours the theme's. */
export function logo(doc: Document, size: number): HTMLSpanElement {
  const box = doc.createElement("span");
  box.setAttribute("aria-hidden", "true");
  box.className = "inline-flex shrink-0 overflow-hidden rounded-[22%] bg-surface hairline panel";
  box.style.width = `${size}px`;
  box.style.height = `${size}px`;
  const svg = doc.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 100 100");
  svg.setAttribute("width", "100%");
  svg.setAttribute("height", "100%");
  const shackle = doc.createElementNS(SVG, "path");
  shackle.setAttribute("d", "M37 45 V36 A13 13 0 0 1 63 36 V45");
  shackle.setAttribute("fill", "none");
  shackle.setAttribute("stroke-width", "7");
  shackle.setAttribute("stroke-linecap", "round");
  shackle.setAttribute("class", "stroke-fg");
  const body = doc.createElementNS(SVG, "rect");
  for (const [k, v] of Object.entries({ x: "26", y: "43", width: "48", height: "37", rx: "9" }))
    body.setAttribute(k, v);
  body.setAttribute("class", "fill-[var(--logo-body)]");
  svg.append(shackle, body);
  for (const cx of [38, 50, 62]) {
    const dot = doc.createElementNS(SVG, "circle");
    dot.setAttribute("cx", String(cx));
    dot.setAttribute("cy", "61.5");
    dot.setAttribute("r", "4.2");
    dot.setAttribute("class", "fill-[var(--logo-dots)]");
    svg.append(dot);
  }
  box.append(svg);
  return box;
}
