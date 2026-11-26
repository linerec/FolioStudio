// Run with node next/app/scripts/test-zeropoint.cjs; uses only the installed TypeScript compiler.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/zeropoint.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const m = {};
vm.runInNewContext(js, { exports: m });
const eq = (a, b, msg) => assert.deepEqual(a === null ? a : Array.from(a), b, msg);

eq(m.zeroOf(null), [0, 0]);
eq(m.zeroOf({}), [0, 0]);
eq(m.zeroOf({ zeroPoint: [99, 201] }), [99, 201]);
// InDesign 2025: a single page, spread or page ruler; a rectangle at [300, 300, 400, 400] reads [99, 201, ...] with the zero point [99, 201]
for (const origin of ['SpreadOrigin', 'PageOrigin', 'SpineOrigin']) {
  const off = m.rulerOffset(origin, [0, 0], [99, 201]);
  eq([300 + off[0], 300 + off[1]], [201, 99], origin);
}
// a drop at (100, 200) of the only page makes [100, 200]; a double click puts [0, 0] back
eq(m.zeroPointAt('SpreadOrigin', [0, 0], [100, 200]), [100, 200]);
eq(m.zeroPointAt('PageOrigin', [0, 0], [100, 200]), [100, 200]);
// the right page of a spread (612 wide): a spread ruler counts from the spread, a page ruler from the page (InDesign 2025: [662.25, 60.75] and [50.25, 60.75])
eq(m.zeroPointAt('SpreadOrigin', [612, 0], [50.25, 60.75]), [662.25, 60.75]);
eq(m.zeroPointAt('PageOrigin', [612, 0], [50.25, 60.75]), [50.25, 60.75]);
// the spine ruler cannot move the zero point
assert.equal(m.zeroPointAt('SpineOrigin', [612, 0], [50, 60]), null);
// shown values = position - zero point: a vertical guide at 250 reads 151 less the old origin
eq(m.rulerOffset('SpreadOrigin', [612, 0], [99, 201]), [513, -201]);
eq(m.rulerOffset('PageOrigin', [612, 0], [99, 201]), [-99, -201]);
console.log('zeropoint: ok');
