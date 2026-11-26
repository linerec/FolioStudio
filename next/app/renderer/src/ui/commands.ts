// Commands: one id per action, shared by the menus, the keyboard and the panels. Shortcuts are
// written the Windows way ("Ctrl+Shift+D"); on a Mac Ctrl means Command and Alt means Option,
// as in InDesign.

export type Command = {
  label: string;
  /** Shortcuts; the first is shown in menus. */
  keys?: string[];
  run?: () => void;
  /** Default true when `run` is set. */
  enabled?: boolean;
  checked?: boolean;
  /** Why it is off (shown as a tooltip and in the status line). */
  why?: string;
  /** Entries replacing a list placeholder wherever it appears in a menu. */
  items?: { cmd: string }[];
  /** Without `run` the key is not this command's: it goes on (the same key means a text format with a text selection and something else for objects, ui/textkeys.ts). */
  fallThrough?: boolean;
  /** Keep contextual key actions available in the shortcut editor while inactive. */
  shortcutVisible?: boolean;
};

export type Commands = Record<string, Command>;

export { isMac, parseKey, matches, keyLabel } from './keys';
import { commandKey, matches } from './keys';

/** The command a key press runs, if any (first match in `order`). */
export function commandFor(e: KeyboardEvent, commands: Commands): string | null {
  for (const [id, c] of Object.entries(commands)) {
    if (!c.keys || (c.fallThrough && !c.run)) continue;
    for (const k of c.keys) if (matches(e, k)) return id;
  }
  return null;
}

export const isEnabled = (c: Command | undefined) => !!c && (c.enabled ?? !!c.run);

/** Whether focus is in a text field (single-key shortcuts must not fire). */
export function typing(e: KeyboardEvent | Event) {
  const t = e.target as HTMLElement | null;
  if (!t) return false;
  // the story editor passes on the Ctrl/⌘ and Alt chords it does not use; they are commands
  if (t.dataset?.textEditor === 'true' && 'ctrlKey' in e && (commandKey(e as KeyboardEvent) || (e as KeyboardEvent).altKey)) return false;
  // and the function keys (Find/Change's Shift+F1 to Shift+F3, the context menu's Shift+F10: InDesign's text context keys; tools-c1)
  if (t.dataset?.textEditor === 'true' && 'code' in e && /^F([1-9]|1[0-2])$/.test((e as KeyboardEvent).code)) return false;
  const tag = t.tagName;
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t.isContentEditable;
}
