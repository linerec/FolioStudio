// Run with node next/app/scripts/test-windows.cjs; uses only the installed TypeScript compiler.
// Window > Arrange: the layouts (ui/winlayout.ts) cover the document area without gaps or overlaps, and the window state functions
// (ui/windows.ts) keep their rules: an arrangement holds only for the number of windows it wants, floating and docking, closing.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

function load(file, deps = {}) {
  const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui', file), 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
  const m = {};
  vm.runInNewContext(js, { exports: m, require: (name) => deps[name] ?? {} });
  return m;
}
const eq = (a, b, msg) => assert.deepEqual(JSON.parse(JSON.stringify(a)), b, msg);

const layout = load('winlayout.ts');
const w = load('windows.ts', { './winlayout': layout });

// every arrangement with every number it fits: rects inside the unit square, no overlap, areas add up to 1
const EPS = 1e-9;
for (const id of Object.keys(layout.ARRANGEMENTS)) {
  const a = layout.ARRANGEMENTS[id];
  for (let n = 1; n <= 8; n++) {
    const fit = a.count === null ? n >= 2 : a.count === n;
    assert.equal(layout.fits(id, n), fit, `${id} fits ${n}`);
    const rs = layout.layoutRects(id, n);
    if (!fit) { assert.equal(rs, null, `${id} has no layout for ${n}`); continue; }
    assert.equal(rs.length, n, `${id} ${n} rects`);
    let sum = 0;
    for (const r of rs) {
      sum += r.w * r.h;
      assert.ok(r.x >= -EPS && r.y >= -EPS && r.x + r.w <= 1 + EPS && r.y + r.h <= 1 + EPS, `${id} ${n} inside`);
    }
    for (let i = 0; i < n; i++) for (let j = i + 1; j < n; j++) {
      const p = rs[i], q = rs[j];
      assert.ok(Math.min(p.x + p.w, q.x + q.w) - Math.max(p.x, q.x) < EPS || Math.min(p.y + p.h, q.y + q.h) - Math.max(p.y, q.y) < EPS, `${id} ${n} no overlap ${i},${j}`);
    }
    assert.ok(Math.abs(sum - 1) < 1e-6, `${id} ${n} covers the area: ${sum}`);
  }
}
// the shapes
eq(layout.layoutRects('2UpVertical', 2), [{ x: 0, y: 0, w: 0.5, h: 1 }, { x: 0.5, y: 0, w: 0.5, h: 1 }], 'two columns');
eq(layout.layoutRects('2UpHorizontal', 2), [{ x: 0, y: 0, w: 1, h: 0.5 }, { x: 0, y: 0.5, w: 1, h: 0.5 }], 'two rows');
eq(layout.layoutRects('3UpStacked', 3)[0], { x: 0, y: 0, w: 0.5, h: 1 }, 'the big window on the left');
eq(layout.layoutRects('4Up', 4)[3], { x: 0.5, y: 0.5, w: 0.5, h: 0.5 }, '2 by 2');
// the proportions Folio chose (winlayout.ts header): stacked is half and half, the small ones share the right half equally
eq(layout.layoutRects('4UpStacked', 4).slice(1), [{ x: 0.5, y: 0, w: 0.5, h: 1 / 3 }, { x: 0.5, y: 1 / 3, w: 0.5, h: 1 / 3 }, { x: 0.5, y: 2 / 3, w: 0.5, h: 1 / 3 }], 'stacked: the others share the right half');
eq(layout.layoutRects('4UpVertical', 4).map((r) => r.w), [0.25, 0.25, 0.25, 0.25], 'four equal columns');
eq(layout.layoutRects('tileAllHorizontally', 3).map((r) => r.h), [1 / 3, 1 / 3, 1 / 3], 'three equal rows');
eq(layout.layoutRects('6Up', 6)[5], { x: 2 / 3, y: 0.5, w: 1 / 3, h: 0.5 }, '3 by 2');
eq(layout.layoutRects('tile', 3).map((r) => r.w), [0.5, 0.5, 1], 'Tile with 3: a last row of one takes the whole width');

// cascade: a step of 32, and back to the corner when the window would leave the area
const area = { w: 1000, h: 700 };
const c0 = layout.cascadeRect(0, area), c1 = layout.cascadeRect(1, area);
assert.equal(c1.x - c0.x, 32);
assert.equal(c1.y - c0.y, 32);
assert.equal(c1.w, c0.w);
for (let i = 0; i < 30; i++) { const r = layout.cascadeRect(i, area); assert.ok(r.x + r.w <= area.w && r.y + r.h <= area.h, `cascade ${i} stays inside`); }
const k = layout.keepInside({ x: -50, y: 5000, w: 90, h: 90 }, area);
assert.ok(k.x >= 0 && k.y <= area.h - 32 && k.w >= layout.FLOAT_MIN.w && k.h >= layout.FLOAT_MIN.h, 'keepInside');

// window state
let s = w.initialState('a');
assert.equal(s.wins.length, 1);
assert.equal(w.arrangement(s), null);
s = w.newWindow(s);
assert.equal(s.wins.length, 2);
assert.equal(s.active, 2, 'the new window is the active one');
assert.equal(w.shownId(s), 2, 'and the one shown');
s = w.dockAll(s, '2UpVertical');
assert.equal(w.arrangement(s), '2UpVertical');
s = w.newWindow(s);
assert.equal(w.arrangement(s), null, 'a third window ends a 2-up arrangement');
s = w.dockAll(s, '3UpStacked');
assert.equal(w.arrangement(s), '3UpStacked');
s = w.floatWindow(s, 3, area);
assert.equal(w.floating(s).length, 1);
assert.equal(w.docked(s).length, 2);
assert.equal(w.arrangement(s), null, 'floating a window ends the arrangement');
s = w.dockAll(s, null);
assert.equal(w.floating(s).length, 0, 'Consolidate docks all');
s = w.floatAll(s, area);
assert.equal(w.floating(s).length, 3);
eq(w.floating(s).map((x) => x.float.x), [12, 44, 76], 'Cascade steps');
s = w.moveFloat(s, 2, { x: -400, y: 9000, w: 500, h: 400 }, area);
assert.ok(w.floating(s).find((x) => x.id === 2).float.x >= 0, 'a floating window is kept inside');
s = w.closeWindow(s, 3);
assert.equal(s.wins.length, 2);
assert.ok(s.wins.some((x) => x.id === s.active), 'the active window is one that is left');
s = w.closeWindow(w.closeWindow(s, 2), 1);
assert.equal(s.wins.length, 1, 'the last window is not closed by this');

// split: the second pane can be the active one; joining the panes makes the first the active one again
let t = w.initialState('a');
t = w.toggleSplit(t, 1);
t = w.activate(t, 1, '1b');
assert.equal(t.activeKey, '1b');
t = w.toggleSplit(t, 1);
assert.equal(t.activeKey, '1');
assert.equal(w.paneKey(4, true), '4b');

console.log('windows: PASS');
