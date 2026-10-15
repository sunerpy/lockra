/** A scheme's shadow and corners in px: the menu's (`--radius-card`) and the prompts'. */
export interface SchemeLook {
  shadow: string;
  menu: number;
  prompt: number;
}
export interface FrameLook {
  light: SchemeLook;
  dark: SchemeLook;
}
export const TOKENS: string;
export function frameLook(css: string): FrameLook;
export function readFrameLook(path?: string): FrameLook;
