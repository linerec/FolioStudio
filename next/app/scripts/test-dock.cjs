// Run with node next/app/scripts/test-dock.cjs; uses only the installed TypeScript compiler.
// Panel dock (ui/docklayout.ts): moving a panel or a group between groups and zones, floating, folding, closing, reset, and the repair of
// any saved layout. The commands are pure functions of the layout, so this is the whole logic of the dock without a screen.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/docklayout.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const m = {};
// the layout imports the panel list (next/shared/panels.json) as a default import
const spec = JSON.parse(fs.readFileSync(path.join(__dirname, '../../shared/panels.json'), 'utf8'));
vm.runInNewContext(js, { exports: m, require: (p) => (p.endsWith('panels.json') ? { default: spec } : {}) });
const { CLASSIC, applyDock, normalizeDock, describeDock, whereIs, DOCK_COMMANDS, areaTree, snapRect } = m;
const copy = (v) => JSON.parse(JSON.stringify(v));
const eq = (a, b, msg) => assert.deepEqual(copy(a), copy(b), msg);
const KNOWN = new Set(['pages', 'layers', 'links', 'stroke', 'color', 'swatches', 'project', 'info', 'align', 'character', 'history']);
const env = { known: (id) => KNOWN.has(id), size: (id) => ({ w: 250, h: id === 'info' ? 220 : 300 }) };
const ok = (r) => { assert.ok(r.ok, r.message); return r.state; };
const run = (s, cmd) => ok(applyDock(s, cmd, env));
const flat = (s) => [...s.strip.flat(), ...s.column.flat(), ...s.floating.flatMap((f) => f.panels)];
// every panel in exactly one place, every group has a tab showing, no empty groups
function sound(s, msg) {
  const all = flat(s);
  assert.equal(new Set(all).size, all.length, `${msg}: no panel twice`);
  for (const g of [...s.strip, ...s.column]) { assert.ok(g.length, `${msg}: no empty group`); assert.ok(g.includes(s.active[g[0]]), `${msg}: a tab shows in ${g[0]}`); }
  for (const f of s.floating) { assert.ok(f.panels.length && f.panels.includes(f.active), `${msg}: floating tab`); }
}

sound(CLASSIC, 'classic');
eq(describeDock(CLASSIC).filter((r) => r.zone === 'column'), [{ panel: 'project', zone: 'column', group: 0, index: 0, shown: true }], 'describe');

// a tab moves to another group of the same zone: it joins as the last tab (or at an index) and becomes the one showing
let s = run(CLASSIC, { op: 'movePanel', panel: 'swatches', to: { zone: 'strip', group: 1 } });
eq(s.strip, [['pages', 'layers', 'links'], ['stroke', 'color', 'swatches']], 'swatches joins stroke and color; its own group is gone');
assert.equal(s.active.stroke, 'swatches', 'the dropped tab shows');
sound(s, 'join');
s = run(s, { op: 'movePanel', panel: 'swatches', to: { zone: 'strip', group: 1, index: 0 } });
eq(s.strip[1], ['swatches', 'stroke', 'color'], 'to the front: the group is now named by swatches');
assert.equal(s.active.swatches, 'swatches');
assert.equal(s.active.stroke, undefined, 'the old name is gone');
sound(s, 'tab order');
// reorder the tabs inside one group
s = run(CLASSIC, { op: 'movePanel', panel: 'links', to: { zone: 'strip', group: 0, index: 0 } });
eq(s.strip[0], ['links', 'pages', 'layers'], 'links to the front of its own group');

// a panel leaves a group: the group's showing tab moves on when it was the one that left
s = run(CLASSIC, { op: 'setActive', panel: 'layers' });
assert.equal(s.active.pages, 'layers');
s = run(s, { op: 'movePanel', panel: 'layers', to: { zone: 'column', group: 0 } });
eq(s.strip[0], ['pages', 'links']);
assert.equal(s.active.pages, 'pages', 'the first panel shows after the showing one left');
eq(s.column, [['project', 'layers']]);
assert.equal(s.active.project, 'layers');
sound(s, 'strip to column');

// between groups: a panel becomes a group of its own at a place (the number of groups: the end)
s = run(CLASSIC, { op: 'movePanel', panel: 'color', to: { zone: 'strip', before: 0 } });
eq(s.strip, [['color'], ['pages', 'layers', 'links'], ['stroke'], ['swatches']], 'color in front of everything');
s = run(CLASSIC, { op: 'movePanel', panel: 'pages', to: { zone: 'column', before: 1 } });
eq(s.column, [['project'], ['pages']], 'a new group at the end of the column');
eq(s.strip[0], ['layers', 'links']);
// moving a group's only panel before itself or the next one changes nothing
s = run(CLASSIC, { op: 'movePanel', panel: 'swatches', to: { zone: 'strip', before: 2 } });
eq(s.strip, CLASSIC.strip, 'before itself: nothing changes');
let r = applyDock(CLASSIC, { op: 'movePanel', panel: 'swatches', to: { zone: 'strip', before: 3 } }, env);
assert.ok(r.ok && !r.changed, 'at the end where it already is: no change');

// reorder groups: the whole group moves
s = run(CLASSIC, { op: 'moveGroup', zone: 'strip', group: 2, to: { zone: 'strip', before: 0 } });
eq(s.strip, [['swatches'], ['pages', 'layers', 'links'], ['stroke', 'color']], 'swatches group first');
s = run(CLASSIC, { op: 'moveGroup', zone: 'strip', group: 0, to: { zone: 'strip', before: 3 } });
eq(s.strip, [['stroke', 'color'], ['swatches'], ['pages', 'layers', 'links']], 'the first group last');
assert.equal(s.active.pages, 'pages');
// a group goes to the other zone as one group, keeping its tabs and the one showing
s = run(run(CLASSIC, { op: 'setActive', panel: 'color' }), { op: 'moveGroup', zone: 'strip', group: 1, to: { zone: 'column', before: 0 } });
eq(s.column, [['stroke', 'color'], ['project']]);
assert.equal(s.active.stroke, 'color', 'the tab that showed still shows');
sound(s, 'group to column');
// a group joins another group: all its tabs, in order
s = run(CLASSIC, { op: 'moveGroup', zone: 'strip', group: 1, to: { zone: 'strip', group: 0 } });
eq(s.strip, [['pages', 'layers', 'links', 'stroke', 'color'], ['swatches']]);
r = applyDock(CLASSIC, { op: 'moveGroup', zone: 'strip', group: 1, to: { zone: 'strip', group: 1 } }, env);
assert.ok(r.ok && !r.changed, 'a group dropped on itself: nothing');

// floating: out of the dock to a place, with the panel's size; and into a floating panel; and back into the dock
s = run(CLASSIC, { op: 'movePanel', panel: 'links', to: { zone: 'floating', x: 300.4, y: 200 } });
eq(s.floating, [{ panels: ['links'], active: 'links', x: 300, y: 200, w: 250, h: 300, home: { zone: 'strip', group: 0, with: 'pages' } }], 'it remembers where it was docked');
eq(s.strip[0], ['pages', 'layers']);
s = run(s, { op: 'movePanel', panel: 'info', to: { zone: 'floating', x: 20, y: 30, w: 400, h: 100 } });
eq(s.floating[1], { panels: ['info'], active: 'info', x: 20, y: 30, w: 400, h: 100 });
s = run(s, { op: 'movePanel', panel: 'info', to: { zone: 'floating', group: 0 } });
eq(s.floating.map((f) => f.panels), [['links', 'info']], 'info joins the floating panel; its own is gone');
assert.equal(s.floating[0].active, 'info');
s = run(s, { op: 'moveGroup', zone: 'floating', group: 0, to: { zone: 'column', before: 0 } });
eq(s.column, [['links', 'info'], ['project']]);
assert.equal(s.floating.length, 0);
sound(s, 'floating round trip');
// a panel that is not in the layout (closed) comes in by a move
s = run(CLASSIC, { op: 'movePanel', panel: 'align', to: { zone: 'strip', group: 2 } });
eq(s.strip[2], ['swatches', 'align']);

// folding
s = run(CLASSIC, { op: 'collapseColumn', collapsed: true });
assert.equal(s.columnCollapsed, true);
s = run(run(CLASSIC, { op: 'movePanel', panel: 'align', to: { zone: 'column', before: 1 } }), { op: 'foldGroup', group: 1, folded: true });
assert.deepEqual(copy(s.folded), { align: true });
s = run(s, { op: 'moveGroup', zone: 'column', group: 1, to: { zone: 'column', before: 0 } });
eq(s.column, [['align'], ['project']]);
assert.deepEqual(copy(s.folded), { align: true }, 'a folded group stays folded when it moves');
r = applyDock(CLASSIC, { op: 'foldGroup', group: 5, folded: true }, env);
assert.ok(!r.ok && r.error === 'badGroup');

// close and reset
s = run(CLASSIC, { op: 'closePanel', panel: 'layers' });
eq(s.strip[0], ['pages', 'links']);
s = run(s, { op: 'closePanel', panel: 'swatches' });
eq(s.strip, [['pages', 'links'], ['stroke', 'color']], 'a group without panels is gone');
s = run(s, { op: 'reset' });
eq(s, normalizeDock(CLASSIC, env), 'reset is the default layout');
assert.ok(applyDock(CLASSIC, { op: 'reset' }, env).changed === false);

// errors are results, never exceptions, and leave the layout alone
for (const [cmd, kind] of [
  [{ op: 'movePanel', panel: 'nonesuch', to: { zone: 'strip', group: 0 } }, 'unknownPanel'],
  [{ op: 'movePanel', panel: 'info', to: { zone: 'strip', group: 9 } }, 'badTarget'],
  [{ op: 'movePanel', panel: 'info', to: { zone: 'column', before: 9 } }, 'badTarget'],
  [{ op: 'movePanel', panel: 'info', to: { zone: 'floating', x: NaN, y: 0 } }, 'badTarget'],
  [{ op: 'moveGroup', zone: 'strip', group: 7, to: { zone: 'strip', before: 0 } }, 'badGroup'],
  [{ op: 'setActive', panel: 'info' }, 'notInLayout'],
  [{ op: 'closePanel', panel: 'nonesuch' }, 'unknownPanel'],
  [{ op: 'bogus' }, 'invalid'],
]) {
  const res = applyDock(CLASSIC, cmd, env);
  assert.ok(!res.ok && res.error === kind, `${JSON.stringify(cmd)} -> ${res.ok ? 'ok' : res.error}`);
}
assert.equal(applyDock(CLASSIC, { op: 'movePanel', panel: 'info', to: { zone: 'strip', group: 0 } }, env).ok, true);
// the input is never changed
const frozen = copy(CLASSIC);
applyDock(CLASSIC, { op: 'movePanel', panel: 'pages', to: { zone: 'column', before: 0 } }, env);
eq(CLASSIC, frozen, 'the layout in is not changed');

// repair: anything saved becomes a layout that holds
eq(normalizeDock(null, env), { strip: [], column: [], active: {}, columnCollapsed: false, folded: {}, floating: [] }, 'nothing saved: empty (the app uses the default itself)');
s = normalizeDock({
  strip: [['pages', 'bogus', 'pages'], [], ['layers', 3, null]], column: [['links', 'layers']], active: { pages: 'gone', links: 'links' },
  columnCollapsed: 'yes', folded: { links: true, pages: true },
  floating: [{ panels: ['color'], active: 'stroke', x: -5, y: 'x' }, { panels: ['bogus'] }, 7],
}, env);
eq(s.strip, [['pages'], ['layers']], 'unknown and repeated panels and empty groups are dropped');
eq(s.column, [['links']], 'a panel is in one place: the first');
assert.equal(s.active.pages, 'pages');
assert.equal(s.columnCollapsed, false);
assert.deepEqual(copy(s.folded), { links: true }, 'only column groups fold');
eq(s.floating, [{ panels: ['color'], active: 'color', x: 0, y: 120, w: 250, h: 300 }], 'floating panels get a place and a size');
sound(s, 'repair');
// a layout from before folding existed (no `folded`) still reads
s = normalizeDock({ strip: CLASSIC.strip, column: CLASSIC.column, active: CLASSIC.active, columnCollapsed: true, floating: [] }, env);
assert.equal(s.columnCollapsed, true);
eq(s.strip, CLASSIC.strip);
assert.deepEqual(copy(whereIs(s, 'color')), { zone: 'strip', group: 1, index: 1 });
assert.equal(whereIs(s, 'info'), null);

// the command table lists every command the function knows
for (const op of ['movePanel', 'moveGroup', 'setActive', 'collapseColumn', 'foldGroup', 'closePanel', 'reset']) assert.ok(DOCK_COMMANDS.some((c) => c.op === op), `${op} is described`);
require('./test-docktree.cjs')(m, { env, run, ok, eq, copy, sound });

// uix-d3: a panel let go outside the app window: floating in an OS window of its own there (and remembering where it was docked)
s = run(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'floating', x: 300, y: 200, window: { x: 2100, y: 140, w: 260, h: 380 } } });
eq(s.floating, [{ panels: ['layers'], active: 'layers', x: 300, y: 200, w: 250, h: 300, home: { zone: 'strip', group: 0, with: 'pages' }, window: { x: 2100, y: 140, w: 260, h: 380 } }], 'out of the window: an OS window there');
sound(s, 'window target');
assert.equal(applyDock(CLASSIC, { op: 'movePanel', panel: 'layers', to: { zone: 'floating', x: 3, y: 3, window: { x: 0, y: 0, w: 10, h: 10 } } }, env).error, 'invalid', 'a window too small to use is refused');
// ... and a tab of that window dragged back onto a dock group: it joins, the window's group is gone (its window closes)
s = run(s, { op: 'movePanel', panel: 'layers', to: { zone: 'column', group: 0 } });
eq(s.floating, [], 'the window group is gone');
eq(s.column, [['project', 'layers']], 'the tab joined the project panel');
// the layout's own undo: a stack apart from the document's, 50 steps, a new change drops what was undone
const { historyRecord, historyUndo, historyRedo, emptyHistory, keepWindows, LAYOUT_HISTORY } = m;
let h = emptyHistory();
assert.equal(historyUndo(h, 'now'), null, 'nothing to undo');
h = historyRecord(h, 'a'); h = historyRecord(h, 'b');
let u = historyUndo(h, 'c');
assert.equal(u.to, 'b'); eq(u.history, { past: ['a'], future: ['c'] }, 'undo keeps the layout in use to redo');
const re = historyRedo(u.history, 'b');
assert.equal(re.to, 'c'); eq(re.history, { past: ['a', 'b'], future: [] }, 'redo');
eq(historyRecord(u.history, 'b2'), { past: ['a', 'b2'], future: [] }, 'a new change after an undo drops the redo');
for (let i = 0; i < 70; i++) h = historyRecord(h, i);
assert.equal(h.past.length, LAYOUT_HISTORY, 'fifty steps');
assert.equal(h.past[0], 20, 'the oldest go first');
// an undo keeps the OS windows where the person put them (the same window: the same first panel)
const before = run(CLASSIC, { op: 'popOut', zone: 'strip', group: 1, window: { x: 100, y: 100, w: 300, h: 300 } });
const now = run(before, { op: 'moveWindow', group: 0, window: { x: 1900, y: 400, w: 320, h: 360 } });
eq(keepWindows(before, now).floating[0].window, { x: 1900, y: 400, w: 320, h: 360 }, 'the window stays where it is now');
eq(keepWindows(before, CLASSIC).floating[0].window, { x: 100, y: 100, w: 300, h: 300 }, 'a window the layout in use has not opens where it was');
console.log('dock logic ok');
