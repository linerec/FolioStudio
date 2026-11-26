// Which menu command uses a key combination (InDesign 2025, measured: oracle/rules/stylekeys-c2.ko.md). The Style Options "Shortcut" field names it
// under "Currently assigned to" when no style holds the combination, and a style shortcut never beats a menu command when the key is pressed.
// Combinations are compared in the engine's spelling of a style shortcut (`ctrl+shift+alt+key`), not by the menus' own matcher: a keypad digit
// is not a main-row digit (Ctrl+Num 1 is free while Ctrl+1 is Actual Size); the zoom commands keep InDesign's keypad twins (Ctrl+Num + and
// Ctrl+Num - are Zoom In and Zoom Out). The name is the menu path under the menu bar with the item last, as InDesign writes it
// ("Grids & Guides: Show Guides", "Insert Special Character: Hyphens and Dashes: Nonbreaking Hyphen"); a panel menu item names its panel.
import { MAIN_MENUS, PANEL_MENUS } from './menutree';
import type { MenuEntry } from './menudef';
import type { Commands } from './commands';
import { shortcutSpecFromMenuKey } from './styleshortcuts';

let source: () => Commands = () => ({});
/** App tells where the live command table is (the keys the user changed in Preferences included). */
export function setCommandSource(fn: () => Commands) { source = fn; }

/** Keypad twins InDesign gives to commands of the main keyboard (measured: Ctrl+Num + / Ctrl+Num - only). */
const KEYPAD_TWINS: Record<string, string> = { 'ctrl+=': 'ctrl+numadd', 'ctrl+-': 'ctrl+numsub' };

/** The menu path (submenu names) of every command that sits in a menu; the deepest place when it sits in several (Type ▸ Character and Window ▸ Type & Tables ▸ Character). */
function menuPaths(commands: Commands): Map<string, string[]> {
  const out = new Map<string, string[]>();
  const walk = (entries: MenuEntry[], path: string[], into: Map<string, string[]>) => {
    for (const e of entries) {
      if ('cmd' in e) { const old = into.get(e.cmd); if (!old || path.length > old.length) into.set(e.cmd, path); }
      else if ('sub' in e) walk(e.sub, [...path, e.label], into);
    }
  };
  for (const m of MAIN_MENUS) walk(m.items, [], out);
  for (const [panel, items] of Object.entries(PANEL_MENUS)) {
    const title = commands[`panel.${panel}`]?.label;
    if (!title) continue;
    const own = new Map<string, string[]>();
    walk(items, [title], own);
    for (const [id, path] of own) if (!out.has(id)) out.set(id, path);
  }
  return out;
}

export type CommandOwner = { id: string; name: string };

/** The menu command that has this style-shortcut spelling (`ctrl+alt+z`), the first in the command table; null when none has it. */
export function commandOwner(spec: string, commands: Commands = source()): CommandOwner | null {
  let paths: Map<string, string[]> | null = null;
  for (const [id, c] of Object.entries(commands)) {
    if (!c.keys || !c.label) continue;
    for (const key of c.keys) {
      const own = shortcutSpecFromMenuKey(key);
      if (own === null || (own !== spec && KEYPAD_TWINS[own] !== spec)) continue;
      paths ??= menuPaths(commands);
      return { id, name: [...(paths.get(id) ?? []), c.label].join(': ') };
    }
  }
  return null;
}
