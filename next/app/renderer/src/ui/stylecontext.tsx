// The right-click menu of a row in a style panel: InDesign's Text Style List, Object Styles Item, Tables Style and Cell Styles context menus
// (generated into menutree.ts CONTEXT_MENUS under styleParagraph, styleObject, styleTable, styleCell). The panel selects the row that was
// clicked and says what each command of the menu does for it (`actions`, by command id); a command the panel gives no action is off.
// The menu items that are also panel menu items share their command id (gen-menutree.py ALIAS), so the same handler answers both.
import { type MouseEvent, useState } from 'react';
import { CONTEXT_MENUS } from './menutree';
import type { MenuEntry } from './menudef';
import type { Commands } from './commands';
import { Dropdown } from './menu';

export type RowAction = { disabled?: boolean; checked?: boolean; label?: string; run: () => unknown };
export type StyleMenuKey = 'styleParagraph' | 'styleObject' | 'styleTable' | 'styleCell';

const idsOf = (entries: MenuEntry[]): string[] => entries.flatMap((e) => ('cmd' in e ? [e.cmd] : 'sub' in e ? idsOf(e.sub) : []));

/** The menu's commands for the row: the panel's action for each id, InDesign's ^1 in a label replaced by the style's name. */
export function rowCommands(key: StyleMenuKey, base: Commands, actions: Record<string, RowAction | undefined>, name: string): Commands {
  const out: Commands = { ...base };
  for (const id of idsOf(CONTEXT_MENUS[key] ?? [])) {
    const a = actions[id];
    const c = base[id];
    out[id] = {
      ...c,
      label: (a?.label ?? c?.label ?? id).replace('^1', name),
      enabled: !!a && !a.disabled,
      checked: a?.checked,
      run: a ? () => { void a.run(); } : undefined,
    };
  }
  return out;
}

/** Where the right-click menu is open (none when shut). */
export function useRowMenu() {
  const [at, setAt] = useState<{ x: number; y: number } | null>(null);
  return { at, open: (e: MouseEvent) => { e.preventDefault(); e.stopPropagation(); setAt({ x: e.clientX, y: e.clientY }); }, close: () => setAt(null) };
}

export function StyleRowMenu({ menu, at, commands, actions, name, close }: {
  menu: StyleMenuKey; at: { x: number; y: number } | null; commands?: () => Commands; actions: Record<string, RowAction | undefined>; name: string; close: () => void;
}) {
  if (!at || !commands) return null;
  return <Dropdown items={CONTEXT_MENUS[menu] ?? []} commands={rowCommands(menu, commands(), actions, name)} x={at.x} y={at.y} onClose={close} />;
}
