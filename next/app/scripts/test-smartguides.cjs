// Run with node next/app/scripts/test-smartguides.cjs; uses only the installed TypeScript compiler.
// The vectors are InDesign 2025 results (real mouse, artifacts/smartguides-c2-20261103/out/*.rows.json, rules/smartguides-c2.ko.md):
// positions in page points, zone 2.085 pt (the Snap to Zone, 4 px, at fit-page zoom: 0.417 pt per device px).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const load = (f) => { const m = {}; vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(__dirname, '../renderer/src/ui', f), 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: m, Math, Array, Object }); return m; };
const sg = load('smartguides.ts');
const J = (v) => JSON.parse(JSON.stringify(v));
const near = (a, b, tol = 0.011) => assert.ok(Math.abs(a - b) <= tol, `${a} != ${b}`);
const zone = 2.085;
const P = sg.SMART_ALL;
const page = { size: [612, 792], margins: [36, 36, 36, 36] };
const S = [188.99, 303.34, 388.99, 403.34];               // x0 y0 x1 y1 (200 x 100)
const mv = (b, others, prefs = P, base = [0, 0]) => sg.smartMove(b, others, page, prefs, zone, base);
const box = (x, y, w = 100, h = 60) => [x, y, x + w, y + h];

// ---- alignment (s1_align): the top of M, offset by n device px from the top of S, ends on it
for (const [top, snapped] of [[303.394, true], [304.642, true], [305.059, true], [305.891, true], [306.308, false], [302.561, true], [301.729, true], [301.312, false], [300.896, false]]) {
  const r = mv(box(459, top), [S]);
  if (snapped) near(top + r.dy, 303.34); else assert.equal(r.dy, 0, `top ${top} is not pulled`);
}
// middle to middle (+3 px pulled, +5 px not) and bottom to top (+6 px)
near(324.623 + mv(box(459, 324.623), [S]).dy, 323.34);
assert.equal(mv(box(459, 325.455), [S]).dy, 0);
near(245.951 + 60 + mv(box(459, 245.951), [S]).dy, 303.34);
// left onto left / right onto left / left onto right (M above S, pulled sideways)
near(191.409 + mv(box(191.409, 150), [S]).dx, 188.99);
near(390.735 + mv(box(390.735, 150), [S]).dx, 388.99);
assert.equal(mv(box(186.405, 150), [S]).dx, 0);                              // -6 px: not pulled
// the middle of M onto the page middle (x 306): +5 px, not -5 px
near(mv(box(256 + 2.08, 600), []).dx + 258.08 + 50, 306);
assert.equal(mv(box(256 - 2.12, 600), []).dx, 0);
// the margin and the page edge are targets too (pm36: left edge onto 36; pe0: onto the page edge)
near(mv(box(38.4, 500), []).dx + 38.4, 36);
near(mv(box(1.25, 500), []).dx + 1.25, 0);
// the options: edges off -> the top edge stays, the middle is still pulled; centre off -> the other way; master off -> nothing
let off = { ...P, edges: false };
assert.equal(mv(box(459, 304.642), [S], off).dy, 0); near(324.623 + mv(box(459, 324.623), [S], off).dy, 323.34);
off = { ...P, center: false };
near(304.642 + mv(box(459, 304.642), [S], off).dy, 303.34); assert.equal(mv(box(459, 324.623), [S], off).dy, 0);
assert.deepEqual(J(mv(box(459, 304.642), [S], { ...P, enabled: false })), { dx: 0, dy: 0, draw: { lines: [], gaps: [], sizes: [] } });
// the guides' own pull (base) wins when it is nearer
assert.equal(mv(box(459, 304.642), [S], P, [0, -0.5]).dy, -0.5);

// ---- what is drawn: one line through the aligned coordinate from the dragged box to the farthest aligned object (s1: a line at y 880 px from S's left to M's right)
let d = mv(box(459, 304.642), [S]).draw;
assert.equal(d.lines.length, 1);
near(d.lines[0].at, 303.34); assert.equal(d.lines[0].axis, 'y'); near(d.lines[0].from, 188.99); near(d.lines[0].to, 559);
// s6 (z-order tests): three objects on one line, the dragged one beside them: the line runs over all of them
const X = [box(150, 353.344), box(350, 353.344), box(550, 353.344)];
d = mv(box(-10, 355.4), X).draw;
assert.deepEqual(J(d.lines.filter((l) => l.axis === 'y').map((l) => [l.at, l.from, l.to].map((v) => Math.round(v)))), [[353, -10, 650], [383, -10, 650], [413, -10, 650]]);
// the middle of the page is drawn pink over the whole height
d = mv(box(256.6, 600), []).draw;
assert.ok(d.lines.some((l) => l.kind === 'page' && l.axis === 'x' && Math.abs(l.at - 306) < 1e-9 && l.from === 0 && l.to === 792));

// ---- equal spacing (s2_space): A and B 100 wide, 100.16 apart, C dragged to the right of B
const A = box(163.887, 311.719), B = box(364.047, 311.719);
// (562.122: out of the spacing zone; the middle of C, 0.12 pt past the page edge 612, lands on the edge: smartguides-c3 i2_e, C let go at 562.4 ended at 562)
near(562.122 + mv(box(562.122, 311.719), [A, B]).dx, 562);
for (const [left, snapped] of [[564.207, true], [567.543, true], [562.956, true], [569.211, false]]) {
  const r = mv(box(left, 311.719), [A, B]);
  if (snapped) near(left + r.dx, 564.207, 0.02); else assert.equal(r.dx, 0, `C at ${left} is not pulled`);
}
let r = mv(box(564.207, 311.719), [A, B]);
assert.equal(r.draw.gaps.length, 2);
r.draw.gaps.sort((a, b) => a.from - b.from);
near(r.draw.gaps[0].from, 263.887); near(r.draw.gaps[0].to, 364.047); near(r.draw.gaps[1].from, 464.047); near(r.draw.gaps[1].to, 564.207);
near(r.draw.gaps[0].at, 371.719); assert.equal(r.draw.gaps[0].axis, 'h');
// the object between two others (gap 200.24: both gaps 50.12)
const B2 = box(464.127, 311.719);
for (const [left, snapped] of [[314.007, true], [316.092, true], [318.177, true], [312.339, true], [319.011, false], [309.837, false]]) {
  const r2 = mv(box(left, 311.719), [A, B2]);
  if (snapped) near(left + r2.dx, 314.007, 0.02); else assert.equal(r2.dx, 0, `C at ${left} is not pulled`);
}
// without a shared line the row is no row (s7: C 40 px lower, 20 px lower and shorter, or far below: free)
for (const c of [box(562, 311.719 + 16.6), box(562, 450), box(562, 311.719 + 8.3, 100, 30)]) assert.equal(mv(c, [A, B]).dx, 0);
// the same in a column (s6: the arrows 11 px right of the rightmost edge)
const VA = [163.887, 100, 263.887, 160], VB = [163.887, 227.5, 263.887, 287.5];
r = mv([163.887, 354.049, 263.887, 414.049], [VA, VB]);
near(354.049 + r.dy, VB[3] + (VB[1] - VA[3]), 0.02);
assert.equal(r.draw.gaps[0].axis, 'v'); near(r.draw.gaps[0].at, 263.887);
// spacing off: free
assert.equal(mv(box(567.543, 311.719), [A, B], { ...P, spacing: false }).dx, 0);

// ---- equal size (s3_dims): the width of M, dragged by the right handle, ends on 200 (the width of S) within +4 / -2 px
for (const [w, snapped] of [[200.04, true], [201.708, true], [199.206, true], [202.542, false], [198.372, false]]) {
  const m = sg.sizeMatch(w, [S], 0, zone, P);
  if (snapped) near(w + m.delta, 200); else assert.equal(m, null, `width ${w} is not pulled`);
}
assert.equal(sg.sizeMatch(200.04, [S], 0, zone, { ...P, dimensions: false }), null);
d = sg.resizeDraw([228.94, 394.97, 428.94, 454.97], [S], page, P, { x: [2], y: [] });
assert.deepEqual(J(d.sizes.map((z) => [z.axis, Math.round(z.from), Math.round(z.to), Math.round(z.at)])), [['w', 229, 429, 455], ['w', 189, 389, 403]]);
// ---- the objects that count as targets: visible ones except the dragged ones; with Alt (a copy) the originals stay and count (s5_mods alt-h)
const pageItems = { items: [{ id: 'a', visible: true, bounds: [0, 0, 10, 10] }, { id: 'b', visible: true, bounds: [20, 0, 30, 10] }, { id: 'c', visible: false, bounds: [40, 0, 50, 10] }] };
assert.deepEqual(J(sg.smartObjects(pageItems, ['a'])), [[20, 0, 30, 10]]);
assert.deepEqual(J(sg.smartObjects(pageItems, ['a'], true)), [[0, 0, 10, 10], [20, 0, 30, 10]]);
// a copy dragged sideways stays on the line of its original (distance 0) instead of being pulled onto the other object 3 px away
const orig = box(459, 304.642);
assert.equal(mv(box(509, 304.642), [S, orig]).dy, 0);
near(304.642 + mv(box(509, 304.642), [S]).dy, 303.34);

// ==== smartguides-c3 (InDesign 2025, real mouse, artifacts/smartguides-c3-20261023/out/i2_*.log; rules/smartguides-c3.ko.md) ====
const px = 0.4163;                                         // points per device pixel at the fit-page zoom of the measuring window
const turned = (x, y, w, h, deg) => {                      // a w x h frame turned by deg about its top-left corner (x, y), as the page items list has it
  const a = (deg * Math.PI) / 180, c = Math.cos(a), si = Math.sin(a), m = [c, -si, si, c, x, y];
  const pts = [[0, 0], [w, 0], [0, h], [w, h]].map(([u, v]) => [m[0] * u + m[2] * v + m[4], m[1] * u + m[3] * v + m[5]]);
  const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]);
  return { id: 't' + deg, visible: true, kind: 'rectangle', transform: m, inner: [0, 0, w, h], bounds: [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)] };
};
const sized = (list) => ({ size: [612, 792], items: list });
// ---- a turned target counts with its bounding box (i2_a: 10, 45 and 90 degrees; top and middle of the box pull, the corners of the shape and the
//      unturned top do not)
for (const deg of [10, 45, 90]) {
  const it = turned(180.71, 374.05, 200, 100, deg);
  const [T] = sg.smartObjects(sized([it]), []);
  const cy = (T[1] + T[3]) / 2, a = (deg * Math.PI) / 180;
  near(T[3] - T[1], 200 * Math.abs(Math.sin(a)) + 100 * Math.abs(Math.cos(a)), 1e-6);
  near(T[1] + 3 * px + mv(box(488.25, T[1] + 3 * px), [T]).dy, T[1]);                       // top onto the box's top
  near(cy + 3 * px - 30 + mv(box(488.25, cy + 3 * px - 30), [T]).dy, cy - 30);              // middle onto the box's middle
  assert.equal(mv(box(488.25, cy - 50 + 3 * px), [T]).dy, 0, `turned ${deg}: the unturned top is no target`);
  if (deg === 45) assert.equal(mv(box(488.25, cy - 35.355 + 3 * px - 60), [T]).dy, 0, 'a corner of the turned shape is no target');
}
// ---- a group counts with its box, never with its members (i2_a grp-*): the page items list has the group as one item
const grp = { id: 'g', visible: true, kind: 'group', transform: [1, 0, 0, 1, 0, 0], inner: [180.71, 303.34, 380.71, 423.34], bounds: [180.71, 303.34, 380.71, 423.34],
  children: [{ id: 'm1', visible: true, bounds: [180.71, 303.34, 260.71, 343.34] }, { id: 'm2', visible: true, bounds: [300.71, 373.34, 380.71, 423.34] }] };
const [G] = sg.smartObjects(sized([grp]), []);
assert.equal(sg.smartObjects(sized([grp]), []).length, 1);
near(363.34 + 3 * px - 30 + mv(box(488.25, 363.34 + 3 * px - 30), [G]).dy, 333.34);         // middle onto the middle of the group's box
assert.equal(mv(box(488.25, 373.34 + 3 * px), [G]).dy, 0, 'the top of a member is no target');
assert.equal(mv(box(488.25, 398.34 + 3 * px - 30), [G]).dy, 0, 'the middle of a member is no target');
// ---- the pasteboard (i2_c): an object wholly off the page is no target; an object dragged wholly off the page is not pulled; one that reaches
//      onto the page is a target
const offPage = { id: 'off', visible: true, bounds: [-200, 353.34, -100, 453.34] }, half = { id: 'half', visible: true, bounds: [-100, 353.34, 100, 453.34] };
assert.deepEqual(J(sg.smartObjects(sized([offPage, half]), [])), [[-100, 353.34, 100, 453.34]]);
assert.equal(mv(box(-359.25, 353.34 + 3 * px), [[-500, 353.34, -400, 453.34]]).dy, 0, 'both on the pasteboard: free');
assert.equal(mv(box(-359.25, 396 + 3 * px - 30), []).dy, 0, 'the page middle does not pull an object on the pasteboard');
near(353.34 + 3 * px + mv(box(320.25, 353.34 + 3 * px), [[-100, 353.34, 100, 453.34]]).dy, 353.34);
// ---- the other pages of the spread (i2_f, a two-page spread 612 + 612): an object on the left page pulls one dragged on the right page
const left = sized([{ id: 'L', visible: true, bounds: [200, 300, 400, 400] }]), right = sized([{ id: 'M', visible: true, bounds: [208, 301.25, 308, 361.25] }]);
const mates = sg.smartObjects(right, ['M'], false, [{ items: left, dx: -612, dy: 0 }]);
assert.deepEqual(J(mates), [[-412, 300, -212, 400]]);
r = sg.smartMove([208, 301.25, 308, 361.25], mates, page, P, zone);
near(301.25 + r.dy, 300);
assert.ok(r.draw.lines.some((l) => l.axis === 'y' && Math.abs(l.at - 300) < 1e-6 && l.from <= -412 + 1e-6 && l.to >= 300), 'the line runs across the spine');
// the same item listed on both pages counts once
assert.equal(sg.smartObjects(right, [], false, [{ items: sized([{ id: 'M', visible: true, bounds: [820, 301.25, 920, 361.25] }]), dx: -612, dy: 0 }]).length, 1);
// ---- the page edges and the margins catch the middle too (i2_e): right edge 612, left edge 0, top edge 0, left margin 36, right margin 576
near(mv(box(612 + 3 * px - 50, 373), []).dx + 612 + 3 * px - 50, 562);
near(mv(box(3 * px - 50, 373), []).dx + 3 * px - 50, -50);
near(mv(box(320.25, 3 * px - 30), []).dy + 3 * px - 30, -30);
near(mv(box(36 + 3 * px - 50, 373), []).dx + 36 + 3 * px - 50, -14);
near(mv(box(576 + 3 * px - 50, 373), []).dx + 576 + 3 * px - 50, 526);
// the line of a margin is as long as the margin box (x 576.4, y 36.6..755.4 measured)
d = mv(box(576 + 3 * px - 100, 373), []).draw;
assert.deepEqual(J(d.lines.map((l) => [l.axis, Math.round(l.at), Math.round(l.from), Math.round(l.to)])), [['x', 576, 36, 756]]);
// ---- an equal-spacing place within its zone wins over a nearer alignment (i2_e: C let go at 562.956 and 563.4 ended at 564.21 with the arrows,
//      although its middle was only 0.956 / 1.4 pt from the page edge; at 562.4, out of the spacing zone, it ended on the edge, 562)
near(562.956 + mv(box(562.956, 311.719), [A, B]).dx, 564.207, 0.02);
near(563.4 + mv(box(563.4, 311.719), [A, B]).dx, 564.207, 0.02);
near(562.4 + mv(box(562.4, 311.719), [A, B]).dx, 562);
assert.equal(mv(box(562.4, 311.719), [A, B]).draw.gaps.length, 0);
// C hangs over the page edge there: beside the arrows only the line along the bottom edges (y 371.7, x 163.9..664.3)
d = mv(box(564.207, 311.719), [A, B]).draw;
assert.deepEqual(J(d.lines.map((l) => [l.axis, Math.round(l.at * 10) / 10, Math.round(l.from * 10) / 10, Math.round(l.to * 10) / 10])), [['y', 371.7, 163.9, 664.2]]);
// ---- beside the spacing arrows the lines join the dragged box with ONE partner (i2_d, 15 layouts): the object sharing the most lines, the
//      leftmost (topmost) on a tie; [x0, y0, x1, y1]
const lines = (m, others) => J(sg.smartMove(m, others, page, P, zone).draw.lines.map((l) => [l.axis, Math.round(l.at * 10) / 10, Math.round(l.from * 10) / 10, Math.round(l.to * 10) / 10]));
const rowA = (y0, h) => [155.6, y0, 255.6, y0 + h], rowB = (y0, h) => [435.6, y0, 535.6, y0 + h], rowM = (y0, h) => [295.6, y0, 395.6, y0 + h];
// all three alike: the three lines, from A to M (never on to B)
assert.deepEqual(lines(rowM(270.1, 60), [rowA(270.1, 60), rowB(270.1, 60)]), [['y', 270.1, 155.6, 395.6], ['y', 300.1, 155.6, 395.6], ['y', 330.1, 155.6, 395.6]]);
assert.deepEqual(lines(rowM(270.1, 60), [rowB(270.1, 60), rowA(270.1, 60)]), [['y', 270.1, 155.6, 395.6], ['y', 300.1, 155.6, 395.6], ['y', 330.1, 155.6, 395.6]]);
// A taller (tops equal, or bottoms equal): B shares three lines, A one: M to B
assert.deepEqual(lines(rowM(270.1, 60), [rowA(270.1, 100), rowB(270.1, 60)]), [['y', 270.1, 295.6, 535.6], ['y', 300.1, 295.6, 535.6], ['y', 330.1, 295.6, 535.6]]);
assert.deepEqual(lines(rowM(310.1, 60), [rowA(270.1, 100), rowB(310.1, 60)]), [['y', 310.1, 295.6, 535.6], ['y', 340.1, 295.6, 535.6], ['y', 370.1, 295.6, 535.6]]);
// B taller: A to M; only the middles equal (100, 80, 60 high): one line, A to M; only the tops equal: one line, A to M
assert.deepEqual(lines(rowM(310.1, 60), [rowA(310.1, 60), rowB(270.1, 100)]), [['y', 310.1, 155.6, 395.6], ['y', 340.1, 155.6, 395.6], ['y', 370.1, 155.6, 395.6]]);
assert.deepEqual(lines(rowM(290.1, 60), [rowA(270.1, 100), rowB(280.1, 80)]), [['y', 320.1, 155.6, 395.6]]);
assert.deepEqual(lines(rowM(270.1, 60), [rowA(270.1, 100), rowB(270.1, 80)]), [['y', 270.1, 155.6, 395.6]]);
// beside a pair (M on the right): the leftmost partner, the line over all three; M on the left of a pair: its neighbour only
assert.deepEqual(lines([435.6, 270.1, 535.6, 330.1], [[155.6, 270.1, 255.6, 370.1], [295.6, 270.1, 395.6, 350.1]]), [['y', 270.1, 155.6, 535.6]]);
assert.deepEqual(lines([155.6, 270.1, 255.6, 330.1], [[295.6, 270.1, 395.6, 370.1], [435.6, 270.1, 535.6, 350.1]]), [['y', 270.1, 155.6, 395.6]]);
// in a column: alike -> the three lines from A (above) to M; A wider (lefts equal) -> M to B
const colv = (y0, x0, w) => [x0, y0, x0 + w, y0 + 60];
assert.deepEqual(lines(colv(245.2, 280.7, 100), [colv(145.2, 280.7, 100), colv(345.2, 280.7, 100)]), [['x', 280.7, 145.2, 305.2], ['x', 330.7, 145.2, 305.2], ['x', 380.7, 145.2, 305.2]]);
assert.deepEqual(lines(colv(245.2, 280.7, 100), [colv(145.2, 280.7, 160), colv(345.2, 280.7, 100)]), [['x', 280.7, 245.2, 405.2], ['x', 330.7, 245.2, 405.2], ['x', 380.7, 245.2, 405.2]]);
// the end marks of the arrows hang off their own objects: A (bottom 370.1) and M, B (bottom 330.1); the arrows hang off the lowest (370.1)
r = sg.smartMove(rowM(270.1, 60), [rowA(270.1, 100), rowB(270.1, 60)], page, P, zone);
assert.deepEqual(J(r.draw.gaps.map((g) => [Math.round(g.from * 10) / 10, Math.round(g.to * 10) / 10, g.at, g.ends])), [[255.6, 295.6, 370.1, [370.1, 330.1]], [395.6, 435.6, 370.1, [330.1, 330.1]]]);
// ---- equal size: a turned target is compared by its own width and height (i2_b: turned 90 or 45 degrees, the width 200 it has unturned pulled,
//      its box width 100 / 212.13 did not); a group by its box (200 pulled, a member's 80 did not)
for (const deg of [90, 45]) {
  const [T] = sg.smartObjects(sized([turned(180.71, 220.09, 200, 100, deg)]), []);
  near(sg.sizeOf(T, 0), 200, 1e-6); near(sg.sizeOf(T, 1), 100, 1e-6);
  near(200.83 + sg.sizeMatch(200.83, [T], 0, zone, P).delta, 200);
  assert.equal(sg.sizeMatch(T[2] - T[0] + 0.3, [T], 0, zone, P), null, 'the box width of a turned target does not pull');
  const dr = sg.resizeDraw([220.71, 394.97, 420.71, 454.97], [T], page, P, { x: [2], y: [] });
  assert.equal(dr.sizes.length, 2);
  // its bracket runs along its own bottom side (200 long), turned with it
  const z = dr.sizes[1];
  near(Math.hypot(z.seg[2] - z.seg[0], z.seg[3] - z.seg[1]), 200, 1e-6); near(Math.hypot(z.out[0], z.out[1]), 1, 1e-9);
  if (deg === 90) { near(z.seg[0], z.seg[2], 1e-6); near(z.seg[0], T[2], 1e-6); near(z.out[0], 1, 1e-9); } // turned 90: an upright bracket right of the box
}
near(200.83 + sg.sizeMatch(200.83, [G], 0, zone, P).delta, 200);
assert.equal(sg.sizeMatch(80.8, [G], 0, zone, P), null);
// a turned object resized by a handle: its own height against the others' heights (i2_b2: 150 -> 100 pulled beside a 200 x 100 frame, -> 200
// not; beside a 100 x 200 frame the other way round); the bracket hangs off its own right side
const own = sg.localFrame([0, -1, 1, 0, 205.71, 394.97], [0, 0, 60, 100]);
near(own.w, 60, 1e-9); near(own.h, 100, 1e-9);
d = sg.resizeDraw([205.71, 334.97, 305.71, 394.97], [S], page, P, { x: [], y: [2] }, own);
assert.equal(d.sizes.length, 2); assert.equal(d.sizes[0].axis, 'h'); assert.ok(d.sizes[0].seg && d.lines.length === 0);
assert.equal(sg.resizeDraw([205.71, 334.97, 305.71, 394.97], [S], page, P, { x: [2], y: [] }, own).sizes.length, 0);
console.log('smartguides: ok');
