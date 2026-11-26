// Run with node next/app/scripts/test-pathgeom.cjs; uses only the installed TypeScript compiler.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/pathgeom.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const m = {};
vm.runInNewContext(js, { exports: m });
// arrays made inside the vm have another prototype: compare through JSON
const eq = (a, b, msg) => assert.deepEqual(JSON.parse(JSON.stringify(a)), b, msg);
const near = (a, b, msg) => assert.ok(a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) < 1e-9), `${msg}: ${a} vs ${b}`);

// the point classes InDesign tells apart (oracle/indesign/probe-point-types.jsx)
const pt = (l, r) => [200, 100, l[0], l[1], r[0], r[1]];
assert.equal(m.isSmooth(pt([190, 100], [210, 100])), true, 'opposite, equal');
assert.equal(m.isSmooth(pt([180, 100], [220, 100])), true, 'opposite, unequal');
assert.equal(m.isSmooth(pt([200, 100], [230, 100])), true, 'left handle with no length');
assert.equal(m.isSmooth(pt([170, 100], [200, 100])), true, 'right handle with no length');
assert.equal(m.isSmooth(pt([190, 100], [190, 100.0001])), false, 'same side');
assert.equal(m.isSmooth(pt([190, 90], [190, 110])), false, 'cusp');
assert.equal(m.isSmooth(pt([190, 90], [210, 100])), false, 'bent');
assert.equal(m.isSmooth(pt([190, 99.9], [210, 100])), false, 'nearly in line');

// moving anchors takes the handles along
const tri = [[100, 100, 100, 100, 100, 100], [200, 100, 170, 100, 230, 100], [300, 200, 300, 200, 300, 200]];
near(m.moveAnchors(tri, [1], [10, 25])[1], [210, 125, 180, 125, 240, 125], 'anchor');
near(m.moveAnchors(tri, [0, 2], [-15, 12.5])[2], [285, 212.5, 285, 212.5, 285, 212.5], 'anchors');
// a smooth point's other handle turns to stay in line at its own length (oracle path-move-handle-smooth: right handle by (30, 20))
const r = m.moveHandle(tri, 1, 'right', [260, 120])[1];
near([r[4], r[5]], [260, 120], 'moved handle');
const k = 30 / Math.hypot(60, 20);
near([r[2], r[3]], [200 - 60 * k, 100 - 20 * k], 'turned handle');
// a corner point's handles move on their own, and so do those of a smooth point with Alt
const corner = [[100, 100, 100, 100, 100, 100], [200, 100, 190, 80, 230, 110], [300, 200, 300, 200, 300, 200]];
near(m.moveHandle(corner, 1, 'right', [245, 125])[1], [200, 100, 190, 80, 245, 125], 'corner');
near(m.moveHandle(tri, 1, 'right', [260, 120], true)[1], [200, 100, 170, 100, 260, 120], 'independent');
// a handle with no length on the other side stays where it is
const oneSided = [[100, 100, 100, 100, 100, 100], [200, 100, 200, 100, 230, 100], [300, 200, 300, 200, 300, 200]];
near(m.moveHandle(oneSided, 1, 'right', [230, 120])[1], [200, 100, 200, 100, 230, 120], 'one-sided');
near(m.moveHandle(oneSided, 1, 'left', [180, 110])[1].slice(4), [200 + 30 * 20 / Math.hypot(20, 10), 100 - 30 * 10 / Math.hypot(20, 10)], 'right turned by a moved left handle');
// the SVG path: lines where handles are on their anchors, curves where not; closed paths end in Z
assert.equal(m.svgPath([[0, 0, 0, 0, 0, 0], [10, 0, 10, 0, 10, 0], [10, 10, 10, 10, 10, 10]], false, 2), 'M0.00,0.00L20.00,0.00L20.00,20.00L0.00,0.00Z');
assert.equal(m.svgPath([[0, 0, 0, 0, 5, 0], [10, 10, 5, 10, 10, 10]], true), 'M0.00,0.00C5.00,0.00 5.00,10.00 10.00,10.00');
// the place on a segment nearest to a point
const seg = (a, b) => m.nearestOnSegment(a, b, [150, 50]);
let n = seg([100, 100, 100, 100, 100, 100], [200, 100, 200, 100, 200, 100]);
assert.ok(Math.abs(n.t - 0.5) < 1e-6 && Math.abs(n.dist - 50) < 1e-9, 'a straight segment: ' + JSON.stringify(n));
// a curve through (150, 25) at t = 0.5: anchors (100,100) and (200,100), handles (100,0) and (200,0)
n = seg([100, 100, 100, 100, 100, 0], [200, 100, 200, 0, 200, 100]);
assert.ok(Math.abs(n.t - 0.5) < 1e-3 && Math.abs(n.dist - 25) < 1e-6, 'a curve: ' + JSON.stringify(n));
const line = [[0, 0, 0, 0, 0, 0], [10, 0, 10, 0, 10, 0]];
assert.ok(m.nearestOnSegment(line[0], line[1], [-5, 0]).t < 1e-6, 'before the start: the start');
assert.ok(1 - m.nearestOnSegment(line[0], line[1], [20, 3]).t < 1e-6, 'past the end: the end');
// a straight segment: t is the share of the length (the engine lerps the anchors there), not the pace of the cubic
n = m.nearestOnSegment([150, 600, 150, 600, 150, 600], [350, 600, 350, 600, 350, 600], [220, 603]);
assert.ok(Math.abs(n.t - 0.35) < 1e-12 && Math.abs(n.dist - 3) < 1e-9, 'straight segment share: ' + JSON.stringify(n));
// Shift: the eight directions, at the projected length
near(m.constrain45([0, 0], [10, 1]), [10, 0], 'nearly horizontal');
near(m.constrain45([0, 0], [1, -10]), [0, -10], 'nearly vertical');
near(m.constrain45([5, 5], [15, 14]), [5 + 9.5, 5 + 9.5], '45 degrees');
near(m.constrain45([0, 0], [-10, 2]), [-10, 0], 'to the left');
// a segment dragged: a straight one moves, a curve's grabbed place follows the pointer
const sa = [0, 0, 0, 0, 0, 0], sb = [100, 0, 100, 0, 100, 0];
let rs = m.reshapeSegment(sa, sb, 0.5, [0, 20]);
assert.ok(rs.move, 'a straight segment moves');
near(rs.a, [0, 20, 0, 20, 0, 20], 'moved a'); near(rs.b, [100, 20, 100, 20, 100, 20], 'moved b');
const ca = [0, 0, 0, 0, 30, -30], cb = [100, 0, 70, -30, 100, 0];
const at = (a, b, t) => { const u = 1 - t; return [0, 1].map((i) => u * u * u * a[i] + 3 * u * u * t * a[4 + i] + 3 * u * t * t * b[2 + i] + t * t * t * b[i]); };
for (const t of [0.1, 0.3, 0.5, 0.8]) {
  rs = m.reshapeSegment(ca, cb, t, [5, -12]);
  const before = at(ca, cb, t), after = at(rs.a, rs.b, t);
  assert.ok(!rs.move, 'a curve is reshaped');
  near(after, [before[0] + 5, before[1] - 12], 'the grabbed place follows at t=' + t);
  near(rs.a.slice(0, 2), [0, 0], 'anchors stay');
}
console.log('pathgeom: all checks PASS');

// the handles of the neighbours of a selected point that shape the segments next to it (screens-doc)
const curvy = [[100, 100, 100, 100, 130, 100], [200, 100, 170, 100, 230, 100], [300, 200, 270, 200, 300, 200], [150, 250, 150, 250, 150, 250]];
eq(m.neighborHandles(curvy, true, [1]), [{ index: 0, side: 'right' }, { index: 2, side: 'left' }], 'open path, middle point');
eq(m.neighborHandles(curvy, true, [0]), [{ index: 1, side: 'left' }], 'the first point has no point before it');
eq(m.neighborHandles(curvy, false, [0]), [{ index: 1, side: 'left' }], 'closed: the last point has no handle on its anchor');
eq(m.neighborHandles(curvy, true, [1, 2]), [{ index: 0, side: 'right' }], 'two selected points: only the outside neighbours');
eq(m.neighborHandles(curvy, true, [3]), [], 'a neighbour without a handle shows none');
