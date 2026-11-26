// Run with node next/app/scripts/test-dragunit.cjs; uses only the installed TypeScript compiler.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/dragunit.ts'), 'utf8');
const m = {};
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: m });
const near = (a, b, msg) => assert.ok(Math.abs(a - b) < 1e-9, `${msg}: ${a} != ${b}`);

// InDesign 2025, measured: the unit by zoom
for (const [zoom, unit] of [[100, 0.75], [150, 0.5], [200, 1 / 3], [300, 0.2], [400, 0.2], [800, 0.1]]) near(m.pointerUnit(zoom), unit, `unit at ${zoom} %`);
// draground-c1: more zooms, line drawing with the real mouse (fit page 115.2 % too)
for (const [zoom, unit] of [[25, 3], [50, 2], [56, 2], [60, 1], [75, 1], [115.2, 0.75], [125, 0.5]]) near(m.pointerUnit(zoom), unit, `unit at ${zoom} %`);
for (const [zoom, step] of [[56, 3], [100, 3], [200, 2], [400, 1.5]]) near(m.rulerTickStep(zoom), step, `ticks at ${zoom} %`);
// smartguides-c3: Shift-drops at 20 more zooms (artifacts/smartguides-c3-20261023/out/i5_*.log)
for (const [zoom, step] of [[125, 3], [137.5, 3], [143.75, 3], [146.875, 3], [150, 2], [175, 2], [250, 2], [262.5, 2], [268.75, 2], [275, 1.5], [300, 1.5], [350, 1.5], [500, 1.5],
  [537.5, 1], [575, 1], [650, 1], [800, 1], [900, 1], [1000, 0.5], [1200, 0.5]]) near(m.rulerTickStep(zoom), step, `ticks at ${zoom} %`);
// Folio's view scale (CSS px per point) to InDesign's zoom: 4/3 px per point is 100 %
near(m.inDesignZoom(4 / 3), 100, 'zoom');
near(m.inDesignZoom(1), 75, 'zoom of Folio actual size');
// guide drops measured at the fit-page zoom: 254.25, 252.75, 408 (nearest of the pointer positions that InDesign saw)
near(m.roundTo(254.3, 0.75), 254.25, 'round 254.3');
near(m.roundTo(408, 0.75), 408, 'round 408');
near(m.roundTo(256.2, 3), 255, 'tick 256.2');
near(m.roundTo(257.9, 3), 258, 'tick 257.9');
near(m.roundTo(401.2, 1.5), 400.5, 'tick 1.5 at 401.2');
near(m.roundTo(402.5, 1.5), 402, 'tick 1.5 at 402.5');
// InDesign rounds a dragged position DOWN (pairs measured at fit page / 100 %: exact -> dropped): 306.625 -> 306.75, 307.04 -> 306.75, 305.79 -> 305.25
near(m.snapPointer(306.625, 100), 306.75, 'down 306.625');
near(m.snapPointer(307.042, 100), 306.75, 'down 307.042');
near(m.snapPointer(309.2, 100), 309, 'down 309.2');
near(m.snapPointer(305.7, 100), 305.25, 'down 305.7');
near(m.snapPointer(306.0, 100), 306, 'on the grid');
near(m.snapPointer(306.208, 100), 306, 'down 306.208');
near(m.snapPointer(306.12, 200), 306, 'down at 200 %');
near(m.snapPointer(306.36, 200), 306.333333333333, 'down 306.36 at 200 %');
// a handle drag puts the edge down on the grid like a pointer position: far edge (right, bottom) floor(v / u + 0.3), near edge (left, top) + 0.2
// (docdrag-c1: off-grid edges dragged in and out, exact pixel map)
near(m.snapEdge(312.48, 100, true), 312, 'right edge 312.48');
near(m.snapEdge(313.44, 100, true), 313.5, 'right edge 313.44');
near(m.snapEdge(286.08, 100, true), 285.75, 'right edge 286.08 (dragged in)');
near(m.snapEdge(285.6, 100, true), 285.75, 'right edge 285.6');
near(m.snapEdge(187.04, 100, false), 186.75, 'left edge 187.04');
near(m.snapEdge(186.08, 100, false), 186, 'left edge 186.08');
near(m.snapEdge(300, 100, true), 300, 'an edge on the grid stays');
// a move puts the top-left corner down on the grid, rounded down
near(m.snapCorner(212.518, 100), 212.25, 'corner 212.518');
near(m.snapCorner(214.714, 100), 214.5, 'corner 214.714');
near(m.snapCorner(300, 100), 300, 'a corner on the grid stays');
near(m.snapCorner(300.4, 200), 300.333333333333, 'corner at 200 %');
// draground-c1: a Shift-drawn line (both pointer points already on the grid): the angle to a multiple of 45 degrees, the shorter way on a diagonal.
// Measured rows (fit page, unit 0.75): press (222, 333.75), pointer (234.75, 340.5) -> end (228.75, 340.5); (222.75, 333.75) -> (213.75, 342.75) for (-9, 9)
const sl = (x, y) => m.shiftLine([x, y]);
const same = (got, want, msg) => assert.ok(Math.abs(got[0] - want[0]) < 1e-9 && Math.abs(got[1] - want[1]) < 1e-9, `${msg}: ${got} != ${want}`);
same(sl(12.75, 6.75), [6.75, 6.75], 'diagonal: the shorter way');
same(sl(-12.75, 9), [-9, 9], 'diagonal up-left/down-left');
same(sl(15, -11.25), [11.25, -11.25], 'diagonal right-up');
same(sl(19.5, -6.75), [19.5, 0], 'along x (19 degrees)');
same(sl(-19.5, 6), [-19.5, 0], 'along x to the left');
same(sl(3, 15), [0, 15], 'along y (79 degrees)');
same(sl(-1.5, -17.25), [0, -17.25], 'along y upwards');
same(sl(0, 0), [0, 0], 'no way');
console.log('dragunit: ok');
