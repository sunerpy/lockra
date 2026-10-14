// A login's square, as the app draws an account left on its automatic colour (@lockra/ui
// `EntryAvatar`): the same FNV-1a hash of the name over the same nine colours, so a site has the same
// colour in the extension as in Lockra. The extension's own copy keeps the app's dictionaries and
// IPC schemas out of its pages; avatar.test.ts checks the two agree.
export const AUTO_COLORS = [
  "red",
  "orange",
  "amber",
  "green",
  "teal",
  "blue",
  "indigo",
  "purple",
  "pink",
] as const;

export type AvatarColor = (typeof AUTO_COLORS)[number];

export function autoColor(name: string, fallback = ""): AvatarColor {
  const source = (name.trim() || fallback.trim()).toLocaleLowerCase();
  let hash = 0x811c9dc5;
  for (const ch of source) {
    hash ^= ch.codePointAt(0) ?? 0;
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return AUTO_COLORS[hash % AUTO_COLORS.length] ?? "blue";
}

export function initial(name: string, fallback = ""): string {
  const source = name.trim() || fallback.trim();
  return (Array.from(source)[0] ?? "?").toLocaleUpperCase();
}

/** The square's corner for its size (EntryAvatar's), and its letter's size. */
export const AVATAR_RADIUS: Readonly<Record<32 | 36, number>> = { 32: 10, 36: 10 };
export const AVATAR_TEXT: Readonly<Record<32 | 36, string>> = {
  32: "text-[14px]",
  36: "text-[16px]",
};
