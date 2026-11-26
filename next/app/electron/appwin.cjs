'use strict';
// Window > Arrange across the documents: which app windows (one per document, each with an engine of its own) sit together in a
// group, in what order, which one shows, and how the group lays them out.
//
// Pure model, no Electron and no screen: electron/appwin-shell.cjs applies the result to the real windows, the window shows the tab
// strip, and a later MCP server can call the same commands (APPWIN_COMMANDS is the table its tool descriptions are made from).
// Every command returns a value, never throws: { ok: true, changed, state, ... } or { ok: false, error, message }; the state given in
// is never changed. Nothing here is saved with a project: a window layout belongs to the computer (principle 1), and the state holds
// only window ids, titles and rectangles, no path.
//
// Words: a "doc" is one app window (it holds one document, or the start screen). A "group" is the docs that share one frame
// rectangle: as TABS only the active one shows (all the others wait, hidden, at the same rectangle); in an ARRANGEMENT (tile, 2-up ...)
// every doc shows, each in its share of the frame. A doc alone in its group is a floating window. Cascade puts every doc in a group of
// its own, a step down and right of the last.

const STEP = 32;
/** The smallest size a floated or cascaded app window gets (the window itself can be made smaller by hand). */
const APP_MIN = { width: 640, height: 420 };

// --- the layouts: the same shares as renderer/src/ui/winlayout.ts (the windows of one document); test-appwin.cjs compares them ---
const columns = (n) => Array.from({ length: n }, (_, i) => ({ x: i / n, y: 0, w: 1 / n, h: 1 }));
const rows = (n) => Array.from({ length: n }, (_, i) => ({ x: 0, y: i / n, w: 1, h: 1 / n }));
const stacked = (n) => [
  { x: 0, y: 0, w: 0.5, h: 1 },
  ...Array.from({ length: n - 1 }, (_, i) => ({ x: 0.5, y: i / (n - 1), w: 0.5, h: 1 / (n - 1) })),
];
const grid = (n, cols) => {
  const lines = Math.ceil(n / cols);
  return Array.from({ length: n }, (_, i) => {
    const r = Math.floor(i / cols);
    const inRow = r === lines - 1 ? n - r * cols : cols;
    const c = i - r * cols;
    return { x: c / inRow, y: r / lines, w: 1 / inRow, h: 1 / lines };
  });
};
const square = (n) => grid(n, Math.ceil(Math.sqrt(n)));

/** The arrangements: the number of docs each wants (null: any number from 2) and the shares of the frame for that number. */
const ARRANGEMENTS = {
  tileAllVertically: { count: null, rects: columns },
  tileAllHorizontally: { count: null, rects: rows },
  '2UpHorizontal': { count: 2, rects: rows },
  '2UpVertical': { count: 2, rects: columns },
  '3UpHorizontal': { count: 3, rects: rows },
  '3UpVertical': { count: 3, rects: columns },
  '3UpStacked': { count: 3, rects: stacked },
  '4Up': { count: 4, rects: (n) => grid(n, 2) },
  '4UpVertical': { count: 4, rects: columns },
  '4UpHorizontal': { count: 4, rects: rows },
  '4UpStacked': { count: 4, rects: stacked },
  '5UpStacked': { count: 5, rects: stacked },
  '6Up': { count: 6, rects: (n) => grid(n, 3) },
  '6UpStacked': { count: 6, rects: stacked },
  tile: { count: null, rects: square },
};
const fits = (id, n) => { const a = ARRANGEMENTS[id]; return !!a && (a.count === null ? n >= 2 : a.count === n); };
const layoutRects = (id, n) => (fits(id, n) ? ARRANGEMENTS[id].rects(n) : null);

/** The frame's share `r` (fractions) in pixels; neighbouring shares meet exactly (the edges are rounded, not the sizes). */
function cell(b, r) {
  const x0 = b.x + Math.round(r.x * b.width), x1 = b.x + Math.round((r.x + r.w) * b.width);
  const y0 = b.y + Math.round(r.y * b.height), y1 = b.y + Math.round((r.y + r.h) * b.height);
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/** Floating window number `i` of a cascade in `area` (a display's work area): 70% of it, a step down and right of the last. */
function cascadeBounds(i, area) {
  const width = Math.min(area.width, Math.max(APP_MIN.width, Math.round(area.width * 0.7)));
  const height = Math.min(area.height, Math.max(APP_MIN.height, Math.round(area.height * 0.7)));
  const room = Math.max(1, Math.floor(Math.min((area.width - width) / STEP, (area.height - height) / STEP)) + 1);
  const k = i % room;
  return { x: area.x + k * STEP, y: area.y + k * STEP, width, height };
}

/** The same rectangle moved so that it lies inside `area` (its size is cut down only when it is bigger than the area). */
function keepInside(b, area) {
  const width = Math.min(b.width, area.width), height = Math.min(b.height, area.height);
  return {
    width, height,
    x: Math.max(area.x, Math.min(b.x, area.x + area.width - width)),
    y: Math.max(area.y, Math.min(b.y, area.y + area.height - height)),
  };
}

// --- state -------------------------------------------------------------------------------------------------------------------
const initialState = () => ({ docs: {}, groups: [], nextGroup: 1 });
const clone = (s) => JSON.parse(JSON.stringify(s));
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
const rect = (b) => b && ['x', 'y', 'width', 'height'].every((k) => Number.isFinite(b[k])) && b.width > 0 && b.height > 0;
const groupOf = (s, doc) => s.groups.find((g) => g.docs.includes(doc));

/** Keeps the rules: no empty group, the active doc is a member, an arrangement holds only for the number of docs it wants. */
function normalize(s) {
  s.groups = s.groups.filter((g) => g.docs.length > 0);
  for (const g of s.groups) {
    if (!g.docs.includes(g.active)) g.active = g.docs[0];
    if (g.arrange && !fits(g.arrange, g.docs.length)) g.arrange = null;
    if (g.arrange) g.maximized = false;
  }
  return s;
}

const fail = (error, message) => ({ ok: false, error, message });
const done = (before, after, extra = {}) => ({ ok: true, changed: !same(before, after), state: after, ...extra });

/** The group the arrange-everything commands gather into: the caller's group (`focus`), else the group of the first doc. */
function gatherTarget(s, focus) {
  return (focus !== undefined && groupOf(s, focus)) || s.groups[0];
}

/** Moves every doc into `target` (its own docs first, then the others in group order); the other groups go away. */
function gather(s, target) {
  const rest = s.groups.filter((g) => g !== target);
  for (const g of rest) target.docs.push(...g.docs);
  s.groups = [target];
}

function newGroup(s, doc, bounds) {
  const g = { id: s.nextGroup++, docs: [doc], active: doc, arrange: null, maximized: false, bounds: { ...bounds } };
  s.groups.push(g);
  return g;
}

/** Takes a doc out of its group (the group goes if it was the last). */
function takeOut(s, doc) {
  const g = groupOf(s, doc);
  if (!g) return null;
  const at = g.docs.indexOf(doc);
  g.docs.splice(at, 1);
  // the tab to its right (else to its left) shows now, when the one taken out was the one showing
  if (g.active === doc) g.active = g.docs[Math.min(at, g.docs.length - 1)];
  return g;
}

const areaOf = (args) => (rect(args.area) ? args.area : null);
const needDoc = (s, args) => (Object.prototype.hasOwnProperty.call(s.docs, args.doc) ? null : fail('unknownDoc', `no app window ${args.doc}`));

/**
 * The commands. `kind` is for the tool descriptions (a query changes nothing); `args` lists the named arguments.
 * `internal` commands are the shell's own bookkeeping (a window made, closed, moved, retitled); the others are what a person (or an
 * LLM) asks for through Window > Arrange and the tab strip.
 */
const APPWIN_COMMANDS = {
  state: { kind: 'query', description: 'The whole window state: every app window (id, title, unsaved changes) and the groups they sit in.', args: {} },
  layout: { kind: 'query', description: 'Where every app window is now: its group, rectangle on screen and whether it shows.', args: {}, shell: true },
  view: { kind: 'query', description: 'What the calling window is told: the tab strip of its group (id, title, unsaved mark, active), how the group is laid out, and the counts.', args: {}, shell: true },
  closeDoc: { kind: 'command', description: 'The close button of a tab: shows the app window `doc` (default: the caller) and asks it to close the way its own close button does; a document with unsaved changes asks to save before it closes.', args: { doc: 'app window id' }, shell: true },
  saveDoc: { kind: 'command', description: 'File > Save All (Ctrl+Alt+Shift+S): asks the app window `doc` to save its document the way its own Save does; a document that cannot be saved there (read-only) says so in its window.', args: { doc: 'app window id' }, shell: true },
  activate: { kind: 'command', description: 'Shows the app window `doc` (its tab) and makes it the active one of its group.', args: { doc: 'app window id' } },
  moveTab: { kind: 'command', description: 'Moves the tab of `doc` to place `index` of its group, or to the group `group` (a group id; "new": a floating window of its own).', args: { doc: 'app window id', index: 'place in the group, from 0', group: 'group id, or "new"' } },
  consolidate: { kind: 'command', description: 'Window > Arrange > Consolidate All Windows: every app window becomes a tab of one window.', args: {} },
  arrange: { kind: 'command', description: 'Window > Arrange > a tiling (tile, 2-up, 3-up ... 6-up): every app window side by side in one frame. `how` is one of the ARRANGEMENTS ids; the number of windows must suit it.', args: { how: 'arrangement id' } },
  floatDoc: { kind: 'command', description: 'Window > Arrange > Float in Window: the app window `doc` leaves its tabs or tiles and floats as a window of its own (at `bounds` when given, else with its corner at `at`).', args: { doc: 'app window id', bounds: '{x,y,width,height} on screen (optional)', at: '{x,y} of its corner on screen (optional)' } },
  floatAll: { kind: 'command', description: 'Window > Arrange > Float All in Windows: every app window floats on its own.', args: {} },
  cascade: { kind: 'command', description: 'Window > Arrange > Cascade: every app window floats on its own, each a step down and right of the last.', args: {} },
  addDoc: { kind: 'command', internal: true, description: 'A new app window with `title` and rectangle `bounds`; it floats on its own.', args: { doc: 'app window id', title: 'tab label (null: start screen)', bounds: 'rectangle' } },
  updateDoc: { kind: 'command', internal: true, description: 'Another tab label and unsaved-changes mark for `doc`.', args: { doc: 'app window id', title: 'tab label', dirty: 'boolean' } },
  removeDoc: { kind: 'command', internal: true, description: 'The app window `doc` is gone.', args: { doc: 'app window id' } },
  setBounds: { kind: 'command', internal: true, description: 'The frame of the group of `doc` was moved or sized by hand (and maybe maximized).', args: { doc: 'app window id', bounds: 'rectangle', maximized: 'boolean' } },
};

const HANDLERS = {
  state(s) { return { ok: true, changed: false, state: s }; },

  addDoc(s, a) {
    if (!rect(a.bounds)) return fail('invalid', 'addDoc needs bounds {x,y,width,height}');
    if (Object.prototype.hasOwnProperty.call(s.docs, a.doc)) return fail('invalid', `app window ${a.doc} is already there`);
    s.docs[a.doc] = { id: a.doc, title: typeof a.title === 'string' ? a.title : null, dirty: !!a.dirty };
    newGroup(s, a.doc, a.bounds);
    return null;
  },
  updateDoc(s, a) {
    const bad = needDoc(s, a); if (bad) return bad;
    const d = s.docs[a.doc];
    if (a.title !== undefined) d.title = typeof a.title === 'string' ? a.title : null;
    if (a.dirty !== undefined) d.dirty = !!a.dirty;
    return null;
  },
  removeDoc(s, a) {
    const bad = needDoc(s, a); if (bad) return bad;
    takeOut(s, a.doc);
    delete s.docs[a.doc];
    return null;
  },
  setBounds(s, a) {
    const bad = needDoc(s, a); if (bad) return bad;
    if (a.bounds !== undefined && !rect(a.bounds)) return fail('invalid', 'setBounds needs bounds {x,y,width,height}');
    const g = groupOf(s, a.doc);
    if (a.bounds) g.bounds = { ...a.bounds };
    if (a.maximized !== undefined && !g.arrange) g.maximized = !!a.maximized;
    return null;
  },

  activate(s, a) {
    const bad = needDoc(s, a); if (bad) return bad;
    groupOf(s, a.doc).active = a.doc;
    return null;
  },

  moveTab(s, a) {
    const bad = needDoc(s, a); if (bad) return bad;
    const from = groupOf(s, a.doc);
    if (a.group === undefined || a.group === from.id) {
      if (!Number.isInteger(a.index)) return fail('invalid', 'moveTab needs an index');
      from.docs.splice(from.docs.indexOf(a.doc), 1);
      from.docs.splice(Math.max(0, Math.min(a.index, from.docs.length)), 0, a.doc);
      return null;
    }
    if (a.group === 'new') {
      const area = areaOf(a);
      if (from.docs.length === 1) return null; // already a window of its own
      const b = a.bounds && rect(a.bounds) ? a.bounds : cascadeBounds(s.groups.length, area || { x: 0, y: 0, ...APP_MIN });
      takeOut(s, a.doc);
      newGroup(s, a.doc, area ? keepInside(b, area) : b);
      return null;
    }
    const to = s.groups.find((g) => g.id === a.group);
    if (!to) return fail('unknownGroup', `no group ${a.group}`);
    takeOut(s, a.doc);
    to.docs.splice(Math.max(0, Math.min(Number.isInteger(a.index) ? a.index : to.docs.length, to.docs.length)), 0, a.doc);
    to.active = a.doc;
    return null;
  },

  consolidate(s, a) {
    if (!s.groups.length) return null;
    const target = gatherTarget(s, a.focus);
    gather(s, target);
    target.arrange = null;
    return null;
  },

  arrange(s, a) {
    if (!ARRANGEMENTS[a.how]) return fail('badArrangement', `unknown arrangement ${a.how}`);
    const n = Object.keys(s.docs).length;
    if (n < 2) return fail('notEnoughDocs', 'an arrangement needs at least two app windows');
    if (!fits(a.how, n)) return fail('doesNotFit', `${a.how} wants ${ARRANGEMENTS[a.how].count} app windows, ${n} are open`);
    const target = gatherTarget(s, a.focus);
    gather(s, target);
    target.arrange = a.how;
    target.maximized = false;
    return null;
  },

  floatDoc(s, a) {
    const bad = needDoc(s, a); if (bad) return bad;
    const g = groupOf(s, a.doc);
    if (g.docs.length === 1) { if (a.bounds && rect(a.bounds)) g.bounds = { ...a.bounds }; return null; } // already floating
    const area = areaOf(a);
    if (!area && !(a.bounds && rect(a.bounds))) return fail('noArea', 'floatDoc needs the display area (or bounds)');
    const at = a.at && Number.isFinite(a.at.x) && Number.isFinite(a.at.y) ? a.at : null;
    const own = a.bounds && rect(a.bounds) ? a.bounds : at && area ? { ...cascadeBounds(0, area), x: at.x, y: at.y } : cascadeBounds(s.groups.length, area);
    takeOut(s, a.doc);
    newGroup(s, a.doc, area ? keepInside(own, area) : own);
    return null;
  },

  floatAll(s, a) {
    const area = areaOf(a);
    if (!area) return fail('noArea', 'floatAll needs the display area');
    const docs = s.groups.flatMap((g) => g.docs);
    const before = new Map(s.groups.map((g) => [g.docs[0], g]));
    s.groups = [];
    docs.forEach((d, i) => {
      const old = before.get(d);
      // a window that already floated keeps its place; the others step out from the corner
      newGroup(s, d, old && old.docs.length === 1 ? old.bounds : cascadeBounds(i, area));
    });
    return null;
  },

  cascade(s, a) {
    const area = areaOf(a);
    if (!area) return fail('noArea', 'cascade needs the display area');
    const docs = s.groups.flatMap((g) => g.docs);
    s.groups = [];
    docs.forEach((d, i) => newGroup(s, d, cascadeBounds(i, area)));
    return null;
  },
};

/** Runs one command on `state`. The state given in is never changed; the new one is in the result. */
function apply(state, command) {
  if (!command || typeof command !== 'object' || typeof command.cmd !== 'string') return fail('invalid', 'a command is an object with a cmd name');
  const handler = Object.prototype.hasOwnProperty.call(HANDLERS, command.cmd) ? HANDLERS[command.cmd] : null;
  if (!handler) return fail('unknownCommand', `unknown window command ${command.cmd}`);
  const next = clone(state);
  const err = handler(next, command);
  if (err) return err;
  normalize(next);
  return done(state, next);
}

/**
 * Where every doc is: [{ doc, group, bounds, visible, active }] in group order, then tab order. Tabs: all at the group's frame, the
 * active one visible. An arrangement: each in its share of the frame, all visible.
 */
function placements(state) {
  const out = [];
  for (const g of state.groups) {
    const shares = g.arrange ? layoutRects(g.arrange, g.docs.length) : null;
    g.docs.forEach((doc, i) => out.push({
      doc, group: g.id,
      bounds: shares ? cell(g.bounds, shares[i]) : { ...g.bounds },
      visible: shares ? true : doc === g.active,
      active: doc === g.active,
      maximized: !shares && !!g.maximized,
    }));
  }
  return out;
}

/** What the window of `doc` is told: its own group's tab strip and how many windows there are. */
function view(state, doc) {
  const g = groupOf(state, doc);
  if (!g) return null;
  return {
    doc,
    group: g.id,
    mode: g.arrange || 'tabs',
    tabs: g.docs.map((id) => ({ id, title: state.docs[id].title, dirty: state.docs[id].dirty, active: id === g.active })),
    docs: Object.keys(state.docs).length,
    groups: state.groups.length,
  };
}

module.exports = { APPWIN_COMMANDS, ARRANGEMENTS, STEP, APP_MIN, initialState, apply, placements, view, fits, layoutRects, cascadeBounds, keepInside, cell, groupOf };
