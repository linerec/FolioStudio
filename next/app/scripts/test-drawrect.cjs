// Run with node next/app/scripts/test-drawrect.cjs; uses only the installed TypeScript compiler.
// `drawRect` (what a drag with Shift / Alt draws, in the page as shown) is checked against every real drag measured in InDesign 2025 on turned spreads
// (next/oracle/measured/turned-draw-2025.json: pointer positions in page-as-shown points and the box InDesign made). InDesign rounds the pointer to a
// grid that depends on the zoom (0.75 pt and finer), so the comparison allows 1 pt; the typed-dimensions click is checked too (top-left at the click,
// the middle at the click with Alt).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const src = path.join(__dirname, '../renderer/src');
const source = fs.readFileSync(path.join(src, 'ui/drawing.tsx'), 'utf8');
// only drawRect: the file also holds the preview component (JSX); cut the function out
const start = source.indexOf('export function drawRect');
const closing = /\r?\n\}\r?\n/g;
closing.lastIndex = start;
const end = closing.exec(source).index + 3;
const code = ts.transpileModule(source.slice(start, end), { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const m = {};
vm.runInNewContext(code, { exports: m, Math });
const measured = JSON.parse(fs.readFileSync(path.join(__dirname, '../../oracle/measured/turned-draw-2025.json'), 'utf8'));
const KIND = { rect: 'rectangle', oval: 'ellipse', poly: 'polygon', rectframe: 'rectangleFrame', ovalframe: 'ellipseFrame', polyframe: 'polygonFrame' };
const HEXAGON = 2 / Math.sqrt(3);
let drags = 0, clicks = 0;
for (const c of measured.cases) {
  const kind = KIND[c.tool];
  const shift = c.mods.includes('shift'), alt = c.mods.includes('alt');
  let got;
  if (c.kind === 'drag') {
    got = m.drawRect(kind, c.down, c.up, shift, alt, kind.startsWith('polygon') ? HEXAGON : 1);
    drags += 1;
  } else {
    const [w, h] = c.dims;
    got = alt ? [c.at[0] - w / 2, c.at[1] - h / 2, c.at[0] + w / 2, c.at[1] + h / 2] : [c.at[0], c.at[1], c.at[0] + w, c.at[1] + h];
    clicks += 1;
  }
  got.forEach((v, i) => assert.ok(Math.abs(v - c.shown[i]) <= 1, `${c.turn} ${c.label}: ${JSON.stringify(got)} vs InDesign ${JSON.stringify(c.shown)}`));
}
assert.ok(drags >= 40, `${drags} drags`);
assert.ok(clicks >= 10, `${clicks} clicks`);
// the rules in plain numbers
const plain = (v) => JSON.parse(JSON.stringify(v));
assert.deepEqual(plain(m.drawRect('rectangle', [40, 60], [190, 120], true, false)), [40, 60, 100, 120]);
assert.deepEqual(plain(m.drawRect('rectangle', [190, 150], [40, 100], true, false)), [140, 100, 190, 150]);
assert.deepEqual(plain(m.drawRect('ellipse', [110, 120], [190, 160], false, true)), [30, 80, 190, 160]);
assert.deepEqual(plain(m.drawRect('rectangle', [110, 120], [190, 160], true, true)), [70, 80, 150, 160]);
console.log(`PASS test-drawrect (${drags} drags, ${clicks} clicks)`);
