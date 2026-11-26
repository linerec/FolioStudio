



import { userSettings } from '../user-settings';

export type Status = 'idle' | 'opening' | 'ready' | 'error';
export type Theme = 'dark' | 'mediumDark' | 'mediumLight' | 'light';

export const THEME_CHROME: Record<Theme, [string, string]> = {
  // the window buttons drawn by the system over the title row: ui/tokens.css --chrome and --chrome-symbol of each theme
  dark: ['#1b1c20', '#dfe1e6'],
  mediumDark: ['#3b3c41', '#f2f3f5'],
  mediumLight: ['#afb0b5', '#17181b'],
  light: ['#e4e5e9', '#1f2024'],
};
export const NO_EDITS = { undo: null, redo: null, dirty: false, history: { undo: [] as string[], redo: [] as string[] } };

export function stored<T extends string>(key: string, fallback: T): T {
  try { return (userSettings.getItem(key) as T) || fallback; } catch { return fallback; }
}
export function keep(key: string, value: string) {
  try { userSettings.setItem(key, value); } catch { /* not kept */ }
}
