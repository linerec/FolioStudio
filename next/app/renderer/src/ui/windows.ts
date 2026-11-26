// Window > Arrange: the windows of the open document and how they sit in the document area.
//
// Folio opens one document at a time, so "the open windows" are windows on that document (New Window makes another; each has its own
// zoom and scroll, while the document, the selection and the undo steps are one). Docked windows are tabs of the document area, or,
// after an arrangement, cells of a grid; floating windows sit above the area. The state is the screen's only: nothing of it is saved
// with the project (a window layout belongs to the computer, and the document does not remember it).
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { Command } from './commands';
import { ARRANGEMENTS, type Rect, cascadeRect, fits, keepInside, layoutRects } from './winlayout';
import type { Viewport } from './docview';

/** Window > Arrange > Minimize: the application window goes to the task bar. */
export function minimizeWindow() {
  void window.folioWindow?.minimize?.();
}

export type Win = {
  id: number;
  /** In a floating window of its own (pixels in the document area), or docked. */
  float: Rect | null;
  /** The window is split in two panes: two views of the document in one window. */
  split: boolean;
};

export type WinState = {
  /** What document these windows are of: another document starts again with one window. */
  doc: string | null;
  wins: Win[];
  next: number;
  /** The window last used (commands that act on "the window" act on it), and the pane in it. */
  active: number;
  activeKey: string;
  /** The docked window that shows when docked windows are tabs. */
  shown: number;
  /** An arrangement of the docked windows (winlayout.ts), or null: tabs. */
  arrange: string | null;
};

export const paneKey = (id: number, second = false) => (second ? `${id}b` : String(id));

export const initialState = (doc: string | null): WinState => ({ doc, wins: [{ id: 1, float: null, split: false }], next: 2, active: 1, activeKey: '1', shown: 1, arrange: null });

export const docked = (s: WinState) => s.wins.filter((w) => !w.float);
export const floating = (s: WinState) => s.wins.filter((w) => !!w.float);

/** The arrangement in force: it holds only while the docked windows are the number it wants. */
export const arrangement = (s: WinState): string | null => (s.arrange && fits(s.arrange, docked(s).length) ? s.arrange : null);

/** The docked window shown when docked windows are tabs. */
export function shownId(s: WinState): number {
  const d = docked(s);
  return d.some((w) => w.id === s.shown) ? s.shown : (d[0]?.id ?? 0);
}

const keepActive = (s: WinState): WinState => {
  const w = s.wins.find((x) => x.id === s.active) ?? docked(s)[0] ?? s.wins[0];
  return w && w.id !== s.active ? { ...s, active: w.id, activeKey: paneKey(w.id) } : s;
};

export const newWindow = (s: WinState): WinState => ({
  ...s, wins: [...s.wins, { id: s.next, float: null, split: false }], next: s.next + 1, active: s.next, activeKey: paneKey(s.next), shown: s.next, arrange: null,
});

export function closeWindow(s: WinState, id: number): WinState {
  if (s.wins.length < 2) return s;
  return keepActive({ ...s, wins: s.wins.filter((w) => w.id !== id) });
}

/** A click in a window (or its tab): it is the active one, and a docked one is the one shown. */
export function activate(s: WinState, id: number, key = paneKey(id)): WinState {
  const w = s.wins.find((x) => x.id === id);
  if (!w || (s.active === id && s.activeKey === key && (w.float || s.shown === id))) return s;
  return { ...s, active: id, activeKey: key, shown: w.float ? s.shown : id };
}

/** Every window docked, in an arrangement (null: as tabs). */
export const dockAll = (s: WinState, how: string | null): WinState => ({ ...s, wins: s.wins.map((w) => (w.float ? { ...w, float: null } : w)), arrange: how, shown: s.active });

/** Every window floating, one step down and right of the last (Cascade). */
export const floatAll = (s: WinState, area: { w: number; h: number }): WinState => ({ ...s, wins: s.wins.map((w, i) => ({ ...w, float: cascadeRect(i, area) })), arrange: null });

export function floatWindow(s: WinState, id: number, area: { w: number; h: number }): WinState {
  const at = floating(s).length;
  return keepActive({ ...s, wins: s.wins.map((w) => (w.id === id && !w.float ? { ...w, float: cascadeRect(at, area) } : w)), arrange: null, shown: s.shown === id ? (docked(s).find((w) => w.id !== id)?.id ?? 0) : s.shown });
}

export const moveFloat = (s: WinState, id: number, r: Rect, area: { w: number; h: number }): WinState => ({ ...s, wins: s.wins.map((w) => (w.id === id && w.float ? { ...w, float: keepInside(r, area) } : w)) });

/** Splits a window in two panes, or joins them again (the active pane is then the window's first). */
export const toggleSplit = (s: WinState, id: number): WinState => ({
  ...s,
  wins: s.wins.map((w) => (w.id === id ? { ...w, split: !w.split } : w)),
  activeKey: s.active === id && s.wins.find((w) => w.id === id)?.split ? paneKey(id) : s.activeKey,
});

type Parts = { hasDoc: boolean; docKey: string | null };

/** The windows, their commands and the viewport that the viewport commands (zoom, fit, go to a page) act on: the active pane's. */
export function useWindows({ hasDoc, docKey }: Parts) {
  const [raw, setRaw] = useState<WinState>(() => initialState(docKey));
  const state = raw.doc === docKey ? raw : initialState(docKey);
  // no document: the windows are gone (the same document opened again starts with one window)
  useEffect(() => { if (!hasDoc) setRaw(initialState(null)); }, [hasDoc]);
  const update = useCallback((f: (s: WinState) => WinState) => setRaw((r) => f(r.doc === docKey ? r : initialState(docKey))), [docKey]);
  /** The viewport of each pane on screen, by pane key (a pane puts its own here while it draws). */
  const registry = useRef(new Map<string, Viewport>());
  const hostRef = useRef<HTMLDivElement | null>(null);
  // a pane's zoom changed: the app draws again so that what reads the active viewport (zoom commands, the zoom steps) sees it
  const [, setTick] = useState(0);
  const touch = useCallback(() => setTick((n) => n + 1), []);
  const area = useCallback(() => {
    const r = hostRef.current?.getBoundingClientRect();
    return { w: Math.round(r?.width ?? 900), h: Math.round(r?.height ?? 600) };
  }, []);

  const n = state.wins.length;
  const active = state.wins.find((w) => w.id === state.active);
  const handlers = useMemo(() => {
    const h: Record<string, Partial<Command>> = {
      'id.window.arrange.minimize': { run: minimizeWindow },
      'id.window.arrange.cascade': { run: hasDoc && n >= 2 ? () => update((s) => floatAll(s, area())) : undefined },
      'id.window.arrange.floatInWindow': { run: hasDoc && active && !active.float ? () => update((s) => floatWindow(s, s.active, area())) : undefined },
      'id.window.arrange.floatAllInWindows': { run: hasDoc && docked(state).length ? () => update((s) => floatAll(s, area())) : undefined },
      'id.window.arrange.consolidateAllWindows': { run: hasDoc && floating(state).length ? () => update((s) => dockAll(s, null)) : undefined },
      'id.window.arrange.newWindow': { run: hasDoc ? () => update(newWindow) : undefined },
      'id.window.arrange.splitWindow': { run: hasDoc ? () => update((s) => toggleSplit(s, s.active)) : undefined, checked: !!active?.split },
      // Next / Previous Window (Ctrl+Tab, Ctrl+Shift+Tab; tools-c1): this document's windows in turn; with one window the documents' app windows
      // take the keys (ui/appwindows.ts composeArrange)
      'window.key.next': { run: hasDoc && n >= 2 ? () => update((s) => { const i = s.wins.findIndex((w) => w.id === s.active); return activate(s, s.wins[(i + 1) % s.wins.length].id); }) : undefined },
      'window.key.previous': { run: hasDoc && n >= 2 ? () => update((s) => { const i = s.wins.findIndex((w) => w.id === s.active); return activate(s, s.wins[(i - 1 + s.wins.length) % s.wins.length].id); }) : undefined },
    };
    // the arrangements: the ones with a number of windows of their own are on only when that many windows are open
    for (const id of Object.keys(ARRANGEMENTS)) {
      h[`id.window.arrange.${id}`] = { run: hasDoc && fits(id, n) ? () => update((s) => dockAll(s, id)) : undefined, checked: arrangement(state) === id };
    }
    return h;
  }, [hasDoc, n, active, state, update, area]);

  const activeKey = state.wins.some((w) => w.id === state.active) ? state.activeKey : '1';
  /** The viewport the commands use: a new object each render, reading the active pane's viewport when a command runs. */
  const forward = useCallback((primary: Viewport): Viewport => {
    // the active pane's; else any pane of the active window; else any pane at all (a pane just closed), else the app's own
    const cur = () => registry.current.get(activeKey) ?? registry.current.get(String(parseInt(activeKey, 10))) ?? registry.current.values().next().value ?? primary;
    return {
      get viewRef() { return cur().viewRef; },
      get viewEl() { return cur().viewEl; },
      get zoom() { return cur().zoom; },
      get scale() { return cur().scale; },
      setView: primary.setView,
      zoomTo: (...a: Parameters<Viewport['zoomTo']>) => cur().zoomTo(...a),
      fit: (...a: Parameters<Viewport['fit']>) => cur().fit(...a),
      scrollToPage: (...a: Parameters<Viewport['scrollToPage']>) => cur().scrollToPage(...a),
      scrollToTarget: (...a: Parameters<Viewport['scrollToTarget']>) => cur().scrollToTarget(...a),
      togglePrevious: () => cur().togglePrevious(),
      setZoom: (...a: Parameters<Viewport['setZoom']>) => cur().setZoom(...a),
    };
  }, [activeKey]);

  const rects = useMemo(() => {
    const a = arrangement(state);
    return a ? layoutRects(a, docked(state).length) : null;
  }, [state]);

  return { state, update, handlers, forward, registry, hostRef, touch, area, rects, activeKey } as const;
}

export type Windows = ReturnType<typeof useWindows>;
