// Run with node next/app/scripts/test-pointsnap.cjs; uses only the installed TypeScript compiler.
// ui/pointsnap.ts (Smart Guides for one point: the Pen, a dragged anchor or handle) and ui/turned.ts ownDelta (a move on a turned spread).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const load = (name, deps = {}) => {
  const source = fs.readFileSync(path.join(__dirname, `../renderer/src/ui/${name}.ts`), 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
  const m = {};
  vm.runInNewContext(js, { exports: m, require: (r) => { if (deps[r]) return deps[r]; throw Error('no module ' + r); } });
  return m;
};
const sg = load('smartguides');
const ps = load('pointsnap', { './smartguides': sg });
const tn = load('turned');
const eq = (a, b, msg) => assert.deepEqual(JSON.parse(JSON.stringify(a)), b, msg);

// ---- turned.ts: a move as shown in the page's own space (the inverse of the quarter turn) ----
eq(tn.ownDelta(undefined, [3, 4]), [3, 4], 'upright');
// a 90 degree turn (clockwise): right on the screen is up in the page, down is right
eq(tn.ownDelta(90, [1, 0]), [0, -1], '90: right');
eq(tn.ownDelta(90, [0, 1]), [1, 0], '90: down');
eq(tn.ownDelta(180, [1, 2]), [-1, -2], '180');
eq(tn.ownDelta(270, [1, 0]), [0, 1], '270: right');
eq(tn.ownDelta(270, [0, 1]), [-1, 0], '270: down');
// the same map as ownPoint: two points as shown, their own-space difference
for (const turn of [90, 180, 270]) {
  const a = tn.ownPoint(turn, 300, 400, [40, 60]), b = tn.ownPoint(turn, 300, 400, [47, 51]);
  eq(tn.ownDelta(turn, [7, -9]), [b[0] - a[0], b[1] - a[1]], `ownDelta = ownPoint difference at ${turn}`);
}
// shownPoint takes ownPoint back (a drop on another spread puts the middle of the objects where the screen shows it)
for (const turn of [undefined, 90, 180, 270]) {
  eq(tn.shownPoint(turn, 300, 400, tn.ownPoint(turn, 300, 400, [40, 60])), [40, 60], `shownPoint(ownPoint) at ${turn}`);
  eq(tn.ownPoint(turn, 300, 400, tn.shownPoint(turn, 300, 400, [25, 333])), [25, 333], `ownPoint(shownPoint) at ${turn}`);
}

// ---- pointsnap.ts ----
const P = { enabled: true, center: true, edges: true, dimensions: true, spacing: true };
const item = (id, m, pts, extra = {}) => ({ id, visible: true, transform: m, paths: [{ open: true, points: pts.map(([x, y]) => [x, y, x, y, x, y]) }], children: [], ...extra });
const items = { items: [item('a', [1, 0, 0, 1, 0, 0], [[100, 100], [200, 150]]), item('b', [1, 0, 0, 1, 50, 0], [[300, 300]], { children: [item('c', [1, 0, 0, 1, 0, 0], [[10, 20]])] })] };
eq(ps.anchorPoints(items, null), [[100, 100], [200, 150], [350, 300], [10, 20]], 'the anchors of every path in page points, with their transform, members of a group too');
eq(ps.anchorPoints(items, { id: 'a', path: 0, indices: [1] }), [[100, 100], [350, 300], [10, 20]], 'the points being moved are no targets');
eq(ps.anchorPoints(items, { id: 'a', path: 0, indices: [] }), [[350, 300], [10, 20]], 'a whole path skipped');
eq(ps.anchorPoints({ items: [{ ...item('h', [1, 0, 0, 1, 0, 0], [[5, 5]]), visible: false }] }, null), [], 'a hidden path is no target');
eq(ps.anchorPoints(items, null, [{ items: { items: [item('d', [1, 0, 0, 1, 0, 0], [[1, 2]])] }, dx: 612, dy: 0 }]).slice(-1), [[613, 2]], 'the other page of the spread, at its place');

const page = { size: [612, 792], margins: [36, 36, 36, 36] };
// an anchor within the zone wins, exactly
let r = ps.smartPoint([102, 97], [[100, 100], [200, 150]], [], page, P, 4);
eq([r.pt, r.anchor, r.pulled], [[100, 100], [100, 100], [true, true]], 'caught by the anchor');
// out of the zone: no anchor
r = ps.smartPoint([106, 97], [[100, 100]], [], page, P, 4);
assert.equal(r.anchor, null, 'too far for the anchor');
// the object lines, each axis on its own: the left edge of a box (x 150) and its middle (y 225)
r = ps.smartPoint([152, 227], [], [[150, 200, 250, 250]], page, P, 4);
eq([r.pt, r.pulled], [[150, 225], [true, true]], 'edge and middle lines');
assert.ok(r.draw.lines.some((l) => l.axis === 'x' && l.at === 150) && r.draw.lines.some((l) => l.axis === 'y' && l.at === 225), 'drawn: ' + JSON.stringify(r.draw.lines));
// Align to Object Edges off: the edge no longer catches, the middle does
r = ps.smartPoint([152, 227], [], [[150, 200, 250, 250]], page, { ...P, edges: false }, 4);
eq([r.pt, r.pulled], [[152, 225], [false, true]], 'edges off');
// the page middle (center) and a margin (edges)
r = ps.smartPoint([307, 38], [], [], page, P, 4);
eq(r.pt, [306, 36], 'page middle and the top margin');
// Smart Guides off: nothing
r = ps.smartPoint([102, 97], [[100, 100]], [[150, 200, 250, 250]], page, { ...P, enabled: false }, 4);
eq([r.pt, r.anchor, r.pulled], [[102, 97], null, [false, false]], 'off');
console.log('PASS test-pointsnap');
