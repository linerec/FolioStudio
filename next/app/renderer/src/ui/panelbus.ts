// The commands of a panel's menu (Hyperlinks, Glyphs, Index, Conditional Text, Data Merge, Trap Presets, Tabs ...) whose work needs the panel's
// own state (the topic, condition, preset or tab picked in it). The panel offers what its menu items do while it is open (`usePanelCommands`),
// and App turns every id a panel declared (`declarePanelCommands`) into a command: on while the panel offers it, off otherwise. The ids are the
// menu items' command ids (menutree.ts); the work is the panel's own function, the same one its buttons call (menuaudit-c, AUDIT.ko.md).
import { useEffect, useSyncExternalStore } from 'react';

export type PanelOffer = { run: () => void; checked?: boolean };
type Offered = Record<string, PanelOffer | (() => void) | undefined>;

const declared = new Set<string>();
const offered = new Map<string, Map<string, PanelOffer>>();
const listeners = new Set<() => void>();
let version = 0;

/** The menu item ids a panel gives handlers to. Called when the panel's module loads, so App knows them before the panel is open. */
export function declarePanelCommands(ids: readonly string[]): void {
  for (const id of ids) declared.add(id);
}

export const declaredPanelCommands = (): string[] => [...declared];

const signature = (m: Map<string, PanelOffer>) => [...m].map(([id, o]) => `${id}${o.checked ? '+' : ''}`).sort().join('|');

function setOffered(panel: string, next: Map<string, PanelOffer>): void {
  const before = offered.get(panel);
  const changed = (before ? signature(before) : '') !== signature(next);
  if (next.size) offered.set(panel, next); else offered.delete(panel);
  if (!changed) return;
  version++;
  listeners.forEach((fn) => fn());
}

/** What the panel's menu items do right now: an id left out or undefined is off. Called after every render (the functions are fresh then). */
export function offerPanelCommands(panel: string, offer: Offered): void {
  const next = new Map<string, PanelOffer>();
  for (const [id, o] of Object.entries(offer)) if (o) next.set(id, typeof o === 'function' ? { run: o } : o);
  setOffered(panel, next);
}

export const withdrawPanelCommands = (panel: string): void => setOffered(panel, new Map());

/** What the open panels offer for the menu item `id`. */
export function panelCommand(id: string): PanelOffer | undefined {
  for (const m of offered.values()) {
    const o = m.get(id);
    if (o) return o;
  }
  return undefined;
}

const subscribe = (fn: () => void) => { listeners.add(fn); return () => { listeners.delete(fn); }; };

/** Changes whenever a panel opens, closes, or turns an item on or off (App builds its commands again then). */
export const usePanelBusVersion = (): number => useSyncExternalStore(subscribe, () => version);

/** For a panel component: offer the menu items' work while it is shown. */
export function usePanelCommands(panel: string, offer: Offered): void {
  useEffect(() => { offerPanelCommands(panel, offer); });
  useEffect(() => () => withdrawPanelCommands(panel), [panel]);
}
