// Run with node next/app/scripts/test-appwin.cjs; uses only the installed TypeScript compiler.
// Window > Arrange across documents (electron/appwin.cjs): the pure model of which app windows sit together as tabs or tiles. The
// layouts must be the same shares as the ones the windows of one document use (ui/winlayout.ts); the commands keep their rules.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const model = require('../electron/appwin.cjs');

const eq = (a, b, msg) => assert.deepEqual(JSON.parse(JSON.stringify(a)), JSON.parse(JSON.stringify(b)), msg);

// --- the layouts agree with ui/winlayout.ts, for every arrangement and number --------------------------------------------------
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/winlayout.ts'), 'utf8');
const layoutTs = {};
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: layoutTs });
eq(Object.keys(model.ARRANGEMENTS), Object.keys(layoutTs.ARRANGEMENTS), 'the same arrangements');
for (const id of Object.keys(layoutTs.ARRANGEMENTS)) {
  for (let n = 0; n <= 8; n++) {
    assert.equal(model.fits(id, n), layoutTs.fits(id, n), `${id} fits ${n}`);
    eq(model.layoutRects(id, n), layoutTs.layoutRects(id, n), `${id} ${n} windows: same shares`);
  }
}

// --- helpers -------------------------------------------------------------------------------------------------------------------
const AREA = { x: 0, y: 0, width: 1920, height: 1040 };
const B = { x: 100, y: 80, width: 1480, height: 940 };
const run = (s, cmd) => { const r = model.apply(s, cmd); assert.ok(r.ok, `${cmd.cmd}: ${JSON.stringify(r)}`); return r; };
function open(n) {
  let s = model.initialState();
  for (let i = 1; i <= n; i++) s = run(s, { cmd: 'addDoc', doc: i, title: `doc${i}.folio`, bounds: { ...B, x: B.x + i * 10 } }).state;
  return s;
}
const ids = (s) => s.groups.map((g) => g.docs);

// --- a new window floats on its own; the state is never changed in place ---------------------------------------------------------
let s = open(3);
eq(ids(s), [[1], [2], [3]], 'each new window is a group of its own');
const frozen = JSON.stringify(s);
run(s, { cmd: 'consolidate' });
assert.equal(JSON.stringify(s), frozen, 'the state given in is not changed');

// --- consolidate: everything becomes tabs of one window ---------------------------------------------------------------------------
let r = run(s, { cmd: 'consolidate', focus: 2 });
eq(ids(r.state), [[2, 1, 3]], 'the caller\'s window first, the others after it');
assert.equal(r.changed, true);
assert.equal(r.state.groups[0].active, 2);
eq(r.state.groups[0].bounds, { ...B, x: B.x + 20 }, 'the frame is the caller\'s');
let ps = model.placements(r.state);
eq(ps.map((p) => p.visible), [true, false, false], 'as tabs only the active one shows');
for (const p of ps) eq(p.bounds, r.state.groups[0].bounds, 'every tab sits at the frame');
assert.equal(run(r.state, { cmd: 'consolidate' }).changed, false, 'consolidating again changes nothing');
let tabs = r.state;

// --- activate and reorder -----------------------------------------------------------------------------------------------------------
r = run(tabs, { cmd: 'activate', doc: 3 });
assert.equal(r.state.groups[0].active, 3);
eq(model.placements(r.state).map((p) => [p.doc, p.visible]), [[2, false], [1, false], [3, true]]);
r = run(tabs, { cmd: 'moveTab', doc: 3, index: 0 });
eq(ids(r.state), [[3, 2, 1]], 'a tab moved to the front');
r = run(tabs, { cmd: 'moveTab', doc: 2, index: 99 });
eq(ids(r.state), [[1, 3, 2]], 'an index past the end goes last');
assert.equal(model.apply(tabs, { cmd: 'moveTab', doc: 2 }).ok, false, 'moveTab needs an index');

// --- the view each window is told ---------------------------------------------------------------------------------------------------
let v = model.view(tabs, 1);
assert.equal(v.mode, 'tabs'); assert.equal(v.docs, 3); assert.equal(v.groups, 1);
eq(v.tabs.map((t) => [t.id, t.title, t.active]), [[2, 'doc2.folio', true], [1, 'doc1.folio', false], [3, 'doc3.folio', false]]);
const dirty = run(tabs, { cmd: 'updateDoc', doc: 1, dirty: true }).state;
assert.equal(model.view(dirty, 3).tabs.find((t) => t.id === 1).dirty, true, 'every window sees the unsaved mark of another');
assert.equal(model.view(tabs, 99), null);

// --- arrangements: every window shows, in its share of the frame, with no gap and no overlap ---------------------------------------------
for (const how of Object.keys(model.ARRANGEMENTS)) {
  for (let n = 1; n <= 8; n++) {
    const opened = open(n);
    const ok = model.fits(how, n);
    r = model.apply(opened, { cmd: 'arrange', how });
    if (!ok) { assert.equal(r.ok, false, `${how} ${n}`); assert.ok(['doesNotFit', 'notEnoughDocs'].includes(r.error)); continue; }
    assert.ok(r.ok, `${how} ${n}`);
    const g = r.state.groups[0];
    assert.equal(r.state.groups.length, 1); assert.equal(g.arrange, how); assert.equal(g.docs.length, n);
    ps = model.placements(r.state);
    assert.ok(ps.every((p) => p.visible), `${how} ${n}: all show`);
    let area = 0;
    for (const p of ps) {
      area += p.bounds.width * p.bounds.height;
      assert.ok(p.bounds.width > 0 && p.bounds.height > 0, `${how} ${n} size`);
      assert.ok(p.bounds.x >= g.bounds.x && p.bounds.y >= g.bounds.y && p.bounds.x + p.bounds.width <= g.bounds.x + g.bounds.width && p.bounds.y + p.bounds.height <= g.bounds.y + g.bounds.height, `${how} ${n} inside the frame`);
    }
    assert.equal(area, g.bounds.width * g.bounds.height, `${how} ${n}: the shares cover the frame exactly (no gap)`);
    for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
      const p = ps[i].bounds, q = ps[j].bounds;
      assert.ok(Math.min(p.x + p.width, q.x + q.width) <= Math.max(p.x, q.x) || Math.min(p.y + p.height, q.y + q.height) <= Math.max(p.y, q.y), `${how} ${n}: ${i} and ${j} do not overlap`);
    }
    assert.equal(model.view(r.state, 1).mode, how);
  }
}
let two = run(open(2), { cmd: 'arrange', how: 'tile' }).state;
eq(model.placements(two).map((p) => p.bounds), [{ x: 110, y: 80, width: 740, height: 940 }, { x: 850, y: 80, width: 740, height: 940 }], 'two windows side by side, halves of the frame');
assert.equal(model.apply(open(1), { cmd: 'arrange', how: 'tile' }).error, 'notEnoughDocs');
assert.equal(model.apply(open(2), { cmd: 'arrange', how: 'nonsense' }).error, 'badArrangement');
assert.equal(model.apply(open(3), { cmd: 'arrange', how: '2UpVertical' }).error, 'doesNotFit');

// --- an arrangement holds only for the number of windows it wants ---------------------------------------------------------------------
let four = run(open(4), { cmd: 'arrange', how: '4Up' }).state;
let loss = run(four, { cmd: 'removeDoc', doc: 4 }).state;
assert.equal(loss.groups[0].arrange, null, '4-up with three windows left falls back to tabs');
let tile4 = run(open(4), { cmd: 'arrange', how: 'tile' }).state;
assert.equal(run(tile4, { cmd: 'removeDoc', doc: 4 }).state.groups[0].arrange, 'tile', 'tile suits any number from two');
assert.equal(run(run(tile4, { cmd: 'removeDoc', doc: 4 }).state, { cmd: 'removeDoc', doc: 3 }).state.groups[0].arrange, 'tile');
assert.equal(run(run(run(tile4, { cmd: 'removeDoc', doc: 4 }).state, { cmd: 'removeDoc', doc: 3 }).state, { cmd: 'removeDoc', doc: 2 }).state.groups[0].arrange, null, 'one window left: no arrangement');

// --- float in window: a window leaves the tabs and floats ----------------------------------------------------------------------------
r = run(tabs, { cmd: 'floatDoc', doc: 1, area: AREA });
eq(ids(r.state), [[2, 3], [1]]);
assert.equal(r.state.groups[0].active, 2, 'the others keep their active tab');
assert.ok(model.placements(r.state).every((p) => p.visible === (p.doc === 2 || p.doc === 1)), 'the floating one shows beside the active tab');
let fb = r.state.groups[1].bounds;
assert.ok(fb.x >= AREA.x && fb.y >= AREA.y && fb.x + fb.width <= AREA.x + AREA.width && fb.y + fb.height <= AREA.y + AREA.height, 'floated inside the display');
r = run(tabs, { cmd: 'floatDoc', doc: 2, area: AREA });
eq(ids(r.state), [[1, 3], [2]]);
assert.equal(r.state.groups[0].active, 1, 'the tab after the one floated shows now');
r = run(tabs, { cmd: 'floatDoc', doc: 3, bounds: { x: 300, y: 200, width: 900, height: 600 } });
eq(r.state.groups[1].bounds, { x: 300, y: 200, width: 900, height: 600 }, 'a tab dragged out lands where it was dropped');
assert.equal(model.apply(tabs, { cmd: 'floatDoc', doc: 1 }).error, 'noArea');
assert.equal(model.apply(tabs, { cmd: 'floatDoc', doc: 77, area: AREA }).error, 'unknownDoc');
assert.equal(run(s, { cmd: 'floatDoc', doc: 1, area: AREA }).changed, false, 'a floating window floated again changes nothing');
// out of a tiling: the rest keep a tiling when it still suits
r = run(tile4, { cmd: 'floatDoc', doc: 4, area: AREA });
eq(ids(r.state), [[1, 2, 3], [4]]);
assert.equal(r.state.groups[0].arrange, 'tile');

// --- float all, cascade --------------------------------------------------------------------------------------------------------------
r = run(tabs, { cmd: 'floatAll', area: AREA });
eq(ids(r.state), [[2], [1], [3]], 'each window in a group of its own');
assert.ok(model.placements(r.state).every((p) => p.visible), 'all float and show');
r = run(tabs, { cmd: 'cascade', area: AREA });
ps = model.placements(r.state);
assert.equal(ps.length, 3);
for (let i = 1; i < 3; i++) {
  assert.equal(ps[i].bounds.x - ps[i - 1].bounds.x, model.STEP, 'a step to the right');
  assert.equal(ps[i].bounds.y - ps[i - 1].bounds.y, model.STEP, 'and a step down');
  assert.equal(ps[i].bounds.width, ps[0].bounds.width);
}
assert.equal(ps[0].bounds.width, Math.round(AREA.width * 0.7));
assert.equal(model.apply(tabs, { cmd: 'cascade' }).error, 'noArea');
// many windows: the steps start again at the corner when one more would not fit
const crowd = run(open(40), { cmd: 'cascade', area: AREA });
for (const p of model.placements(crowd.state)) assert.ok(p.bounds.x + p.bounds.width <= AREA.width && p.bounds.y + p.bounds.height <= AREA.height, 'every cascaded window is inside the display');
// consolidate undoes a cascade
eq(ids(run(crowd.state, { cmd: 'consolidate' }).state).map((g) => g.length), [40]);

// --- moving a tab between groups --------------------------------------------------------------------------------------------------------
const apart = run(tabs, { cmd: 'floatDoc', doc: 3, area: AREA }).state; // [[2,1],[3]]
const gid = apart.groups[0].id;
r = run(apart, { cmd: 'moveTab', doc: 3, group: gid, index: 1 });
eq(ids(r.state), [[2, 3, 1]], 'dragged back into the tabs at the place it was dropped');
assert.equal(r.state.groups[0].active, 3);
r = run(tabs, { cmd: 'moveTab', doc: 1, group: 'new', area: AREA });
eq(ids(r.state), [[2, 3], [1]]);
assert.equal(model.apply(tabs, { cmd: 'moveTab', doc: 1, group: 99, index: 0 }).error, 'unknownGroup');

// --- a window closes -----------------------------------------------------------------------------------------------------------------------
r = run(tabs, { cmd: 'removeDoc', doc: 2 });
eq(ids(r.state), [[1, 3]]);
assert.equal(r.state.groups[0].active, 1, 'the next tab shows when the active one closes');
r = run(run(tabs, { cmd: 'activate', doc: 3 }).state, { cmd: 'removeDoc', doc: 3 });
assert.equal(r.state.groups[0].active, 1, 'the one before it when it was the last');
eq(run(open(1), { cmd: 'removeDoc', doc: 1 }).state.groups, [], 'no window, no group');
assert.equal(model.apply(open(1), { cmd: 'removeDoc', doc: 5 }).error, 'unknownDoc');

// --- the frame follows a window moved or sized by hand --------------------------------------------------------------------------------------
r = run(tabs, { cmd: 'setBounds', doc: 1, bounds: { x: 5, y: 6, width: 700, height: 500 }, maximized: true });
eq(r.state.groups[0].bounds, { x: 5, y: 6, width: 700, height: 500 });
assert.equal(model.placements(r.state)[0].maximized, true);
assert.equal(model.placements(run(r.state, { cmd: 'arrange', how: 'tile' }).state)[0].maximized, false, 'a tiling is never maximized');
assert.equal(model.apply(tabs, { cmd: 'setBounds', doc: 1, bounds: { x: 0, y: 0, width: 0, height: 5 } }).error, 'invalid');

// --- bad commands answer with an error, they do not throw -----------------------------------------------------------------------------------
for (const bad of [null, 5, {}, { cmd: 7 }, { cmd: 'noSuch' }, { cmd: 'constructor' }, { cmd: 'activate', doc: 'x' }, { cmd: 'addDoc', doc: 9 }, { cmd: 'addDoc', doc: 1, bounds: B }]) {
  const out = model.apply(open(1), bad);
  assert.equal(out.ok, false, JSON.stringify(bad));
  assert.equal(typeof out.error, 'string'); assert.equal(typeof out.message, 'string');
}
// the command table covers every command the model runs (the tool descriptions are made from it)
for (const name of Object.keys(model.APPWIN_COMMANDS)) {
  const c = model.APPWIN_COMMANDS[name];
  assert.ok(c.kind === 'command' || c.kind === 'query', name);
  assert.ok(typeof c.description === 'string' && c.description.length > 10, name);
}
assert.equal(model.apply(open(1), { cmd: 'state' }).changed, false);
console.log('PASS test-appwin');
