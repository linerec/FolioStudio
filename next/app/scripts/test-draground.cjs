// Replays InDesign 2025's drags (draground-c1, next/oracle/rules/draground-c1-golden.json: DOM readings after REAL mouse drags, pointer points exact) through the
// drag code Folio uses: ui/dragunit.ts (grid, Shift line, handle drags) and ui/handledrag.ts (the box a handle drag leaves). Run with node next/app/scripts/test-draground.cjs.
// A real mouse is not exact (InDesign puts the pointer a pixel off in about one drag in five), so a group passes when most rows agree, not all.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

const load = (name) => {
  const m = {};
  vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(__dirname, '../renderer/src/ui', name), 'utf8'), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: m });
  return m;
};
const du = load('dragunit.ts'), hd = load('handledrag.ts');
const g = JSON.parse(fs.readFileSync(path.join(__dirname, '../../oracle/rules/draground-c1-golden.json'), 'utf8'));
const close = (a, b, tol) => a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) <= tol);
const rate = (name, rows, ok, min) => {
  if (!rows.length) { console.log(`draground ${name}: no rows`); return; }
  const n = rows.filter(ok).length;
  const zs = [...new Set(rows.map((r) => r.zoom))].sort((x, y) => x - y);
  console.log(`draground ${name}: ${n}/${rows.length}  (by zoom: ${zs.map((z) => `${z} %: ${rows.filter((r) => r.zoom === z && ok(r)).length}/${rows.filter((r) => r.zoom === z).length}`).join(', ')})`);
  assert.ok(n / rows.length >= min, `${name}: only ${n} of ${rows.length} agree with InDesign (need ${min * 100} %)`);
};
// the unit by zoom must be the one the golden was measured at (InDesign's zoom is in each row)
const snap = (p, z) => [du.snapPointer(p[0], z), du.snapPointer(p[1], z)];

// 1. lines: both end points on the grid; Shift: the angle to 45 degrees, the shorter way on a diagonal
rate('line start point', g.line.filter((r) => !r.alt), (r) => close(snap(r.press, r.zoom), r.start, 0.011), 0.6);
rate('line end point', g.line.filter((r) => !r.alt && !r.shift), (r) => close(snap(r.release, r.zoom), r.end, 0.011), 0.6);
// Shift: the end follows the start InDesign put down (a pointer a pixel off moves the start, and the end with it)
rate('line end point with Shift', g.line.filter((r) => !r.alt && r.shift), (r) => {
  const b = snap(r.release, r.zoom), v = du.shiftLine([b[0] - r.start[0], b[1] - r.start[1]]);
  return close([r.start[0] + v[0], r.start[1] + v[1]], r.end, 0.011);
}, 0.6);
// Alt: drawn from the middle: the middle is the pointer's grid point at the press, the end the one at the release (the start is the mirror of it)
rate('line drawn from the middle (Alt), middle', g.line.filter((r) => r.alt), (r) => close(snap(r.press, r.zoom), [(r.start[0] + r.end[0]) / 2, (r.start[1] + r.end[1]) / 2], 0.011), 0.6);
rate('line drawn from the middle (Alt), end', g.line.filter((r) => r.alt), (r) => close(snap(r.release, r.zoom), r.end, 0.011), 0.6);
// 2. a dragged end point of a line (Direct Selection) or a bounding-box handle of a line (Selection): the pointer's grid point
rate('line end point', g.lineEnd, (r) => close(snap(r.release, r.zoom), r.after, 0.011), 0.6);
// 3. a handle drag of an upright or turned rectangle: the box (as four corners in the page) InDesign left
const apply = (m, x, y) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
const invert = (m) => { const det = m[0] * m[3] - m[1] * m[2]; return [m[3] / det, -m[1] / det, -m[2] / det, m[0] / det]; };
const handleCorners = (r) => {
  const [fx, fy] = r.handle, b = r.inner;
  const at = apply(r.m, b[0] + (b[2] - b[0]) * fx, b[1] + (b[3] - b[1]) * fy);
  const raw = [r.to[0] - r.from[0], r.to[1] - r.from[1]];
  const by = du.handleBy({ fx, fy, at, from: r.from, m: r.m }, raw[0], raw[1], true, true, r.zoom);
  const inv = invert(r.m);
  const own = [inv[0] * by[0] + inv[2] * by[1], inv[1] * by[0] + inv[3] * by[1]];
  const [x0, y0, x1, y1] = hd.handleRect(b, [fx, fy], own, r.shift, r.alt);
  return [apply(r.m, x0, y0), apply(r.m, x1, y0), apply(r.m, x0, y1), apply(r.m, x1, y1)];
};
const sameBox = (r) => { const c = handleCorners(r); return c.every((p, i) => close(p, r.expected[i], 0.03)); };
rate('handle drags of upright rectangles', g.handle.filter((r) => r.angle === 0), sameBox, 0.6);
rate('handle drags of turned rectangles', g.handle.filter((r) => r.angle !== 0 && !r.shift && !r.alt), sameBox, 0.6);
rate('handle drags of turned rectangles with Shift or Alt', g.handle.filter((r) => r.angle !== 0 && (r.shift || r.alt)), sameBox, 0.55);
// pressed beside the handle: the handle still goes to the pointer's grid point
rate('handle pressed beside the handle', g.handle.filter((r) => r.off && (r.off[0] || r.off[1])), sameBox, 0.55);
// 4. a dragged picture: its top-left corner lands on the grid, rounded down like a moved frame
const cornerAxis = (r, a) => du.snapCorner(r.topLeft[a] + r.release[a] - r.press[a], r.zoom);
rate('content drag, left edge', g.content, (r) => Math.abs(cornerAxis(r, 0) - r.after[0]) < 0.011, 0.6);
rate('content drag, top edge', g.content, (r) => Math.abs(cornerAxis(r, 1) - r.after[1]) < 0.011, 0.6);
console.log('draground: ok');
