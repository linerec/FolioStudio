// Run with node next/app/scripts/test-snap.cjs; uses only the installed TypeScript compiler.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/snap.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const m = {};
vm.runInNewContext(js, { exports: m });

// arrays made inside the vm have another prototype: compare through JSON
const eq = (a, b, msg) => assert.deepEqual(JSON.parse(JSON.stringify(a)), b, msg);
const items = (guides = [], columns = { count: 1, gutter: 0, positions: null }) => ({ guides, columns });
const base = { size: [600, 800], margins: [36, 36, 36, 36], guides: true, hiddenGuides: () => false, grid: null, pageLeft: 0, pageTop: 0 };
const zone = 4;

// page edges, margins, ruler guides: a box within the zone is pulled on, one outside it is left
let t = m.snapTargets({ ...base, items: items([{ orientation: 'vertical', location: 300, layer: null }, { orientation: 'horizontal', location: 400, layer: null }]) });
eq(t.x.sort((a, b) => a - b), [0, 36, 300, 564, 600]);
eq(t.y.sort((a, b) => a - b), [0, 36, 400, 764, 800]);
eq(m.snapBox([38, 100, 138, 150], t, zone), [-2, 0], 'left edge onto the margin');
eq(m.snapBox([60, 100, 160, 150], t, zone), [0, 0], 'nothing near');
eq(m.snapBox([252, 100, 340, 150], t, zone), [4, 0], 'a centre 4 pt from the guide is inside the zone');
eq(m.snapBox([252, 100, 338, 150], t, zone), [0, 0], 'one 5 pt away is not');
eq(m.snapBox([250, 100, 349, 150], t, zone), [0.5, 0], 'the middle meets the vertical guide');
eq(m.snapBox([100, 397, 200, 447], t, zone), [0, 3], 'the top edge onto the horizontal guide');
// the nearer target wins
t = m.snapTargets({ ...base, items: items([{ orientation: 'vertical', location: 102, layer: null }, { orientation: 'vertical', location: 99, layer: null }]) });
eq(m.snapBox([100, 100, 200, 200], t, zone)[0], -1, 'the guide 1 pt away, not the one 2 pt away');
// guides off: nothing but the grid; hidden layers' guides are skipped
t = m.snapTargets({ ...base, guides: false, items: items([{ orientation: 'vertical', location: 102, layer: null }]) });
eq(t.x, []);
t = m.snapTargets({ ...base, items: items([{ orientation: 'vertical', location: 102, layer: 'L' }]), hiddenGuides: (l) => l === 'L' });
assert.ok(!t.x.includes(102));
// column guides between the margins
t = m.snapTargets({ ...base, items: items([], { count: 2, gutter: 12, positions: null }) });
assert.ok(t.x.includes(36 + 258) && t.x.includes(36 + 258 + 12), 'column edges ' + t.x);
// the document grid: every 72 / 8 = 9 pt; edges only; the right-hand page continues the left page's grid
t = m.snapTargets({ ...base, guides: false, items: items(), grid: { horizontal: 72, vertical: 72, horizontalSub: 8, verticalSub: 8 } });
eq(m.snapBox([10, 20, 60, 70], t, zone), [-1, -2], 'left 10 → 9, top 20 → 18 (right 60 → 63 is 3 away but the left edge is nearer)');
t = m.snapTargets({ ...base, guides: false, items: items(), grid: { horizontal: 72, vertical: 100, horizontalSub: 1, verticalSub: 4 }, pageLeft: 600 });
// 600 is 6 gridlines of 25 on: the page's left edge is a gridline, so x = 10 → 0 is not within 4, 24 → 25 is
eq(m.snapBox([24, 100, 80, 144], t, zone)[0], 1);
eq(m.snapBox([12, 100, 40, 144], t, zone)[0], 0, '12 and 40 are 12 and 10 from the lines');
// a handle: only the axes being dragged
t = m.snapTargets({ ...base, items: items() });
eq(m.snapPoint([563, 21], t, zone, { x: true, y: false }), [1, 0]);
eq(m.snapPoint([563, 21], t, zone, { x: false, y: true }), [0, 0]);
eq(m.snapPoint([563, 38], t, zone), [1, -2]);
console.log('snap: 17 checks PASS');
