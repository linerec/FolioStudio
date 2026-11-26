// Run with node next/app/scripts/test-anchorbox.cjs; uses only the installed TypeScript compiler.
// The anchored object control (View > Extras > Anchored Object Control): `markPlacement` in features/view/marks.ts puts the 9 x 8 square (plain object) and the
// 19 x 19 anchor symbol box (object set in text) where InDesign 2025 draws them, on plain, turned and flipped objects. The cases are the ones measured with the
// input-free screenshot rig (next/oracle/measured/anchor-box-2025.json, rules in next/oracle/rules/anchorbox-c1.ko.md).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const src = path.join(__dirname, '../renderer/src');
const source = fs.readFileSync(path.join(src, 'features/view/marks.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const apply = (m, x, y) => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];
const inspect = { corners: (m, b) => [apply(m, b[0], b[1]), apply(m, b[2], b[1]), apply(m, b[2], b[3]), apply(m, b[0], b[3])] };
const m = {};
vm.runInNewContext(js, { exports: m, Math, require: (n) => (n.endsWith('inspect') ? inspect : n.endsWith('i18n') ? { tr: (s) => s } : {}) });
const measured = JSON.parse(fs.readFileSync(path.join(__dirname, '../../oracle/measured/anchor-box-2025.json'), 'utf8'));

// an object whose own axes are u (x) and v (y) on the screen, w_px x h_px screen px (w_px / s points, s px per point), its origin at (300, 300)
const item = (c, anchored) => ({ id: 'x', kind: 'rectangle', transform: [c.u[0], c.u[1], c.v[0], c.v[1], 300 / c.s, 300 / c.s], inner: [0, 0, c.w_px / c.s, c.h_px / c.s], anchored });
const place = (c) => m.markPlacement(item(c, c.kind === 'box'), c.s, { liveCorners: false, anchorControl: true }).anchor;
let n = 0, worst = { box: 0, square: 0 };
for (const c of measured.cases) {
  const got = place(c);
  assert.ok(got, c.name + ': drawn');
  // E = the own top-right corner minus 20 % of the width along the own x axis (always the 20 % point: the offsets were measured from it)
  const tr = [300 + c.u[0] * c.w_px, 300 + c.u[1] * c.w_px], E = [tr[0] - 0.2 * c.w_px * c.u[0], tr[1] - 0.2 * c.w_px * c.u[1]];
  const dx = got.x - E[0] - c.off[0], dy = got.y - E[1] - c.off[1], d = Math.hypot(dx, dy);
  worst[c.kind] = Math.max(worst[c.kind], d);
  assert.ok(Math.abs(dx) <= 1.5 && Math.abs(dy) <= 1.5, `${c.name}: the ${c.kind} is ${dx.toFixed(2)}, ${dy.toFixed(2)} px from where InDesign drew it`);
  n++;
}
assert.ok(n >= 100, 'measured cases checked: ' + n);
// the plain object control keeps the earlier shape rules: hidden below 19 px width, off when the setting is off
const small = { id: 'y', kind: 'rectangle', transform: [1, 0, 0, 1, 0, 0], inner: [0, 0, 17, 40] };
assert.equal(m.markPlacement(small, 1, { liveCorners: false, anchorControl: true }).anchor, null);
assert.equal(m.markPlacement({ ...small, inner: [0, 0, 100, 40] }, 1, { liveCorners: false, anchorControl: false }).anchor, null);
console.log(`PASS test-anchorbox: ${n} measured cases, worst distance box ${worst.box.toFixed(2)} px, square ${worst.square.toFixed(2)} px`);
