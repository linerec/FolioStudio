// Panel windows (uix-d3). A panel group taken out to an OS window of its own (dock.tsx PopoutWindow) is an empty page of this same
// renderer that the app window draws into, so the app window's code gets that window's pointer and key events itself. What a drag
// across the windows and the windows' shortcuts need is here: which window a point of the desktop is over and where in its page, and
// the keys a panel window passes on to the app window. The places of the windows come from the Electron main process (it knows them
// exactly, in the desktop's own pixels); it only tells where the windows are: what a drop does stays in the page (docklayout.ts).
import { typing } from './commands';

/** A window's page on the desktop: its content rectangle (desktop pixels) and the interface zoom (Preferences ▸ Interface size). */
export type PagePlace = { x: number; y: number; w: number; h: number; zoom: number };
export type WindowPlaces = { main: PagePlace; popouts: (PagePlace & { name: string })[] };
export type Point = { x: number; y: number };

/** A panel window's name: the group's first panel when it opened. */
export const windowName = (lead: string) => `folio-panel-${lead}`;
/** An open panel window and the first panel of the group it shows now (that changes while the window stays: tabs reordered, the first
 *  one dragged away, another window's tabs joining). */
export type PanelWindow = { win: Window; lead: string };
/** The open panel windows, by name (PopoutWindow adds its window while it is open). */
export const panelWindows = new Map<string, PanelWindow>();
/** A name for a new panel window: its first panel's, numbered when a window that showed that panel first is still open under it. */
export function freeName(lead: string): string {
  let name = windowName(lead);
  for (let i = 2; panelWindows.has(name); i++) name = `${windowName(lead)}-${i}`;
  return name;
}
/** The panel window a page belongs to (its name), or undefined. */
export const nameOf = (win: Window) => [...panelWindows].find(([, p]) => p.win === win)?.[0];
/** The floating group a panel window shows (by the window's name), or -1. */
export function groupOfWindow(name: string, floating: readonly { panels: readonly string[]; window?: unknown }[]): number {
  const lead = panelWindows.get(name)?.lead;
  return lead === undefined ? -1 : floating.findIndex((f) => !!f.window && f.panels[0] === lead);
}

/** Where the windows are now (null: one window only on this device, or no answer). */
export async function windowPlaces(): Promise<WindowPlaces | null> {
  try { return (await window.folio.windowPlaces?.()) ?? null; } catch { return null; }
}

/** A point of the desktop in a page (CSS pixels from its top left), or null when it is outside that page. */
export function inPage(p: Point, b: PagePlace): Point | null {
  const z = b.zoom > 0 ? b.zoom : 1;
  const x = (p.x - b.x) / z, y = (p.y - b.y) / z;
  return x >= 0 && y >= 0 && x < b.w / z && y < b.h / z ? { x, y } : null;
}

/** A point of a page on the desktop (the reverse of inPage). */
export const onDesktop = (p: Point, b: PagePlace): Point => ({ x: b.x + p.x * (b.zoom > 0 ? b.zoom : 1), y: b.y + p.y * (b.zoom > 0 ? b.zoom : 1) });

/**
 * The window a desktop point is over: a panel window (by name) or the app window (`name` null), and the point in its page. The panel
 * windows come first (they stay above their app window). Null: over none of them (the desktop, another program).
 */
export function windowAt(p: Point, places: WindowPlaces): { name: string | null; at: Point } | null {
  for (const w of [...places.popouts].reverse()) { const at = inPage(p, w); if (at) return { name: w.name, at }; }
  const at = inPage(p, places.main);
  return at ? { name: null, at } : null;
}

/**
 * The keys pressed in a panel window reach the app window's shortcuts (the same key map: tools, zoom, undo, save, Tab ...): a panel
 * window has no menu of its own, so no menu key reaches it either (Windows, Linux). A key typed into a field, or one the panel used
 * itself, stays there. Returns the function that stops it.
 */
export function forwardKeys(child: Window, to: Window = window): () => void {
  const pass = (e: KeyboardEvent) => {
    if (e.defaultPrevented || typing(e)) return;
    const copy = new KeyboardEvent(e.type, {
      key: e.key, code: e.code, location: e.location, repeat: e.repeat,
      ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey, metaKey: e.metaKey, bubbles: true, cancelable: true,
    });
    to.dispatchEvent(copy);
    if (copy.defaultPrevented) e.preventDefault();
  };
  child.addEventListener('keydown', pass);
  child.addEventListener('keyup', pass);
  return () => { child.removeEventListener('keydown', pass); child.removeEventListener('keyup', pass); };
}
