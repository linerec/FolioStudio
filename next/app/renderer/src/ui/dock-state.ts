//! Owns panel layout state, persistence, undo and commands triggered by drags.

import { useCallback, useEffect, useRef, useState, type SetStateAction } from 'react';
import { tr } from '../i18n';
import { dropKey, isAreaZone, placeGhost, rectOf, targetAt, view, type Rect } from './dock-drop-target';
import { PANEL_META, VIEWS, type PanelId, type ViewId } from './dock-panels';
import {
  CLASSIC as CLASSIC_LAYOUT, type Area, type DockCommand, type DockEnv, type DockResult, type DockState as Layout, type DropTarget,
  type History, type OsWindow, type Zone, applyDock, describeDock, emptyHistory, historyRecord, historyRedo,
  historyUndo, keepWindows, normalizeDock, snapRect, whereIs, zoneGroups,
} from './docklayout';
import { type Point, type WindowPlaces, groupOfWindow, nameOf, onDesktop, panelWindows, windowAt, windowPlaces } from './dockwindows';
import { usePanelState } from './panel-state';

/** The layout the sidebar (uix-c1) had: its width, the explorer's share, whether it and its two tiers were open. */
export type SidebarSeed = { open?: boolean; width?: number; split?: number; explorerOpen?: boolean; toolsOpen?: boolean };


/** The layout of the dock (ui/docklayout.ts has the logic and the commands; this file draws it and turns drags into those commands). */
export type DockState = Layout<PanelId>;
export const CLASSIC = CLASSIC_LAYOUT as DockState;

const dockEnv: DockEnv = {
  known: (id) => Object.prototype.hasOwnProperty.call(PANEL_META, id),
  size: (id) => ({ w: PANEL_META[id as PanelId].width, h: PANEL_META[id as PanelId].height }),
};

/**
 * A layout without the Folio views (made before uix-d2, when the explorer and the tools were a sidebar of their own) gets them back
 * at their home: a column of the left area, the explorer above the tools, as wide and split as the sidebar was (`seed`, the engine's
 * `workspaceLayout.sidebar`). A layout that has one of them is left as it is (a person moved or closed the other).
 */
export function withViews(s: DockState, seed: SidebarSeed = {}): DockState {
  if (VIEWS.some((id) => whereIs(s, id))) return s;
  const split = typeof seed.split === 'number' && seed.split >= 0.15 && seed.split <= 0.85 ? seed.split : 0.64;
  const fold = { ...(seed.explorerOpen === false ? { explorer: true } : {}), ...(seed.toolsOpen === false ? { toolbox: true } : {}) };
  if (!zoneGroups(s, 'left').length) {
    return normalizeDock({
      ...s,
      left: [['explorer'], ['toolbox']],
      layout: { ...s.layout, left: { dir: 'col', parts: [0, 1], sizes: [split, 1 - split] } },
      size: { ...s.size, ...(typeof seed.width === 'number' ? { left: seed.width } : {}) },
      collapsed: { ...s.collapsed, ...(seed.open === false ? { left: true } : {}) },
      folded: { ...s.folded, ...fold },
      active: { ...s.active, explorer: 'explorer', toolbox: 'toolbox' },
    }, dockEnv) as DockState;
  }
  // the left area holds panels already: the two views come in as a column of their own at its left edge
  let next: DockState = s;
  const step = (cmd: DockCommand) => { const r = applyDock(next, cmd, dockEnv); if (r.ok) next = r.state as DockState; };
  step({ op: 'movePanel', panel: 'explorer', to: { zone: 'left', edge: 'left' } });
  const at = whereIs(next, 'explorer');
  if (at) step({ op: 'movePanel', panel: 'toolbox', to: { zone: 'left', group: at.group, edge: 'bottom' } });
  return next;
}

/** The layout of an older version of the app kept in the window's own storage: read once, then the settings hold it. */
const LEGACY_STORE = 'folio.workspace.classic.v1';
function legacy(): unknown {
  try { return JSON.parse(localStorage.getItem(LEGACY_STORE) ?? 'null'); } catch { return null; }
}

/** What a drag carries: one panel, or all the panels of a group. */
type Source = { kind: 'panel'; id: PanelId } | { kind: 'group'; zone: 'strip' | Area | 'floating'; group: number };
/** A panel drag in progress: what it carries, where it would land (`key`, and the part of the screen it would take: `preview`), and the
 *  pointer (`at`, kept current between renders). A floating panel moved by its title has no ghost (`ghost: false`): it moves itself.
 *  `win`: the panel window (by name) the pointer is over, whose page shows the preview and the ghost (null: the app window; uix-d3). */
type Drag = { label: string; at: { x: number; y: number }; key: string | null; preview: Rect | null; ghost: boolean; win?: string | null };

/** A step of the layout's undo: the layout, and what its owner keeps with it (ui/workspaces.tsx: the workspace in use ...). */
type UndoEntry = { dock: DockState; with?: unknown };
/** Changes that only follow the system's windows (a panel window moved by its title bar, the app window made smaller): no undo step. */
const FOLLOWS_WINDOWS = new Set<DockCommand['op']>(['moveWindow', 'fitFloating']);

/** Whether this screen can take a panel group out to an OS window of its own (the desktop app; a phone has one window only). */
export const canPopOut = () => typeof window.folio?.displays === 'function';

/** Where a group's own OS window first goes: over the group's place now, a little down and right (desktop pixels). */
function popOutRect(s: DockState, zone: Zone, group: number) {
  const panels = (zoneGroups(s, zone)[group] ?? []) as PanelId[];
  const el = document.querySelector(zone === 'floating' ? `.floating[data-float="${group}"]` : `.dpanel[data-zone="${zone}"][data-group="${group}"]`);
  const r = el?.getBoundingClientRect();
  const meta = panels[0] ? PANEL_META[panels[0]] : { width: 280, height: 360 };
  return {
    x: Math.round(window.screenX + (r?.left ?? 200) + 24), y: Math.round(window.screenY + (r?.top ?? 120) + 24),
    w: Math.round(Math.max(240, r && r.width > 0 ? r.width : meta.width)), h: Math.round(Math.max(200, r && r.height > 0 ? r.height : meta.height) + 32),
  };
}
/** A DockCommand's result as the screen and tests see it: structured, with the layout as one row per panel. */
export type DockReply = { ok: true; changed: boolean; layout: ReturnType<typeof describeDock> } | { ok: false; error: string; message: string };

export type HiddenPanels = ReadonlySet<PanelId>;

/** Dock layout state and the actions the Window menu needs. */
export function useDock() {
  // The layout lives in `stateRef` and every change goes through `setState` below, which reads it there: a command right after
  // another (a tool, a test, a quick hand) always builds on the newest layout. Before (uix-b2 saw it, uix-d1 fixed it), the render
  // copied React's state into the ref, and Window menu opens changed React's state without the ref, so a command sent right after
  // one of them built on an older layout and the panels jumped back.
  const stateRef = useRef<DockState>(CLASSIC);
  const [state, setRendered] = useState<DockState>(CLASSIC);
  const setState = useCallback((v: SetStateAction<DockState>) => {
    const next = typeof v === 'function' ? (v as (s: DockState) => DockState)(stateRef.current) : v;
    stateRef.current = next;
    setRendered(next);
  }, []);
  const [flyout, setFlyoutState] = useState<{ group: number; active: PanelId } | null>(null);
  const flyoutRef = useRef(flyout);
  const setFlyout = useCallback((v: SetStateAction<{ group: number; active: PanelId } | null>) => {
    const next = typeof v === 'function' ? v(flyoutRef.current) : v;
    flyoutRef.current = next;
    setFlyoutState(next);
  }, []);
  const [hidden, setHidden] = useState(false);
  const [drag, setDrag] = useState<Drag | null>(null);
  const justDragged = useRef(false);

  usePanelState({ state, setState, stateRef, flyout, setFlyout, hidden, setHidden }, dockEnv, legacy,
    (d, seed) => withViews(d, seed && typeof seed === 'object' ? seed as SidebarSeed : {}));

  const where = useCallback((id: PanelId) => {
    const w = whereIs(state, id);
    if (!w) return null;
    return { place: w.zone, group: w.group };
  }, [state]);

  const openIn = (s: DockState, fl: typeof flyout, id: PanelId, off: boolean) => {
    const w = whereIs(s, id);
    if (!w || off) return false;
    if (w.zone === 'strip') return fl?.group === w.group && fl.active === id;
    if (w.zone === 'floating') return s.floating[w.group].active === id;
    const g = zoneGroups(s, w.zone)[w.group];
    if (w.zone === 'column' ? s.columnCollapsed : s.collapsed?.[w.zone]) return false;
    if (s.maximized && !g.includes(s.maximized) && zoneGroups(s, w.zone).some((x) => x.includes(s.maximized as PanelId))) return false;
    return (s.active[g[0]] ?? g[0]) === id;
  };

  const isOpen = useCallback((id: PanelId) => openIn(state, flyout, id, hidden), [flyout, state, hidden]);

  /** The panels the workspace in use leaves out (AppLayout sets it as it draws): an area's width follows only the columns it shows. */
  const leftOut = useRef<HiddenPanels | undefined>(undefined);

  // The layout's own undo (uix-d3; DESIGN.md § 16): apart from the document's (Ctrl+Z never moves a panel back while a person edits),
  // for this session, 50 steps. An entry is a layout and what goes with it (`companion`: ui/workspaces.tsx gives the workspace in use,
  // its tools and control panel and its arrangement, so a workspace change is one step too).
  const history = useRef<History<UndoEntry>>(emptyHistory());
  const [steps, setSteps] = useState({ undo: 0, redo: 0 });
  const companion = useRef<{ capture: () => unknown; restore: (v: unknown) => void } | null>(null);
  const inStep = useRef(0);
  const counted = () => setSteps({ undo: history.current.past.length, redo: history.current.future.length });
  const snapshot = (): UndoEntry => ({ dock: stateRef.current, with: companion.current?.capture() });
  /** The layout in use can be undone to (a change is about to be made). */
  const record = useCallback(() => { if (inStep.current) return; history.current = historyRecord(history.current, snapshot()); counted(); }, []);  // eslint-disable-line react-hooks/exhaustive-deps

  /** Runs a dock command (ui/docklayout.ts): the one way the layout changes, for a drag, a menu or a tool. Never throws. A change is a
   *  step of the layout's undo, but for the ones that only follow the system's windows (`record: false`; moveWindow, fitFloating). */
  const apply = useCallback((cmd: DockCommand, how: { record?: boolean } = {}): DockReply => {
    const r: DockResult = applyDock(stateRef.current, cmd, { ...dockEnv, hidden: (id) => !!leftOut.current?.has(id as PanelId) });
    if (!r.ok) return { ok: false, error: r.error, message: r.message };
    if (r.changed) {
      if (how.record ?? !FOLLOWS_WINDOWS.has(cmd.op)) record();
      setState(r.state as DockState);
      setFlyout(null);
    }
    return { ok: true, changed: r.changed, layout: describeDock(r.state) };
  }, [setState, setFlyout, record]);

  /** Several changes as one step of the layout's undo (a workspace chosen: its panels, tools, control panel and arrangement). */
  const step = useCallback((fn: () => void) => {
    record();
    inStep.current++;
    try { fn(); } finally { inStep.current--; }
  }, [record]);

  const bringBack = useCallback((e: UndoEntry) => {
    setState(keepWindows(e.dock, stateRef.current) as DockState);
    setFlyout(null);
    if (e.with !== undefined) companion.current?.restore(e.with);
  }, [setState, setFlyout]);
  /** Window ▸ Workspace ▸ Undo Layout Change (Alt+Shift+Z), and the engine's dockAction `undo`. */
  const undo = useCallback((): DockReply => {
    const u = historyUndo(history.current, snapshot());
    if (!u) return { ok: false, error: 'nothingToUndo', message: 'no layout change to undo' };
    history.current = u.history;
    bringBack(u.to);
    counted();
    return { ok: true, changed: true, layout: describeDock(stateRef.current) };
  }, [bringBack]);  // eslint-disable-line react-hooks/exhaustive-deps
  /** Window ▸ Workspace ▸ Redo Layout Change (Alt+Shift+X), and dockAction `redo`. */
  const redo = useCallback((): DockReply => {
    const u = historyRedo(history.current, snapshot());
    if (!u) return { ok: false, error: 'nothingToRedo', message: 'no undone layout change to redo' };
    history.current = u.history;
    bringBack(u.to);
    counted();
    return { ok: true, changed: true, layout: describeDock(stateRef.current) };
  }, [bringBack]);  // eslint-disable-line react-hooks/exhaustive-deps

  /** The layout now, one row per panel: where it is (zone, group, tab) and whether its tab shows. */
  const layout = useCallback(() => describeDock(stateRef.current), []);

  /** Actions on more than the dock that a tool may send with `dockAction` (the workspaces: saveWorkspace, loadWorkspace,
   *  resetWorkspace); their owner (ui/workspaces.tsx) answers them here, or null for an op that is not its. */
  const extra = useRef<((action: Record<string, unknown>) => DockReply | null) | null>(null);
  /** One action of the engine's `dockAction` (a tool such as the MCP server): a dock command, `describe`, or a workspace action. */
  const act = useCallback((action: unknown): DockReply & { dock?: DockState } => {
    if (!action || typeof action !== 'object') return { ok: false, error: 'invalid', message: 'action must be an object' };
    let a = action as Record<string, unknown>;
    if (a.op === 'describe') return { ok: true, changed: false, layout: describeDock(stateRef.current), dock: stateRef.current };
    if (a.op === 'undo' || a.op === 'redo') { const r = a.op === 'undo' ? undo() : redo(); return r.ok ? { ...r, dock: stateRef.current } : r; }
    if (a.op === 'popOut') {
      if (!canPopOut()) return { ok: false, error: 'unsupported', message: 'this device has one window only: a panel group cannot leave it' };
      if (!a.window) a = { ...a, window: popOutRect(stateRef.current, a.zone as Zone, Number(a.group)) };
    }
    const other = extra.current?.(a);
    const r = other ?? apply(a as DockCommand);
    return r.ok ? { ...r, layout: describeDock(stateRef.current), dock: stateRef.current } : r;
  }, [apply, undo, redo]);
  // the engine hands a tool's dock action to this window (server dock_action.rs) and passes the answer on to the tool
  useEffect(() => window.folio.onEvent((e) => {
    if (e.event !== 'dock-action') return;
    const d = e as unknown as { request: unknown; action: unknown };
    let result: ReturnType<typeof act>;
    try { result = act(d.action); } catch (err) { result = { ok: false, error: 'invalid', message: String(err) }; }
    void window.folio.request({ cmd: 'dockActionDone', request: d.request, result }).catch(() => {});
  }), [act]);

  /** Shows a panel (Window menu, shortcut); `toggle` hides it when it is already showing. */
  const show = useCallback(
    (id: PanelId, toggle = true) => {
      const s = stateRef.current;
      const w = whereIs(s, id);
      if (toggle && openIn(s, flyoutRef.current, id, hidden)) {
        setHidden(false);
        if (w?.zone === 'strip') setFlyout(null);
        else if (w?.zone === 'floating') apply({ op: 'closeGroup', zone: 'floating', group: w.group });
        else if (w?.zone === 'column') apply({ op: 'collapseColumn', collapsed: true });
        else if (w) apply({ op: 'collapseArea', zone: w.zone as Area, collapsed: true });
        return;
      }
      setHidden(false);
      if (w?.zone === 'strip') { setFlyout({ group: w.group, active: id }); return; }
      apply({ op: 'openPanel', panel: id, view: view() });
    },
    [apply, hidden, setFlyout],
  );

  const reset = useCallback(() => { setState(CLASSIC); setFlyout(null); setHidden(false); }, [setState, setFlyout]);
  /** A whole saved layout (a workspace), repaired as any layout read from the settings. */
  const load = useCallback((raw: unknown) => { setState(withViews(normalizeDock(raw, dockEnv) as DockState)); setFlyout(null); }, [setState, setFlyout]);

  /** Shows one of the Folio views (Window ▸ Project, Window ▸ Tools, the activity bar): where it is; closed, back at its home in the
   *  left area (the explorer at the top, the tools at the bottom). */
  const showView = useCallback((id: ViewId) => {
    setHidden(false);
    if (!whereIs(stateRef.current, id)) {
      const mate = whereIs(stateRef.current, id === 'explorer' ? 'toolbox' : 'explorer');
      const to: DropTarget = mate?.zone === 'left' ? { zone: 'left', group: mate.group, edge: id === 'explorer' ? 'top' : 'bottom' } : { zone: 'left', edge: id === 'explorer' ? 'top' : 'bottom' };
      apply({ op: 'movePanel', panel: id, to });
    }
    return apply({ op: 'openPanel', panel: id, view: view() });
  }, [apply]);

  /** Hides a Folio view the way the activity bar does: its group folds while another group of its area still shows, else the area
   *  folds to icons (the sidebar closes). A floating one closes. */
  const hideView = useCallback((id: PanelId) => {
    const s = stateRef.current;
    const w = whereIs(s, id);
    if (!w) return;
    if (w.zone === 'strip') { setFlyout(null); return; }
    if (w.zone === 'floating') { apply({ op: 'closePanel', panel: id }); return; }
    const groups = zoneGroups(s, w.zone);
    const others = groups.some((g, i) => i !== w.group && !s.folded?.[g[0]]);
    if (others) apply({ op: 'foldGroup', zone: w.zone, group: w.group, folded: true });
    else apply(w.zone === 'column' ? { op: 'collapseColumn', collapsed: true } : { op: 'collapseArea', zone: w.zone, collapsed: true });
  }, [apply, setFlyout]);

  /** Whether a panel shows now with its contents (its area open, its tab in front, its group not folded to its title strip). */
  const showing = useCallback((id: PanelId) => {
    if (!openIn(state, flyout, id, hidden)) return false;
    const w = whereIs(state, id);
    if (!w || !isAreaZone(w.zone)) return true;
    const g = zoneGroups(state, w.zone)[w.group];
    return state.folded?.[g[0]] !== true;
  }, [state, flyout, hidden]);

  /**
   * A pointer went down on something that can be dragged (a tab, a dock icon, the empty part of a panel title). Past a few pixels it
   * is a drag: a ghost follows the pointer, the place under it lights up (and the part of the screen the panel would take shows), and
   * letting go runs movePanel / moveGroup there, or floats the panel where it was let go (over no place). Escape cancels it.
   * Short of those few pixels nothing happens and the click goes through as usual.
   *
   * Across windows (uix-d3, as a browser's tabs): the drag may begin in a panel's own OS window too, and the pointer may go over the
   * app window, another panel window, or out of them all. The pointer is held (pointer capture), so the window it began in keeps
   * getting its moves wherever it goes; the place on the desktop is that window's page place (the Electron main process tells where
   * each window's page is) plus the pointer's place in it, and the window under it shows where the panel would land. Let go over the
   * app window: as in it; over a panel window: its group; outside them all: an OS window of its own there (the group's own window, when
   * all of it is dragged, moves there instead). While the pointer is outside the panel window it began in, that window is see-through,
   * so it hides no place to drop on.
   */
  const beginDrag = useCallback((e: React.PointerEvent, source: Source) => {
    if (e.button !== 0) return;
    const src = ((e.view as unknown as Window | null) ?? window);
    const srcName = src === window ? null : nameOf(src);
    if (srcName === undefined) return;
    const panels = source.kind === 'panel' ? [source.id] : (zoneGroups(stateRef.current, source.zone) as PanelId[][])[source.group] ?? [];
    const label = panels.map((id) => PANEL_META[id].title).join(' · ');
    const start = { x: e.clientX, y: e.clientY };
    const held = e.currentTarget as Element;
    let started = false;
    let key: string | null = null;
    const at = { x: start.x, y: start.y };
    const id = e.pointerId;
    // where the app's windows are on the desktop (asked once, at the press; they do not move during a drag)
    let places: WindowPlaces | null = null;
    if (canPopOut()) void windowPlaces().then((p) => { places = p; });
    let faded = false;
    const fade = (on: boolean) => {
      if (!srcName || faded === on) return;
      faded = on;
      void window.folio.popoutWindow?.(srcName, { opacity: on ? 0.35 : 1 })?.catch(() => {});
    };
    /** The window under the pointer (`name` null: the app window; undefined: none of them), the point in its page, the desktop point. */
    const where = (ev: PointerEvent): { name: string | null | undefined; at: Point; screen: Point } => {
      const own = { x: ev.clientX, y: ev.clientY };
      const p = places as WindowPlaces | null;
      const ownPlace = p ? (srcName ? p.popouts.find((w) => w.name === srcName) : p.main) : undefined;
      const screen = ownPlace ? onDesktop(own, ownPlace) : { x: ev.screenX, y: ev.screenY };
      if (p) { const w = windowAt(screen, p); return w && (w.name === null || panelWindows.has(w.name)) ? { name: w.name, at: w.at, screen } : { name: undefined, at: own, screen }; }
      const inside = own.x >= 0 && own.y >= 0 && own.x < src.innerWidth && own.y < src.innerHeight;
      return { name: inside ? srcName : undefined, at: own, screen };
    };
    /** The group a panel window shows (by its name), or -1. */
    const groupOf = (name: string) => groupOfWindow(name, stateRef.current.floating);
    /** Where a drop at a point of a window's page would land: the dock's places in the app window; in a panel window its own places,
     *  else its group (all of its page is that group). */
    const hitIn = (name: string | null, p: Point) => {
      if (name === null) return targetAt(p.x, p.y);
      const doc = panelWindows.get(name)?.win.document;
      if (!doc) return null;
      const hit = targetAt(p.x, p.y, null, doc);
      if (hit) return hit;
      const g = groupOf(name);
      if (g < 0) return null;
      const to: DropTarget = { zone: 'floating', group: g };
      const frame = doc.querySelector('.floating');
      return { to, key: dropKey(to), preview: frame ? rectOf(frame) : null };
    };
    // what a drop outside every window does: the panel window it began in moves there (all of its group dragged), or a new one opens
    const fromWin = srcName ? stateRef.current.floating[groupOf(srcName)] : undefined;
    const whole = !!fromWin?.window && (source.kind === 'group' || fromWin.panels.length === 1);
    // (what a drop does comes first: a long group label is cut at its end)
    const outLabel = !canPopOut() ? label : `${whole ? tr('창 옮기기') : tr('새 창으로')} · ${label}`;
    const ghostIn = (name: string | null) => (name === null ? document : panelWindows.get(name)?.win.document)?.querySelector<HTMLElement>('.dock-ghost') ?? null;
    const move = (ev: PointerEvent) => {
      // (another finger or pen while one drags is not this drag: the panel must not jump to it)
      if (ev.pointerId !== id) return;
      if (!started && Math.hypot(ev.clientX - start.x, ev.clientY - start.y) < 5) return;
      const first = !started;
      if (first) {
        started = true;
        setFlyout(null);
        // held: the moves keep coming to this window when the pointer leaves it
        try { held.setPointerCapture(id); } catch { /* a pointer the page does not know (a synthetic one) */ }
      }
      const w = where(ev);
      fade(w.name !== srcName);
      const out = w.name === undefined;
      const hit = out ? null : hitIn(w.name as string | null, w.at);
      // outside every window of the app the ghost waits at the edge of the window the drag began in, saying what a drop does there
      // (nothing can be drawn on the desktop itself)
      // (the ghost is at most 320 px wide, less in a narrow window: motion.css; it sits 12 px right of the point)
      at.x = out ? Math.max(0, Math.min(src.innerWidth - 12 - Math.min(320, src.innerWidth - 24), w.at.x)) : w.at.x;
      at.y = out ? Math.max(0, Math.min(src.innerHeight - 28, w.at.y)) : w.at.y;
      const shownIn = out ? srcName : (w.name as string | null);
      const over = out ? '' : `${w.name ?? ''}|${hit?.key ?? ''}`;
      // the app re-renders only when the place under the pointer changes; in between, the ghost alone follows the pointer
      // (a state update at every pointer move re-rendered the whole window for each one: uix-b4, animation-performance)
      if (first || over !== key) { key = over; setDrag({ label: out ? outLabel : label, at: { ...at }, key: hit?.key ?? null, preview: hit?.preview ?? null, ghost: true, win: shownIn }); return; }
      const ghost = ghostIn(shownIn);
      if (ghost) placeGhost(ghost.style, at.x, at.y);
    };
    const end = () => {
      src.removeEventListener('pointermove', move);
      src.removeEventListener('pointerup', up);
      src.removeEventListener('keydown', esc, true);
      if (src !== window) window.removeEventListener('keydown', esc, true);
      try { if (held.hasPointerCapture?.(id)) held.releasePointerCapture(id); } catch { /* gone */ }
      fade(false);
      setDrag(null);
    };
    const up = (ev: PointerEvent) => {
      if (ev.pointerId !== id) return;
      end();
      if (!started) return;
      justDragged.current = true;
      window.setTimeout(() => { justDragged.current = false; }, 60);
      const send = (to: DropTarget) => apply(source.kind === 'panel' ? { op: 'movePanel', panel: source.id, to } : { op: 'moveGroup', zone: source.zone, group: source.group, to });
      const w = where(ev);
      if (w.name !== undefined) {
        const hit = hitIn(w.name, w.at);
        if (hit) send(hit.to);
        // over the app window, nowhere in the dock: a floating panel there (over the panel window it began in: nothing changes)
        else if (w.name === null) send({ zone: 'floating', x: w.at.x - 40, y: w.at.y - 12 });
        return;
      }
      // outside every window of the app
      const inApp = (p: Point) => ({ x: Math.round(Math.max(0, Math.min(window.innerWidth - 240, p.x))), y: Math.round(Math.max(40, Math.min(window.innerHeight - 200, p.y))) });
      if (!canPopOut()) { send({ zone: 'floating', ...inApp({ x: ev.clientX - 40, y: ev.clientY - 12 }) }); return; }
      const fromGroup = srcName ? groupOf(srcName) : -1;
      const f = fromGroup >= 0 ? stateRef.current.floating[fromGroup] : null;
      const place = { x: Math.round(w.screen.x - 40), y: Math.round(w.screen.y - 12) };
      // all of a panel window's group: the window itself goes there (as moving it by its title bar: no undo step)
      if (f?.window && srcName && (source.kind === 'group' || f.panels.length === 1)) {
        const rect = { ...place, w: f.window.w, h: f.window.h };
        apply({ op: 'moveWindow', group: fromGroup, window: rect });
        void window.folio.popoutWindow?.(srcName, { bounds: rect })?.catch(() => {});
        return;
      }
      // else an OS window of its own there, the size the panel has now (inside the app window: where it would float there)
      const meta = source.kind === 'panel' ? PANEL_META[source.id] : null;
      const size = f?.window ? { w: f.window.w, h: f.window.h } : meta ? { w: Math.max(240, meta.width), h: Math.max(200, meta.height) + 32 }
        : popOutRect(stateRef.current, source.kind === 'group' ? source.zone : 'floating', source.kind === 'group' ? source.group : -1);
      const main = (places as WindowPlaces | null)?.main;
      const inside = main ? inApp({ x: (w.screen.x - main.x) / (main.zoom || 1), y: (w.screen.y - main.y) / (main.zoom || 1) }) : { x: 120, y: 120 };
      const win: OsWindow = { ...place, w: size.w, h: size.h };
      send({ zone: 'floating', ...inside, window: win });
    };
    const esc = (ev: KeyboardEvent) => {
      if (ev.key !== 'Escape') return;
      ev.stopPropagation();
      end();
      justDragged.current = true;
      window.setTimeout(() => { justDragged.current = false; }, 60);
    };
    src.addEventListener('pointermove', move);
    src.addEventListener('pointerup', up);
    src.addEventListener('keydown', esc, true);
    if (src !== window) window.addEventListener('keydown', esc, true);
  }, [apply, setFlyout]);

  /**
   * A floating panel moved by its title (or resized by an edge or a corner, `resize`): it follows the pointer itself (the DOM, not a
   * render per move), pulls onto the window's and the other floating panels' edges near it, and on letting go is kept where it is
   * (moveFloating) or, moved over a place of the dock, docks there (Ctrl held: it never docks, as in Adobe's programs). Escape puts it back.
   * Let go outside the app window (uix-d3): over a panel window it joins that window's group; outside them all it goes out to an OS
   * window of its own there (the pointer is held, so the app window still hears it out there).
   */
  const beginFloat = useCallback((e: React.PointerEvent, i: number, resize: string | null = null) => {
    if (e.button !== 0) return;
    const f = stateRef.current.floating[i];
    const el = (e.currentTarget as HTMLElement).closest<HTMLElement>('.floating');
    if (!f || !el) return;
    e.preventDefault();
    const held = e.currentTarget as Element;
    let places: WindowPlaces | null = null;
    if (!resize && canPopOut()) void windowPlaces().then((p) => { places = p; });
    const start = { x: e.clientX, y: e.clientY };
    const from = { x: f.x, y: f.y, w: f.w, h: f.h };
    let rect = { ...from };
    let started = false;
    let key: string | null = null;
    let hit: ReturnType<typeof targetAt> = null;
    const others = stateRef.current.floating.filter((_, j) => j !== i).map((g) => ({ x: g.x, y: g.y, w: g.w, h: g.h }));
    const show = (r: Rect) => { el.style.left = `${r.x}px`; el.style.top = `${r.y}px`; el.style.width = `${r.w}px`; el.style.height = `${r.h}px`; };
    const id = e.pointerId;
    const move = (ev: PointerEvent) => {
      if (ev.pointerId !== id) return;
      const dx = ev.clientX - start.x, dy = ev.clientY - start.y;
      if (!started && Math.hypot(dx, dy) < 3) return;
      const first = !started;
      started = true;
      if (first) { try { held.setPointerCapture(id); } catch { /* a synthetic pointer */ } }
      const v = { ...view(), top: 0 };
      if (!resize) {
        rect = snapRect({ ...from, x: Math.max(0, Math.min(v.w - 80, from.x + dx)), y: Math.max(32, Math.min(v.h - 40, from.y + dy)) }, others, v);
        hit = ev.ctrlKey ? null : targetAt(ev.clientX, ev.clientY, i);
        const over = hit?.key ?? null;
        // the dock shows its places (and what the panel would take) while it moves; one render when the place changes
        if (first || over !== key) { key = over; setDrag({ label: '', at: { x: ev.clientX, y: ev.clientY }, key: over, preview: hit?.preview ?? null, ghost: false }); }
      } else {
        const r = { ...from };
        if (resize.includes('e')) r.w = Math.max(180, from.w + dx);
        if (resize.includes('s')) r.h = Math.max(90, from.h + dy);
        if (resize.includes('w')) { const w = Math.max(180, from.w - dx); r.x = from.x + from.w - w; r.w = w; }
        if (resize.includes('n')) { const h = Math.max(90, from.h - dy); r.y = Math.max(32, from.y + from.h - h); r.h = from.y + from.h - r.y; }
        rect = snapRect(r, others, v, { l: resize.includes('w'), r: resize.includes('e'), t: resize.includes('n'), b: resize.includes('s') });
      }
      show(rect);
    };
    const end = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('keydown', esc, true);
      try { if (held.hasPointerCapture?.(id)) held.releasePointerCapture(id); } catch { /* gone */ }
      setDrag(null);
    };
    const up = (ev: PointerEvent) => {
      if (ev.pointerId !== id) return;
      end();
      if (!started) return;
      const out = ev.clientX < 0 || ev.clientY < 0 || ev.clientX >= window.innerWidth || ev.clientY >= window.innerHeight;
      if (out && !resize && canPopOut()) {
        const p = places as WindowPlaces | null;
        const screen = p ? onDesktop({ x: ev.clientX, y: ev.clientY }, p.main) : { x: ev.screenX, y: ev.screenY };
        const w = p ? windowAt(screen, p) : null;
        const j = w?.name ? groupOfWindow(w.name, stateRef.current.floating) : -1;
        show(from);
        if (j >= 0) { apply({ op: 'moveGroup', zone: 'floating', group: i, to: { zone: 'floating', group: j } }); return; }
        if (!w) { apply({ op: 'popOut', zone: 'floating', group: i, window: { x: Math.round(screen.x - 40), y: Math.round(screen.y - 12), w: Math.round(from.w), h: Math.round(from.h) + 32 } }); return; }
      }
      if (hit && !resize) apply({ op: 'moveGroup', zone: 'floating', group: i, to: hit.to });
      else apply({ op: 'moveFloating', group: i, x: rect.x, y: rect.y, w: rect.w, h: rect.h, view: view() });
    };
    const esc = (ev: KeyboardEvent) => {
      if (ev.key !== 'Escape') return;
      ev.stopPropagation();
      end();
      show(from);
      started = false;
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('keydown', esc, true);
  }, [apply]);

  // the window (or the monitor) got smaller: floating panels come back onto it
  useEffect(() => {
    let t: number | undefined;
    const fit = () => { window.clearTimeout(t); t = window.setTimeout(() => { if (stateRef.current.floating.length) apply({ op: 'fitFloating', view: view() }); }, 150); };
    window.addEventListener('resize', fit);
    return () => { window.removeEventListener('resize', fit); window.clearTimeout(t); };
  }, [apply]);

  /** Whether the click that follows a drag should be ignored (it ends the drag; it is not a press on the tab). */
  const dragged = useCallback(() => justDragged.current, []);

  return { state, setState, flyout, setFlyout, show, isOpen, where, reset, load, hidden, setHidden, apply, layout, drag, beginDrag, beginFloat, dragged, showView, hideView, showing, act, extra, leftOut,
    undo, redo, step, steps, companion };
}

export type Dock = ReturnType<typeof useDock>;
