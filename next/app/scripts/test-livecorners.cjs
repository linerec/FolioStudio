// Run with node next/app/scripts/test-livecorners.cjs; uses only the installed TypeScript compiler.
// The rules (radius, shapes, Alt cycle) run in the engine too (cargo test -p folio-edit livecorners): the two must agree. The pointer-to-points step
// (`dragBy`: the screen axis nearest the frame's own horizontal edge, minus a dead zone) is checked here against the cases measured in InDesign 2025
// (next/oracle/measured/live-corners-2025.json).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const src = path.join(__dirname, '../renderer/src');
const source = fs.readFileSync(path.join(src, 'features/view/livecorners.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const apply = (m, x, y) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
const inspect = { corners: (m, b) => [apply(m, b[0], b[1]), apply(m, b[2], b[1]), apply(m, b[2], b[3]), apply(m, b[0], b[3])] };
const m = {};
vm.runInNewContext(js, { exports: m, Math, require: (n) => (n.endsWith('inspect') ? inspect : {}) });
const measured = JSON.parse(fs.readFileSync(path.join(__dirname, '../../oracle/measured/live-corners-2025.json'), 'utf8'));

const c = (shape, radius) => ({ shape, radius });
// the same vectors as the engine's unit tests
assert.equal(m.radiusAfter(0, 14, 100), 14);
assert.equal(m.radiusAfter(0, 400, 100), 50);
assert.equal(m.radiusAfter(0, 400, 60), 30);
assert.equal(m.radiusAfter(20, -12, 100), 8);
assert.equal(m.radiusAfter(20, -40, 100), 0);
const none = [c('None', 12), c('None', 12), c('None', 12), c('None', 12)];
assert.deepEqual(JSON.parse(JSON.stringify(m.draggedCorners(none, 0, 25, false, 100))), [0, 1, 2, 3].map(() => c('RoundedCorner', 25)));
assert.equal(m.draggedCorners(none, 1, 3, false, 100)[1].radius, 3);
const mixed = [c('BevelCorner', 10), c('InsetCorner', 20), c('FancyCorner', 30), c('None', 40)];
assert.deepEqual(JSON.parse(JSON.stringify(m.draggedCorners(mixed, 0, 15, false, 100))), [c('BevelCorner', 25), c('InsetCorner', 25), c('FancyCorner', 25), c('None', 25)]);
assert.deepEqual(JSON.parse(JSON.stringify(m.draggedCorners(mixed, 1, 14.25, true, 100)))[1], c('InsetCorner', 34.25));
let shape = 'None';
const seen = [];
for (let i = 0; i < 7; i++) { shape = m.nextShape(shape); seen.push(shape); }
assert.deepEqual(seen, ['RoundedCorner', 'FancyCorner', 'BevelCorner', 'InsetCorner', 'InverseRoundedCorner', 'None', 'RoundedCorner']);

// the diamonds: on the top edge, `radius` in from the corner (on the corner while it has no shape)
const frame = (w, h, deg) => {
  const r = (deg * Math.PI) / 180;
  return { transform: [Math.cos(r), -Math.sin(r), Math.sin(r), Math.cos(r), 0, 0], inner: [0, 0, w, h], corners: undefined };
};
const f = frame(200, 100, 0);
const near = (a, b, msg) => assert.ok(a.every((v, i) => Math.abs(v - b[i]) < 1e-9), `${msg}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`);
near(m.diamondPlacement(f, 2).flat(), [0, 0, 400, 0, 0, 200, 400, 200], 'no shape: on the corners');
near(m.diamondPlacement(f, 2, [c('RoundedCorner', 20), c('RoundedCorner', 20), c('BevelCorner', 10), c('None', 30)]).flat(), [40, 0, 360, 0, 20, 200, 400, 200], 'in from the corners');

// the pointer: a drag of 80 px at 0.42 pt/px with the first 5 px lost
const ppp = 0.42, s = 1 / ppp, ppt0 = ppp;
near([m.dragBy(f, 0, [100, 100], [180, 100], s)], [(80 - 5) * ppp], 'horizontal');
near([m.dragBy(f, 1, [100, 100], [20, 100], s)], [(80 - 5) * ppp], 'top right inward is left');
near([m.dragBy(f, 0, [100, 100], [100, 180], s)], [0], 'vertical does nothing on an upright frame');
near([m.dragBy(f, 0, [100, 100], [60, 100], s)], [-(40 - 5) * ppp], 'outward');
near([m.dragBy(f, 0, [100, 100], [103, 100], s)], [0], 'inside the dead zone');
near([m.dragBy(frame(200, 100, 90), 0, [100, 100], [100, 20], s)], [(80 - 5) * ppp], 'turned 90: up along the edge');
near([m.dragBy(frame(200, 100, 90), 0, [100, 100], [180, 100], s)], [0], 'turned 90: sideways does nothing');
near([m.dragBy(frame(200, 100, 45), 0, [100, 100], [180, 100], s)], [(80 - 5) * ppp], '45 counts as x');
near([m.dragBy(frame(200, 100, 150), 0, [100, 100], [20, 100], s)], [(80 - 5) * ppp], '150: left');

// a flipped frame follows its own axes (measured: the yellow square goes to the flipped right edge, the diamond on the visual top left is the frame's own
// top right, dragging it to the right is inward): flip about the middle of a 200 x 100 frame at (0,0)
const flipH = { transform: [-1, 0, 0, 1, 200, 0], inner: [0, 0, 200, 100], corners: undefined };
near([m.dragBy(flipH, 1, [100, 100], [160, 100], s)], [(60 - 5) * ppt0], 'flipped: the own top right is on the visual left, inward is right');
near([m.dragBy(flipH, 0, [100, 100], [160, 100], s)], [-(60 - 5) * ppt0], 'flipped: the own top left is on the visual right, right is outward');
near(m.diamondPlacement(flipH, 2, [c('RoundedCorner', 10), c('RoundedCorner', 20), c('RoundedCorner', 30), c('RoundedCorner', 40)]).slice(0, 2).flat(), [380, 0, 40, 0], 'flipped: diamonds at the own corners (visual right and left)');

// every measured drag with a fine pointer path: the screen's step lands within the measured noise (the dead zone ran 4 to 17 px, 4 to 6 on a quiet machine, results are rounded to 0.75 pt)
let checked = 0;
for (const d of measured.drags) {
  if (d.coarse) continue;
  const ppt = d.pt_per_px, shorter = Math.min(d.w, d.h);
  const start = d.before[d.corner][0] === 'None' ? 0 : d.before[d.corner][1];
  const got = d.after[d.corner][1] - start;
  const want = m.dragBy(frame(d.w, d.h, d.rot), d.corner, d.from_px, d.to_px, 1 / ppt);
  const tol = 18 * ppt;
  const after = d.after[d.corner][1];
  if (after >= shorter / 2 - 1e-9) assert.ok(start + want >= shorter / 2 - tol, `${d.label}: ${want} should reach the largest radius`);
  else if (after <= 1e-9) assert.ok(start + want <= tol, `${d.label}: ${want} should stay at 0`);
  else assert.ok(Math.abs(want - got) <= tol, `${d.label}: ${want} vs measured ${got} (tolerance ${tol})`);
  checked++;
}
assert.ok(checked >= 30, `${checked} measured drags checked`);
console.log(`livecorners: ok (${checked} measured drags)`);
