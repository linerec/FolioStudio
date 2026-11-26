import panelSpec from '../../../../shared/panels.json';
// The panel dock's layout and the commands that change it. Pure data and functions: no React, no window, no storage, so the same
// logic runs on any screen (a phone has no Electron), in tests, and behind a command interface (the planned MCP server) that must
// do whatever a person does with the mouse: the screen only turns a drag into one of these commands.
//
// The layout is where each panel sits (uix-d1, 2026-11-20: docking at the level of VS Code, JetBrains, Visual Studio and Adobe):
//   strip     a column of panel icons (InDesign's narrow dock); a group's panels fly out to the left as tabs of one panel;
//   areas     three dock areas around the document (the middle is always the document): `left`, `column` (the right one; its old
//             name is kept so older layouts and tools still read) and `bottom`. An area holds a split tree: parts side by side
//             (`row`) or one above another (`col`), each part a split again or a group (the tabs of one panel frame). The groups of
//             an area are also kept as a flat list in reading order (`column`, `left`, `bottom`), and the tree (`layout`) names them by
//             their number in that list; an area with no tree is one group above another (the bottom: side by side);
//   floating  panels of their own over the window, each remembering where it was docked (`home`, for a double click on its title).
// A group is a list of panels that share one panel frame (tabs). Dropping a panel onto a group's tabs joins them; onto a group's
// edge splits the group there; onto an area's outer edge makes a new column or row across it; between groups (or at an end) makes
// a group of its own there; onto the icon strip makes an icon; anywhere else floats it.
//
// The layout belongs to this computer (the app's settings, with the other user settings, key `dock`); a project folder never holds
// one. Its shape is DockState and stays readable by older and newer layouts (`normalizeDock` repairs anything it is given; the new
// fields are left out while they hold their defaults, so an older layout reads back exactly as it was).

export type Area = 'left' | 'column' | 'bottom';
export const AREAS: readonly Area[] = ['left', 'column', 'bottom'];
export type Zone = 'strip' | Area | 'floating';
export type Edge = 'left' | 'right' | 'top' | 'bottom';
export type Dir = 'row' | 'col';

/** How an area's groups are arranged: parts side by side (`row`) or one above another (`col`), with their shares (summing to 1).
 *  A number is a group: its place in the area's flat list. */
export type Split = { dir: Dir; parts: (Split | number)[]; sizes: number[] };
/** Where a floating panel was docked: a zone, the group's number there then, and a panel it shared the group with (if any). */
export type Home = { zone: 'strip' | Area; group: number; with?: string };

/** An operating-system window of its own (uix-d2: a floating group taken out to another monitor), in screen pixels of the desktop. */
export type OsWindow = { x: number; y: number; w: number; h: number };
/** `window`: the group is drawn in an OS window of its own there (left out: inside the app window at x, y). A screen with one window
 *  only (a phone) ignores it and draws the group inside. */
export type Floating<P extends string = string> = { panels: P[]; active: P; x: number; y: number; w: number; h: number; home?: Home; window?: OsWindow };

export type DockState<P extends string = string> = {
  strip: P[][];
  /** The right area's groups, in reading order. */
  column: P[][];
  /** The tab showing in each strip / area group, by the group's first panel. */
  active: Record<string, P>;
  columnCollapsed: boolean;
  /** Area groups folded to their title strip, by the group's first panel. */
  folded?: Record<string, boolean>;
  floating: Floating<P>[];
  /** The left and bottom areas' groups (left out while empty). */
  left?: P[][];
  bottom?: P[][];
  /** How each area's groups are arranged (left out while it is the plain one: one above another, the bottom side by side). */
  layout?: Partial<Record<Area, Split>>;
  /** An area's width (left, right) or height (bottom) in screen pixels (left out: the default). */
  size?: Partial<Record<Area, number>>;
  /** The left and bottom areas folded to a row of icons (the right one is `columnCollapsed`). */
  collapsed?: { left?: boolean; bottom?: boolean };
  /** A panel whose group has its area to itself for now (maximized). */
  maximized?: P;
};

/** The default layout (panels.json): InDesign Essentials Classic's dock, and the Folio workspace's two views in the left area (the
 *  explorer above the tools, uix-d2). Each workspace leaves out what it shows otherwise (AppLayout's hide sets): Folio the project
 *  panel (the explorer is the project), InDesign Classic the explorer and the tools (its tools are down the left edge). */
export const CLASSIC = panelSpec.default as unknown as DockState;

/** An area's default size and bounds (pixels), and the room the document always keeps. The left one starts as wide as the Folio
 *  sidebar was (264, uix-c1). */
export const AREA_SIZE: Record<Area, { def: number; min: number; max: number }> = {
  left: { def: 264, min: 160, max: 720 },
  column: { def: 272, min: 160, max: 720 },
  bottom: { def: 220, min: 90, max: 640 },
};
export const CANVAS_MIN = { w: 320, h: 200 };
/** The smallest share a part of a split can get by a drag (the screen also keeps parts at least a title strip tall). */
export const MIN_SHARE = 0.06;
const SNAP = 8;

/** Where a dragged panel (or group) is dropped. */
export type DropTarget =
  /** Into a group (of the strip, an area or a floating panel), as the tab number `index` (the end when left out). */
  | { zone: Zone; group: number; index?: number }
  /** Beside a group of an area: splits the group's place (left/right: side by side; top/bottom: one above the other). */
  | { zone: Area; group: number; edge: Edge }
  /** At an area's outer edge: a new column (left/right) or row (top/bottom) across the whole area; into an empty area. */
  | { zone: Area; edge: Edge }
  /** As a group of its own in front of group number `before` (`before` = the number of groups: at the end). */
  | { zone: 'strip' | Area; before: number }
  /** As a new floating panel at a place on the screen (pixels), with a size when it is known; `window`: in an OS window of its own
   *  there on the desktop (uix-d3: a panel dragged out of the app window and let go outside it), x and y its place inside. */
  | { zone: 'floating'; x: number; y: number; w?: number; h?: number; window?: OsWindow };

/** The commands. Every one returns a DockResult; none throws. */
export type DockCommand =
  /** Moves one panel (out of wherever it is, or in from "closed") to a target. */
  | { op: 'movePanel'; panel: string; to: DropTarget }
  /** Moves a whole group (all its tabs) to a target: reorders the groups of a zone, or moves them to another zone. */
  | { op: 'moveGroup'; zone: Zone; group: number; to: DropTarget }
  /** Brings a panel's tab to the front of its group. */
  | { op: 'setActive'; panel: string }
  /** Folds the whole right area to an icon strip, or unfolds it. */
  | { op: 'collapseColumn'; collapsed: boolean }
  /** Folds an area to a row of icons, or unfolds it (`column`: the same as collapseColumn). */
  | { op: 'collapseArea'; zone: Area; collapsed: boolean }
  /** Folds one area group to its title strip, or unfolds it (`zone` left out: the right area). */
  | { op: 'foldGroup'; group: number; folded: boolean; zone?: Area }
  /** Gives an area group its whole area for now (another command on the area, or `on: false`, gives the others back). */
  | { op: 'maximizeGroup'; zone: Area; group: number; on: boolean }
  /** New shares for the parts of a split of an area (`path`: the part numbers from the area's top split down; [] is the top). */
  | { op: 'resizeSplit'; zone: Area; path: number[]; sizes: number[] }
  /** Equal shares for the parts of a split (a double click on the line between them). */
  | { op: 'equalizeSplit'; zone: Area; path: number[] }
  /** An area's width (left, right) or height (bottom) in pixels; the document keeps its room (`view`, when given). */
  | { op: 'setAreaSize'; zone: Area; size: number; view?: { w: number; h: number } }
  /** Takes a group out of the dock as a floating panel (at x, y when given), remembering where it was. */
  | { op: 'floatGroup'; zone: 'strip' | Area; group: number; x?: number; y?: number }
  /** Moves or resizes a floating panel; kept on the screen (`view`, when given). */
  | { op: 'moveFloating'; group: number; x: number; y: number; w?: number; h?: number; view?: { w: number; h: number } }
  /** Puts a floating panel back where it was docked (a double click on its title). */
  | { op: 'dockFloating'; group: number }
  /** Pulls every floating panel back onto a screen of this size (the window or the monitor changed). */
  | { op: 'fitFloating'; view: { w: number; h: number } }
  /** Takes a group out to an operating-system window of its own (another monitor), floating it first when it is docked; `window`:
   *  where on the desktop (left out: the screen chooses, over the group's place). */
  | { op: 'popOut'; zone: Zone; group: number; window?: OsWindow }
  /** A floating group's OS window back inside the app window (the group stays floating there). */
  | { op: 'popIn'; group: number }
  /** Where a group's OS window is now (moved or resized by hand). */
  | { op: 'moveWindow'; group: number; window: OsWindow }
  /** Shows a panel: its tab comes to the front; a closed panel opens floating. */
  | { op: 'openPanel'; panel: string; view?: { w: number; h: number } }
  /** Takes a panel out of the layout (the panel is closed). */
  | { op: 'closePanel'; panel: string }
  /** Closes all the panels of a group. */
  | { op: 'closeGroup'; zone: Zone; group: number }
  /** The default layout (Essentials Classic's). */
  | { op: 'reset' };

export type DockErrorKind = 'unknownPanel' | 'badTarget' | 'badGroup' | 'notInLayout' | 'invalid';
export type DockResult = { ok: true; state: DockState; changed: boolean } | { ok: false; error: DockErrorKind; message: string };

/** What the screen knows: which ids are panels, and how big a panel wants to be when it floats. */
export type DockEnv = {
  known?: (id: string) => boolean;
  size?: (id: string) => { w: number; h: number };
  /** Panels the workspace in use leaves out (they keep their place): a group of only those takes no column when an area's width
   *  follows its columns (uix-d2: the Folio views in InDesign Classic). */
  hidden?: (id: string) => boolean;
};

const TARGETS = '{zone, group, index} (join a group) | {zone: area, group, edge} (split beside it) | {zone: area, edge} (new column or row at the area edge) | {zone, before} (a group of its own) | {zone: "floating", x, y, window? {x, y, w, h} (desktop pixels: in an operating-system window of its own)}; zones: strip, left, column (the right area), bottom, floating; edges: left, right, top, bottom';
/** The commands for tool descriptions (the MCP server builds its tools from this table). */
export const DOCK_COMMANDS: { op: DockCommand['op']; summary: string; args: string }[] = [
  { op: 'movePanel', summary: 'Move a panel to a group of the dock (join its tabs), beside a group (split), to an area edge, between groups, or out as a floating panel.', args: `panel, to: ${TARGETS}` },
  { op: 'moveGroup', summary: 'Move all panels of a group together (reorder groups, split beside another group, or send a group to another zone).', args: `zone, group, to: ${TARGETS}` },
  { op: 'setActive', summary: 'Bring a panel to the front of its tab group.', args: 'panel' },
  { op: 'collapseColumn', summary: 'Fold the right dock area to icons, or unfold it.', args: 'collapsed' },
  { op: 'collapseArea', summary: 'Fold a dock area (left, column, bottom) to icons, or unfold it.', args: 'zone, collapsed' },
  { op: 'foldGroup', summary: 'Fold one group of a dock area to its title strip, or unfold it.', args: 'group, folded, zone? (default column)' },
  { op: 'maximizeGroup', summary: 'Let one group fill its dock area for now, or give the other groups back.', args: 'zone, group, on' },
  { op: 'resizeSplit', summary: 'Set the shares of the parts of a split in a dock area.', args: 'zone, path (part numbers from the top split), sizes' },
  { op: 'equalizeSplit', summary: 'Give the parts of a split equal shares.', args: 'zone, path' },
  { op: 'setAreaSize', summary: 'Set a dock area width (left, column) or height (bottom) in pixels.', args: 'zone, size, view? {w, h}' },
  { op: 'floatGroup', summary: 'Take a group out of the dock as a floating panel; it remembers where it was.', args: 'zone, group, x?, y?' },
  { op: 'moveFloating', summary: 'Move or resize a floating panel.', args: 'group, x, y, w?, h?, view?' },
  { op: 'dockFloating', summary: 'Put a floating panel back where it was docked.', args: 'group' },
  { op: 'fitFloating', summary: 'Pull the floating panels back onto a screen of this size.', args: 'view {w, h}' },
  { op: 'popOut', summary: 'Take a group out to an operating-system window of its own (another monitor); closing that window docks it back.', args: 'zone, group, window? {x, y, w, h} (desktop pixels)' },
  { op: 'popIn', summary: 'Bring a floating group\'s operating-system window back inside the app window.', args: 'group (floating)' },
  { op: 'moveWindow', summary: 'Record where a floating group\'s operating-system window is.', args: 'group (floating), window {x, y, w, h}' },
  { op: 'openPanel', summary: 'Show a panel (bring its tab to the front, or open a closed panel floating).', args: 'panel, view?' },
  { op: 'closePanel', summary: 'Close a panel (take it out of the layout).', args: 'panel' },
  { op: 'closeGroup', summary: 'Close all panels of a group.', args: 'zone, group' },
  { op: 'reset', summary: 'Back to the default panel layout.', args: '' },
];

const FLOAT_DEFAULT = { w: 280, h: 360 };
const DEFAULT_DIR: Record<Area, Dir> = { left: 'col', column: 'col', bottom: 'row' };
const isArea = (z: unknown): z is Area => z === 'left' || z === 'column' || z === 'bottom';

// ---------------------------------------------------------------------------------------------------------------------------
// working form: groups and splits as objects, so a target can name a group while panels are taken out of other groups

type G = { kind: 'g'; panels: string[]; active: string; folded: boolean; x: number; y: number; w: number; h: number; home?: Home; win?: OsWindow };
type S = { kind: 's'; dir: Dir; parts: T[]; sizes: number[] };
type T = G | S;
type Work = {
  strip: G[]; floating: G[]; area: Record<Area, T | null>;
  columnCollapsed: boolean; collapsed: { left: boolean; bottom: boolean };
  size: Partial<Record<Area, number>>; maximized: string | null;
};

const isRec = (v: unknown): v is Record<string, unknown> => !!v && typeof v === 'object' && !Array.isArray(v);
const num = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? v : d);
const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
const mkG = (panels: string[], active?: string, folded = false): G => ({ kind: 'g', panels: [...panels], active: active && panels.includes(active) ? active : panels[0], folded, x: 0, y: 0, w: 0, h: 0 });

/** The groups of a tree in reading order. */
function dfs(t: T | null, out: G[] = []): G[] {
  if (!t) return out;
  if (t.kind === 'g') out.push(t); else for (const p of t.parts) dfs(p, out);
  return out;
}

const equalShares = (n: number) => Array.from({ length: n }, () => 1 / n);
function share(sizes: number[]): number[] {
  const s = sizes.map((v) => (Number.isFinite(v) && v > 0 ? v : 0));
  const sum = s.reduce((a, b) => a + b, 0);
  return sum > 0 ? s.map((v) => v / sum) : equalShares(s.length);
}
const allEqual = (sizes: number[]) => sizes.every((v) => Math.abs(v - 1 / sizes.length) < 1e-6);

/** A tree without empty groups, splits of one part, or a split inside a split of the same direction; shares sum to 1. */
function prune(t: T | null): T | null {
  if (!t) return null;
  if (t.kind === 'g') return t.panels.length ? t : null;
  const parts: T[] = [];
  const sizes: number[] = [];
  const base = t.sizes.length === t.parts.length ? t.sizes : equalShares(t.parts.length);
  const wasEqual = allEqual(share(base));
  t.parts.forEach((p, i) => {
    const q = prune(p);
    if (!q) return;
    if (q.kind === 's' && q.dir === t.dir) { q.parts.forEach((r, j) => { parts.push(r); sizes.push(base[i] * q.sizes[j]); }); }
    else { parts.push(q); sizes.push(base[i]); }
  });
  if (!parts.length) return null;
  if (parts.length === 1) return parts[0];
  t.parts = parts;
  t.sizes = wasEqual ? equalShares(parts.length) : share(sizes);
  return t;
}

function parentOf(root: T | null, node: T): { p: S; i: number } | null {
  if (!root || root.kind === 'g') return null;
  for (let i = 0; i < root.parts.length; i++) {
    if (root.parts[i] === node) return { p: root, i };
    const r = parentOf(root.parts[i], node);
    if (r) return r;
  }
  return null;
}

/** Puts `g` into split `p` at place `at`: equal parts stay equal; otherwise the newcomer gets an even share. */
function insertPart(p: S, at: number, g: T, give?: number) {
  const n = p.parts.length;
  if (give !== undefined) {
    // the newcomer takes part of one neighbour's share (a split beside one group)
    const half = p.sizes[give] / 2;
    p.sizes[give] = half;
    p.parts.splice(at, 0, g);
    p.sizes.splice(at, 0, half);
    return;
  }
  const equal = allEqual(p.sizes);
  p.parts.splice(at, 0, g);
  if (equal) p.sizes = equalShares(n + 1);
  else { const s = 1 / (n + 1); p.sizes = p.sizes.map((v) => v * (1 - s)); p.sizes.splice(at, 0, s); }
}

function replaceNode(w: Work, area: Area, old: T, neu: T) {
  if (w.area[area] === old) { w.area[area] = neu; return; }
  const at = parentOf(w.area[area], old);
  if (at) at.p.parts[at.i] = neu;
}

/** Beside a group (or any part): in its split when that runs the same way, else the part becomes a split of the two. */
function insertBeside(w: Work, area: Area, target: T, g: G, dir: Dir, before: boolean) {
  const at = parentOf(w.area[area], target);
  if (at && at.p.dir === dir) { insertPart(at.p, before ? at.i : at.i + 1, g, at.i); return; }
  replaceNode(w, area, target, { kind: 's', dir, parts: before ? [g, target] : [target, g], sizes: [0.5, 0.5] });
}

/** At an area's outer edge: the first or last part of a top split that runs that way, else a new top split. */
function insertAtEdge(w: Work, area: Area, g: G, edge: Edge) {
  const dir: Dir = edge === 'left' || edge === 'right' ? 'row' : 'col';
  const first = edge === 'left' || edge === 'top';
  const root = w.area[area];
  if (!root) { w.area[area] = g; return; }
  if (root.kind === 's' && root.dir === dir) { insertPart(root, first ? 0 : root.parts.length, g); return; }
  w.area[area] = { kind: 's', dir, parts: first ? [g, root] : [root, g], sizes: [0.5, 0.5] };
}

/** How many groups stand side by side across an area at most (left and right areas: columns; the bottom one: rows). */
function breadth(t: T | null, area: Area, hidden?: (id: string) => boolean): number {
  if (!t) return 0;
  if (t.kind === 'g') return hidden && t.panels.every(hidden) ? 0 : 1;
  const across = t.dir !== DEFAULT_DIR[area];
  const each = t.parts.map((p) => breadth(p, area, hidden));
  return across ? each.reduce((a, b) => a + b, 0) : Math.max(0, ...each);
}

/** An area that gains a column (a row, for the bottom) widens by a column's default size, and narrows when it loses one, as the dock
 *  columns of InDesign do: two panels side by side in a 272 px area would each be too narrow to use. */
function fitBreadth(w: Work, before: Record<Area, number>, hidden?: (id: string) => boolean) {
  for (const a of AREAS) {
    const after = breadth(w.area[a], a, hidden);
    if (!after || !before[a] || after === before[a]) continue;
    const b = AREA_SIZE[a];
    w.size[a] = Math.round(clamp((w.size[a] ?? b.def) + (after - before[a]) * b.def, b.min, b.max));
  }
}

/** As a group of its own in front of an area's group `at` (null: after the last one). */
function insertBefore(w: Work, area: Area, at: G | null, g: G) {
  const dir = DEFAULT_DIR[area];
  const root = w.area[area];
  if (!root) { w.area[area] = g; return; }
  if (!at) {
    if (root.kind === 's' && root.dir === dir) insertPart(root, root.parts.length, g);
    else { const n = dfs(root).length; w.area[area] = { kind: 's', dir, parts: [root, g], sizes: [n / (n + 1), 1 / (n + 1)] }; }
    return;
  }
  const p = parentOf(root, at);
  if (p && p.p.dir === dir) insertPart(p.p, p.i, g);
  else insertBeside(w, area, at, g, dir, true);
}

/** The split at `path` in an area (null when there is none). */
function splitAt(w: Work, area: Area, path: number[]): S | null {
  let t: T | null = w.area[area];
  for (const i of path) { if (!t || t.kind !== 's' || !Number.isInteger(i) || !t.parts[i]) return null; t = t.parts[i]; }
  return t && t.kind === 's' ? t : null;
}

const groupsOf = (w: Work, zone: Zone): G[] => (zone === 'strip' ? w.strip : zone === 'floating' ? w.floating : dfs(w.area[zone]));
const everyGroup = (w: Work): G[] => [...w.strip, ...AREAS.flatMap((a) => dfs(w.area[a])), ...w.floating];

/** Where a group is in the working form. */
function locate(w: Work, g: G): { zone: Zone; group: number } | null {
  for (const zone of ['strip', ...AREAS, 'floating'] as Zone[]) { const i = groupsOf(w, zone).indexOf(g); if (i >= 0) return { zone, group: i }; }
  return null;
}

// ---------------------------------------------------------------------------------------------------------------------------
// repair: any value (a saved layout, a hand-written one, an older version) becomes a layout that holds

function readSplit(v: unknown, gs: (G | null)[], used: Set<number>, depth = 0): T | null {
  if (depth > 24) return null;
  if (typeof v === 'number') {
    if (!Number.isInteger(v) || v < 0 || v >= gs.length || used.has(v)) return null;
    used.add(v);
    return gs[v];
  }
  if (!isRec(v) || (v.dir !== 'row' && v.dir !== 'col') || !Array.isArray(v.parts)) return null;
  const sizes = Array.isArray(v.sizes) && v.sizes.length === v.parts.length ? v.sizes.map((x) => num(x, 0)) : equalShares(v.parts.length);
  const parts: T[] = [];
  const kept: number[] = [];
  v.parts.forEach((p, i) => { const t = readSplit(p, gs, used, depth + 1); if (t) { parts.push(t); kept.push(sizes[i]); } });
  return parts.length ? { kind: 's', dir: v.dir, parts, sizes: share(kept) } : null;
}

function readHome(v: unknown, known: (id: string) => boolean): Home | undefined {
  if (!isRec(v) || !(v.zone === 'strip' || isArea(v.zone)) || !Number.isInteger(v.group) || (v.group as number) < 0) return undefined;
  const home: Home = { zone: v.zone, group: v.group as number };
  if (typeof v.with === 'string' && known(v.with)) home.with = v.with;
  return home;
}

/** An OS window's place (desktop pixels; a monitor left of or above the main one has negative ones), or undefined. */
function readWindow(v: unknown): OsWindow | undefined {
  if (!isRec(v)) return undefined;
  const n = (k: string) => (typeof v[k] === 'number' && Number.isFinite(v[k]) ? Math.round(v[k] as number) : NaN);
  const r = { x: n('x'), y: n('y'), w: n('w'), h: n('h') };
  return Number.isFinite(r.x) && Number.isFinite(r.y) && r.w >= 120 && r.h >= 80 ? r : undefined;
}

/** The working form of any value (the repair and the commands share it). */
function readWork(raw: unknown, env: DockEnv): Work {
  const known = env.known ?? (() => true);
  const seen = new Set<string>();
  const ids = (v: unknown): string[] => {
    const out: string[] = [];
    if (Array.isArray(v)) for (const id of v) if (typeof id === 'string' && known(id) && !seen.has(id)) { seen.add(id); out.push(id); }
    return out;
  };
  const r = isRec(raw) ? raw : {};
  const oldActive = isRec(r.active) ? r.active : {};
  const oldFolded = isRec(r.folded) ? r.folded : {};
  const showed = Object.values(oldActive);
  // the tab that showed in the group (named by its first panel); else a panel of the group that showed in any group before
  const mk = (panels: string[], fold: boolean): G => {
    const named = oldActive[panels[0]];
    const active = typeof named === 'string' && panels.includes(named) ? named : (panels.find((id) => showed.includes(id)) ?? panels[0]);
    return mkG(panels, active, fold && oldFolded[panels[0]] === true);
  };
  const strip = (Array.isArray(r.strip) ? r.strip : []).map(ids).filter((g) => g.length).map((g) => mk(g, false));
  const layouts = isRec(r.layout) ? r.layout : {};
  const area = { left: null, column: null, bottom: null } as Record<Area, T | null>;
  for (const a of AREAS) {
    const gs = (Array.isArray(r[a]) ? (r[a] as unknown[]) : []).map((g) => { const p = ids(g); return p.length ? mk(p, true) : null; });
    const used = new Set<number>();
    let root = readSplit(layouts[a], gs, used);
    // groups the tree does not name (or no tree at all): one after another in the area's plain direction
    const rest = gs.filter((g, i): g is G => !!g && !used.has(i));
    if (rest.length) {
      const dir = DEFAULT_DIR[a];
      const parts: T[] = root ? [root, ...rest] : rest;
      root = parts.length === 1 ? parts[0] : { kind: 's', dir, parts, sizes: equalShares(parts.length) };
    }
    area[a] = prune(root);
  }
  const floating: G[] = [];
  if (Array.isArray(r.floating)) {
    for (const f of r.floating) {
      if (!isRec(f)) continue;
      const panels = ids(f.panels);
      if (!panels.length) continue;
      const size = env.size?.(panels[0]) ?? FLOAT_DEFAULT;
      const g = mkG(panels, typeof f.active === 'string' ? f.active : undefined);
      g.x = Math.max(0, num(f.x, 120)); g.y = Math.max(0, num(f.y, 120));
      g.w = Math.max(120, num(f.w, size.w)); g.h = Math.max(80, num(f.h, size.h));
      const home = readHome(f.home, known);
      if (home) g.home = home;
      const win = readWindow(f.window);
      if (win) g.win = win;
      floating.push(g);
    }
  }
  const size: Partial<Record<Area, number>> = {};
  if (isRec(r.size)) for (const a of AREAS) { const v = r.size[a]; if (typeof v === 'number' && Number.isFinite(v)) size[a] = Math.round(clamp(v, AREA_SIZE[a].min, AREA_SIZE[a].max)); }
  const c = isRec(r.collapsed) ? r.collapsed : {};
  const w: Work = { strip, floating, area, columnCollapsed: r.columnCollapsed === true, collapsed: { left: c.left === true, bottom: c.bottom === true }, size, maximized: null };
  if (typeof r.maximized === 'string' && AREAS.some((a) => dfs(w.area[a]).some((g) => g.panels.includes(r.maximized as string)))) w.maximized = r.maximized;
  return w;
}

/** The plain arrangement needs no tree: one group, or groups one after another in the area's direction with equal shares. */
function plain(t: T | null, area: Area) {
  return !t || t.kind === 'g' || (t.dir === DEFAULT_DIR[area] && t.parts.every((p) => p.kind === 'g') && allEqual(t.sizes));
}

function writeSplit(t: T, list: G[]): Split | number {
  if (t.kind === 'g') return list.indexOf(t);
  return { dir: t.dir, parts: t.parts.map((p) => writeSplit(p, list)), sizes: t.sizes.map((v) => Math.round(v * 10000) / 10000) };
}

function fromWork(w: Work): DockState {
  for (const a of AREAS) w.area[a] = prune(w.area[a]);
  const strip = w.strip.filter((g) => g.panels.length);
  const lists = { left: dfs(w.area.left), column: dfs(w.area.column), bottom: dfs(w.area.bottom) };
  const active: Record<string, string> = {};
  const folded: Record<string, boolean> = {};
  for (const g of [...strip, ...lists.left, ...lists.column, ...lists.bottom]) active[g.panels[0]] = g.panels.includes(g.active) ? g.active : g.panels[0];
  for (const g of [...lists.left, ...lists.column, ...lists.bottom]) if (g.folded) folded[g.panels[0]] = true;
  const out: DockState = {
    strip: strip.map((g) => g.panels), column: lists.column.map((g) => g.panels), active, columnCollapsed: w.columnCollapsed, folded,
    floating: w.floating.filter((g) => g.panels.length).map((g) => {
      const f: Floating = { panels: g.panels, active: g.panels.includes(g.active) ? g.active : g.panels[0], x: g.x, y: g.y, w: g.w, h: g.h };
      if (g.home) f.home = g.home;
      if (g.win) f.window = g.win;
      return f;
    }),
  };
  if (lists.left.length) out.left = lists.left.map((g) => g.panels);
  if (lists.bottom.length) out.bottom = lists.bottom.map((g) => g.panels);
  const layout: Partial<Record<Area, Split>> = {};
  for (const a of AREAS) { const t = w.area[a]; if (t && !plain(t, a)) layout[a] = writeSplit(t, lists[a]) as Split; }
  if (Object.keys(layout).length) out.layout = layout;
  const size: Partial<Record<Area, number>> = {};
  for (const a of AREAS) if (w.size[a] !== undefined && w.size[a] !== AREA_SIZE[a].def) size[a] = w.size[a];
  if (Object.keys(size).length) out.size = size;
  if (w.collapsed.left || w.collapsed.bottom) out.collapsed = { ...(w.collapsed.left ? { left: true } : {}), ...(w.collapsed.bottom ? { bottom: true } : {}) };
  if (w.maximized && AREAS.some((a) => lists[a].some((g) => g.panels.includes(w.maximized as string)))) out.maximized = w.maximized;
  return out;
}

/** A usable layout out of anything: unknown panels and repeats dropped, empty groups gone, tabs, folds and splits kept where they still apply. */
export function normalizeDock(raw: unknown, env: DockEnv = {}): DockState {
  return fromWork(readWork(raw, env));
}

const sameLayout = (a: DockState, b: DockState) => JSON.stringify(a) === JSON.stringify(b);

const fail = (error: DockErrorKind, message: string): DockResult => ({ ok: false, error, message });

/** The groups of a zone of a layout, as lists of panels (an area's in reading order). */
export function zoneGroups(s: DockState, zone: Zone): string[][] {
  if (zone === 'floating') return s.floating.map((f) => f.panels);
  if (zone === 'strip') return s.strip;
  return (zone === 'column' ? s.column : s[zone]) ?? [];
}

/** Where a panel is now: the zone, the group's number and its tab number; null when it is not in the layout. */
export function whereIs(s: DockState, id: string): { zone: Zone; group: number; index: number } | null {
  for (const zone of ['strip', 'column', 'left', 'bottom'] as const) {
    const list = zoneGroups(s, zone);
    const g = list.findIndex((x) => x.includes(id));
    if (g >= 0) return { zone, group: g, index: list[g].indexOf(id) };
  }
  const f = s.floating.findIndex((x) => x.panels.includes(id));
  return f >= 0 ? { zone: 'floating', group: f, index: s.floating[f].panels.indexOf(id) } : null;
}

/**
 * Takes `panels` out of the working layout and puts them at `to`. The target is resolved against the layout as it is *before* the
 * panels leave (so "group 2" means what the person saw), then the panels come out, then they go in, then empty groups are dropped.
 */
function place(work: Work, panels: string[], to: DropTarget, env: DockEnv): DockResult | null {
  if (!isRec(to) || !(to.zone === 'strip' || to.zone === 'floating' || isArea(to.zone))) return fail('badTarget', 'no such zone');
  const lead = panels[0];
  let into: G | null = null;
  let tab: number | undefined;
  let beside: { area: Area; target: G; edge: Edge } | null = null;
  let edgeOf: { area: Area; edge: Edge } | null = null;
  let before: { zone: 'strip' | Area; at: G | null } | null = null;
  let newFloat: { x: number; y: number; w?: number; h?: number; window?: OsWindow } | null = null;
  const edgeOk = (e: unknown): e is Edge => e === 'left' || e === 'right' || e === 'top' || e === 'bottom';
  if ('edge' in to) {
    if (!isArea(to.zone) || !edgeOk(to.edge)) return fail('badTarget', 'an edge target needs an area and left, right, top or bottom');
    if ('group' in to) {
      const g = groupsOf(work, to.zone)[to.group];
      if (!g) return fail('badTarget', `no ${to.zone} group ${to.group}`);
      beside = { area: to.zone, target: g, edge: to.edge };
    } else edgeOf = { area: to.zone, edge: to.edge };
  } else if ('group' in to) {
    const g = groupsOf(work, to.zone)[to.group];
    if (!g) return fail('badTarget', `no ${to.zone} group ${to.group}`);
    into = g;
    tab = to.index;
  } else if ('before' in to) {
    if ((to.zone as Zone) === 'floating') return fail('badTarget', 'floating panels have no order');
    const list = groupsOf(work, to.zone);
    if (!Number.isInteger(to.before) || to.before < 0 || to.before > list.length) return fail('badTarget', `no place ${to.before} in ${to.zone}`);
    before = { zone: to.zone, at: list[to.before] ?? null };
  } else {
    if (to.zone !== 'floating' || !Number.isFinite(to.x) || !Number.isFinite(to.y)) return fail('badTarget', 'a floating target needs x and y');
    if (to.window !== undefined && !readWindow(to.window)) return fail('invalid', 'window needs x and y and a width of 120 and a height of 80 at least');
    newFloat = to;
  }
  // the dragged panels' own settings that go with them, and where they were (a floating panel remembers it)
  const was = everyGroup(work).find((g) => g.panels.includes(lead));
  const from = was ? locate(work, was) : null;
  const moved: G = mkG(panels, lead);
  if (newFloat && from && from.zone !== 'floating') {
    const stay = was!.panels.find((id) => !panels.includes(id));
    moved.home = { zone: from.zone as 'strip' | Area, group: from.group, ...(stay ? { with: stay } : {}) };
  } else if (newFloat && was?.home) moved.home = was.home;
  // take the panels out
  for (const g of everyGroup(work)) {
    if (!g.panels.some((id) => panels.includes(id))) continue;
    const shown = g.active;
    g.panels = g.panels.filter((id) => !panels.includes(id));
    if (!g.panels.includes(shown)) g.active = g.panels[0] ?? '';
  }
  if (into && !into.panels.length && !(into === was)) return fail('badTarget', 'the target group went away');
  // put them in
  if (into) {
    const at = tab === undefined || tab > into.panels.length ? into.panels.length : Math.max(0, tab);
    into.panels.splice(at, 0, ...panels);
    into.active = lead;
  } else if (beside) {
    const dir: Dir = beside.edge === 'left' || beside.edge === 'right' ? 'row' : 'col';
    insertBeside(work, beside.area, beside.target, moved, dir, beside.edge === 'left' || beside.edge === 'top');
  } else if (edgeOf) {
    insertAtEdge(work, edgeOf.area, moved, edgeOf.edge);
  } else if (before) {
    if (before.zone === 'strip') {
      const list = work.strip;
      const at = before.at ? list.indexOf(before.at) : list.length;
      list.splice(at < 0 ? list.length : at, 0, moved);
    } else insertBefore(work, before.zone, before.at, moved);
  } else if (newFloat) {
    const size = { w: newFloat.w ?? env.size?.(lead)?.w ?? FLOAT_DEFAULT.w, h: newFloat.h ?? env.size?.(lead)?.h ?? FLOAT_DEFAULT.h };
    Object.assign(moved, { x: Math.max(0, Math.round(newFloat.x)), y: Math.max(0, Math.round(newFloat.y)), w: Math.max(120, Math.round(size.w)), h: Math.max(80, Math.round(size.h)) });
    const win = readWindow(newFloat.window);
    if (win) moved.win = win;
    work.floating.push(moved);
  }
  for (const a of AREAS) work.area[a] = prune(work.area[a]);
  return null;
}

/** A floating panel's place kept on a screen of this size, and no bigger than the screen. A move by hand may park it partly outside
 *  (its title strip still shows, as in Adobe's programs); `whole` (the window or the monitor got smaller) brings all of it back in. */
function keepOnScreen(g: { x: number; y: number; w: number; h: number }, view?: { w: number; h: number }, whole = false) {
  if (!view || !(view.w > 0) || !(view.h > 0)) return;
  g.w = Math.round(clamp(g.w, 120, Math.max(120, view.w)));
  g.h = Math.round(clamp(g.h, 80, Math.max(80, view.h - 32)));
  g.x = Math.round(clamp(g.x, 0, Math.max(0, view.w - (whole ? g.w : Math.min(g.w, 120)))));
  g.y = Math.round(clamp(g.y, 32, Math.max(32, view.h - (whole ? g.h : 40))));
}

/** A cascade place for a new floating panel (right of the middle, a little lower for each one already floating). */
function cascade(w: Work, size: { w: number; h: number }, view?: { w: number; h: number }) {
  const n = w.floating.length;
  const vw = view?.w ?? 1480;
  return { x: Math.max(80, vw - 420 - size.w - n * 24), y: 130 + n * 24 };
}

/** The result of a command on a layout. The layout in is never changed; `changed` says whether the one out differs. */
export function applyDock(state: DockState, cmd: DockCommand, env: DockEnv = {}): DockResult {
  const known = env.known ?? (() => true);
  const base = normalizeDock(state, env);
  const start = readWork(base, env);
  const wide = { left: breadth(start.area.left, 'left', env.hidden), column: breadth(start.area.column, 'column', env.hidden), bottom: breadth(start.area.bottom, 'bottom', env.hidden) };
  const done = (w: Work): DockResult => {
    for (const a of AREAS) w.area[a] = prune(w.area[a]);
    fitBreadth(w, wide, env.hidden);
    const next = fromWork(w);
    return { ok: true, state: next, changed: !sameLayout(next, base) };
  };
  const work = () => readWork(base, env);
  if (!isRec(cmd)) return fail('invalid', 'unknown command');
  switch (cmd.op) {
    case 'reset': {
      const next = normalizeDock(CLASSIC, env);
      return { ok: true, state: next, changed: !sameLayout(next, base) };
    }
    case 'collapseColumn': {
      if (typeof cmd.collapsed !== 'boolean') return fail('invalid', 'collapsed must be true or false');
      const w = work();
      w.columnCollapsed = cmd.collapsed;
      return done(w);
    }
    case 'collapseArea': {
      if (!isArea(cmd.zone)) return fail('badTarget', 'zone must be left, column or bottom');
      if (typeof cmd.collapsed !== 'boolean') return fail('invalid', 'collapsed must be true or false');
      const w = work();
      if (cmd.zone === 'column') w.columnCollapsed = cmd.collapsed; else w.collapsed[cmd.zone] = cmd.collapsed;
      return done(w);
    }
    case 'foldGroup': {
      const zone = cmd.zone ?? 'column';
      if (!isArea(zone)) return fail('badTarget', 'zone must be left, column or bottom');
      const w = work();
      const g = groupsOf(w, zone)[cmd.group];
      if (!g) return fail('badGroup', `no ${zone} group ${cmd.group}`);
      g.folded = cmd.folded === true;
      return done(w);
    }
    case 'maximizeGroup': {
      if (!isArea(cmd.zone)) return fail('badTarget', 'zone must be left, column or bottom');
      const w = work();
      const g = groupsOf(w, cmd.zone)[cmd.group];
      if (!g) return fail('badGroup', `no ${cmd.zone} group ${cmd.group}`);
      w.maximized = cmd.on === true ? g.panels[0] : (w.maximized && g.panels.includes(w.maximized) ? null : w.maximized);
      if (cmd.on === true) g.folded = false;
      return done(w);
    }
    case 'resizeSplit':
    case 'equalizeSplit': {
      if (!isArea(cmd.zone)) return fail('badTarget', 'zone must be left, column or bottom');
      const w = work();
      const path = Array.isArray(cmd.path) ? cmd.path : [];
      // the plain arrangement has its split too (groups one after another), though the layout names no tree for it
      const s = splitAt(w, cmd.zone, path);
      if (!s) return fail('badGroup', `no split at ${JSON.stringify(path)} in ${cmd.zone}`);
      if (cmd.op === 'equalizeSplit') s.sizes = equalShares(s.parts.length);
      else {
        if (!Array.isArray(cmd.sizes) || cmd.sizes.length !== s.parts.length || cmd.sizes.some((v) => !(typeof v === 'number' && Number.isFinite(v) && v >= 0))) return fail('invalid', `sizes must be ${s.parts.length} numbers`);
        let sizes = share(cmd.sizes);
        sizes = share(sizes.map((v) => Math.max(MIN_SHARE, v)));
        s.sizes = sizes;
      }
      return done(w);
    }
    case 'setAreaSize': {
      if (!isArea(cmd.zone)) return fail('badTarget', 'zone must be left, column or bottom');
      if (!(typeof cmd.size === 'number' && Number.isFinite(cmd.size))) return fail('invalid', 'size must be a number');
      const w = work();
      const b = AREA_SIZE[cmd.zone];
      let max = b.max;
      // the document keeps its room: the two side areas together leave CANVAS_MIN.w, the bottom leaves CANVAS_MIN.h
      if (cmd.view && cmd.view.w > 0 && cmd.view.h > 0) {
        if (cmd.zone === 'bottom') max = Math.min(max, cmd.view.h - CANVAS_MIN.h - 120);
        else { const other = cmd.zone === 'left' ? 'column' : 'left'; max = Math.min(max, cmd.view.w - CANVAS_MIN.w - (w.area[other] ? (w.size[other] ?? AREA_SIZE[other].def) : 0) - 160); }
      }
      w.size[cmd.zone] = Math.round(clamp(cmd.size, b.min, Math.max(b.min, max)));
      return done(w);
    }
    case 'setActive': {
      const at = whereIs(base, cmd.panel);
      if (!at) return known(cmd.panel) ? fail('notInLayout', `${cmd.panel} is not in the layout`) : fail('unknownPanel', `no panel ${cmd.panel}`);
      const w = work();
      groupsOf(w, at.zone)[at.group].active = cmd.panel;
      return done(w);
    }
    case 'openPanel': {
      if (!known(cmd.panel)) return fail('unknownPanel', `no panel ${cmd.panel}`);
      const at = whereIs(base, cmd.panel);
      const w = work();
      if (at) {
        const g = groupsOf(w, at.zone)[at.group];
        g.active = cmd.panel;
        if (at.zone === 'column') w.columnCollapsed = false;
        else if (at.zone === 'left' || at.zone === 'bottom') w.collapsed[at.zone] = false;
        if (isArea(at.zone)) g.folded = false;
        if (w.maximized && isArea(at.zone) && !g.panels.includes(w.maximized) && groupsOf(w, at.zone).some((x) => x.panels.includes(w.maximized as string))) w.maximized = null;
        return done(w);
      }
      const size = env.size?.(cmd.panel) ?? FLOAT_DEFAULT;
      const p = cascade(w, size, cmd.view);
      const r = place(w, [cmd.panel], { zone: 'floating', x: p.x, y: p.y, w: size.w, h: size.h }, env);
      return r ?? done(w);
    }
    case 'closePanel': {
      const at = whereIs(base, cmd.panel);
      if (!at) return known(cmd.panel) ? fail('notInLayout', `${cmd.panel} is not in the layout`) : fail('unknownPanel', `no panel ${cmd.panel}`);
      const w = work();
      const g = groupsOf(w, at.zone)[at.group];
      g.panels = g.panels.filter((id) => id !== cmd.panel);
      if (g.active === cmd.panel) g.active = g.panels[0] ?? '';
      if (w.maximized === cmd.panel) w.maximized = g.panels[0] ?? null;
      return done(w);
    }
    case 'closeGroup': {
      const w = work();
      if (!(cmd.zone === 'strip' || cmd.zone === 'floating' || isArea(cmd.zone))) return fail('badGroup', 'no such zone');
      const g = groupsOf(w, cmd.zone)[cmd.group];
      if (!g) return fail('badGroup', `no ${cmd.zone} group ${cmd.group}`);
      g.panels = [];
      return done(w);
    }
    case 'movePanel': {
      if (!known(cmd.panel)) return fail('unknownPanel', `no panel ${cmd.panel}`);
      const w = work();
      const r = place(w, [cmd.panel], cmd.to, env);
      return r ?? done(w);
    }
    case 'floatGroup':
    case 'moveGroup': {
      const zone: Zone = cmd.zone;
      if (!(zone === 'strip' || zone === 'floating' || isArea(zone))) return fail('badGroup', 'no such zone');
      const w = work();
      const mine = groupsOf(w, zone)[cmd.group];
      if (!mine) return fail('badGroup', `no ${zone} group ${cmd.group}`);
      let to: DropTarget;
      if (cmd.op === 'floatGroup') {
        if (zone === 'floating') return done(w);
        const size = env.size?.(mine.panels[0]) ?? FLOAT_DEFAULT;
        const p = Number.isFinite(cmd.x) && Number.isFinite(cmd.y) ? { x: cmd.x as number, y: cmd.y as number } : cascade(w, size);
        to = { zone: 'floating', x: p.x, y: p.y, w: size.w, h: size.h };
      } else to = cmd.to;
      const panels = [...mine.panels];
      // a group dropped into a group of its own is nothing to do
      if (isRec(to) && 'group' in to && !('edge' in to) && groupsOf(w, to.zone)[to.group] === mine) return done(w);
      const keepActive = mine.active;
      const wasFolded = mine.folded;
      const r = place(w, panels, to, env);
      if (r) return r;
      // the group's tab that showed still shows
      const at = everyGroup(w).find((x) => x.panels.includes(panels[0]));
      if (at && at.panels.includes(keepActive)) at.active = keepActive;
      // an area group that moves as it is (not joining another) stays folded
      if (at && at.panels.length === panels.length && AREAS.some((a) => dfs(w.area[a]).includes(at))) at.folded = wasFolded;
      return done(w);
    }
    case 'moveFloating': {
      const w = work();
      const g = w.floating[cmd.group];
      if (!g) return fail('badGroup', `no floating group ${cmd.group}`);
      if (!Number.isFinite(cmd.x) || !Number.isFinite(cmd.y)) return fail('invalid', 'x and y must be numbers');
      g.x = Math.max(0, Math.round(cmd.x)); g.y = Math.max(0, Math.round(cmd.y));
      if (cmd.w !== undefined && Number.isFinite(cmd.w)) g.w = Math.max(120, Math.round(cmd.w));
      if (cmd.h !== undefined && Number.isFinite(cmd.h)) g.h = Math.max(80, Math.round(cmd.h));
      keepOnScreen(g, cmd.view);
      return done(w);
    }
    case 'fitFloating': {
      if (!cmd.view || !(cmd.view.w > 0) || !(cmd.view.h > 0)) return fail('invalid', 'view needs a width and a height');
      const w = work();
      for (const g of w.floating) keepOnScreen(g, cmd.view, true);
      return done(w);
    }
    case 'dockFloating': {
      const w = work();
      const g = w.floating[cmd.group];
      if (!g) return fail('badGroup', `no floating group ${cmd.group}`);
      const home = g.home ?? { zone: 'column' as const, group: groupsOf(w, 'column').length };
      const list = groupsOf(w, home.zone);
      const mate = home.with ? list.findIndex((x) => x.panels.includes(home.with as string)) : -1;
      const to: DropTarget = mate >= 0 ? { zone: home.zone, group: mate } : { zone: home.zone, before: Math.min(home.group, list.length) };
      // back home it shows again: its area unfolds
      if (home.zone === 'column') w.columnCollapsed = false; else if (home.zone !== 'strip') w.collapsed[home.zone] = false;
      const r = applyDock(fromWork(w), { op: 'moveGroup', zone: 'floating', group: cmd.group, to }, env);
      return r.ok ? { ...r, changed: !sameLayout(r.state, base) } : r;
    }
    case 'popOut': {
      if (!(cmd.zone === 'strip' || cmd.zone === 'floating' || isArea(cmd.zone))) return fail('badGroup', 'no such zone');
      if (cmd.window !== undefined && !readWindow(cmd.window)) return fail('invalid', 'window needs x and y and a width of 120 and a height of 80 at least');
      let s = base;
      let group = cmd.group;
      // a docked group floats first (it remembers where it was docked: closing its window puts it back there)
      if (cmd.zone !== 'floating') {
        const lead = zoneGroups(base, cmd.zone)[cmd.group]?.[0];
        if (!lead) return fail('badGroup', `no ${cmd.zone} group ${cmd.group}`);
        const r = applyDock(base, { op: 'floatGroup', zone: cmd.zone, group: cmd.group }, env);
        if (!r.ok) return r;
        s = r.state;
        group = s.floating.findIndex((f) => f.panels.includes(lead));
      }
      const w = readWork(s, env);
      const g = w.floating[group];
      if (!g) return fail('badGroup', `no floating group ${cmd.group}`);
      g.win = readWindow(cmd.window) ?? { x: g.x, y: g.y, w: g.w, h: g.h };
      const r = done(w);
      return r.ok ? { ...r, changed: !sameLayout(r.state, base) } : r;
    }
    case 'popIn':
    case 'moveWindow': {
      const w = work();
      const g = w.floating[cmd.group];
      if (!g) return fail('badGroup', `no floating group ${cmd.group}`);
      if (cmd.op === 'popIn') { delete g.win; return done(w); }
      if (!g.win) return fail('notInLayout', `floating group ${cmd.group} has no window of its own`);
      const win = readWindow(cmd.window);
      if (!win) return fail('invalid', 'window needs x and y and a width of 120 and a height of 80 at least');
      g.win = win;
      return done(w);
    }
    default:
      return fail('invalid', 'unknown command');
  }
}

// ---------------------------------------------------------------------------------------------------------------------------
// the layout's own undo (uix-d3): apart from the document's (Ctrl+Z never moves a panel back while a person edits text, as in the
// commercial programs), for this session only, 50 steps

/** How many layout changes can be undone. */
export const LAYOUT_HISTORY = 50;
/** The layouts before (`past`, the newest last) and after (`future`, the next redo last) the one in use. An entry is a layout and
 *  what goes with it (a workspace change also brings back the workspace's name, its tools and control panel and its arrangement). */
export type History<E> = { past: E[]; future: E[] };
export const emptyHistory = <E,>(): History<E> => ({ past: [], future: [] });

/** A change is made: `before` (what was in use) can be undone; what was undone can no longer be redone. */
export function historyRecord<E>(h: History<E>, before: E, limit = LAYOUT_HISTORY): History<E> {
  return { past: [...h.past, before].slice(-Math.max(1, limit)), future: [] };
}
/** Undo: the entry to bring back, and the history with `now` to redo (null: nothing to undo). */
export function historyUndo<E>(h: History<E>, now: E): { to: E; history: History<E> } | null {
  if (!h.past.length) return null;
  return { to: h.past[h.past.length - 1], history: { past: h.past.slice(0, -1), future: [...h.future, now] } };
}
/** Redo: the entry to bring back, and the history with `now` to undo again (null: nothing to redo). */
export function historyRedo<E>(h: History<E>, now: E): { to: E; history: History<E> } | null {
  if (!h.future.length) return null;
  return { to: h.future[h.future.length - 1], history: { past: [...h.past, now], future: h.future.slice(0, -1) } };
}

/** A layout brought back by an undo, its OS windows where they are now: a person moves those windows by hand (the system's title bar),
 *  and an undo of a dock change must not throw them back (a window that the undone layout has and the one in use does not, opens
 *  where it was then). A window is the same window when its group has the same first panel. */
export function keepWindows<P extends string>(back: DockState<P>, now: DockState<P>): DockState<P> {
  const at = new Map(now.floating.filter((f) => f.window).map((f) => [f.panels[0], f.window as OsWindow]));
  if (!at.size || !back.floating.some((f) => f.window && at.has(f.panels[0]))) return back;
  return { ...back, floating: back.floating.map((f) => (f.window && at.has(f.panels[0]) ? { ...f, window: { ...(at.get(f.panels[0]) as OsWindow) } } : f)) };
}

/** The layout as a list a person or a tool can read: one row per panel. */
export function describeDock(s: DockState): { panel: string; zone: Zone; group: number; index: number; shown: boolean; window?: boolean }[] {
  const out: { panel: string; zone: Zone; group: number; index: number; shown: boolean; window?: boolean }[] = [];
  for (const zone of ['strip', 'column', 'left', 'bottom'] as const) {
    zoneGroups(s, zone).forEach((g, gi) => g.forEach((panel, index) => out.push({ panel, zone, group: gi, index, shown: (s.active[g[0]] ?? g[0]) === panel })));
  }
  s.floating.forEach((f, gi) => f.panels.forEach((panel, index) => out.push({ panel, zone: 'floating', group: gi, index, shown: f.active === panel, ...(f.window ? { window: true } : {}) })));
  return out;
}

/** An area's arrangement as a tree the screen draws: splits with their shares and the path to each, and the area's groups by number. */
export type ViewNode = { kind: 'split'; dir: Dir; sizes: number[]; path: number[]; parts: ViewNode[] } | { kind: 'group'; group: number };
export function areaTree(s: DockState, area: Area): ViewNode | null {
  const list = zoneGroups(s, area);
  if (!list.length) return null;
  const tree = s.layout?.[area];
  const build = (t: Split | number, path: number[]): ViewNode => (typeof t === 'number'
    ? { kind: 'group', group: t }
    : { kind: 'split', dir: t.dir, sizes: t.sizes, path, parts: t.parts.map((p, i) => build(p, [...path, i])) });
  if (tree) return build(tree, []);
  if (list.length === 1) return { kind: 'group', group: 0 };
  return { kind: 'split', dir: DEFAULT_DIR[area], sizes: equalShares(list.length), path: [], parts: list.map((_, i) => ({ kind: 'group', group: i })) };
}

/** An area's size now (pixels). */
export const areaSize = (s: DockState, area: Area) => s.size?.[area] ?? AREA_SIZE[area].def;

/**
 * A floating panel's rectangle pulled onto the edges near it (the window's and the other floating panels', within a few pixels),
 * as the panels of Adobe's programs and Visual Studio's tool windows do. Moving: the whole rectangle moves; resizing: only the
 * edges being dragged (`edges`) move.
 */
export function snapRect(r: { x: number; y: number; w: number; h: number }, others: { x: number; y: number; w: number; h: number }[], view: { w: number; h: number; top?: number }, edges?: { l?: boolean; r?: boolean; t?: boolean; b?: boolean }) {
  const out = { ...r };
  const xs = [0, view.w, ...others.flatMap((o) => [o.x, o.x + o.w])];
  const ys = [view.top ?? 0, view.h, ...others.flatMap((o) => [o.y, o.y + o.h])];
  const near = (v: number, list: number[]) => { let best: number | null = null; for (const c of list) if (Math.abs(c - v) <= SNAP && (best === null || Math.abs(c - v) < Math.abs(best - v))) best = c; return best; };
  if (!edges) {
    const l = near(r.x, xs), rr = near(r.x + r.w, xs);
    if (l !== null && (rr === null || Math.abs(l - r.x) <= Math.abs(rr - r.x - r.w))) out.x = l; else if (rr !== null) out.x = rr - r.w;
    const t = near(r.y, ys), b = near(r.y + r.h, ys);
    if (t !== null && (b === null || Math.abs(t - r.y) <= Math.abs(b - r.y - r.h))) out.y = t; else if (b !== null) out.y = b - r.h;
    return out;
  }
  if (edges.l) { const l = near(r.x, xs); if (l !== null) { out.w = r.w + (r.x - l); out.x = l; } }
  if (edges.r) { const v = near(r.x + r.w, xs); if (v !== null) out.w = v - out.x; }
  if (edges.t) { const t = near(r.y, ys); if (t !== null) { out.h = r.h + (r.y - t); out.y = t; } }
  if (edges.b) { const v = near(r.y + r.h, ys); if (v !== null) out.h = v - out.y; }
  return out;
}
