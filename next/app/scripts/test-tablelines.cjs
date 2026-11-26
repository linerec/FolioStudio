// Run with node next/app/scripts/test-tablelines.cjs; uses only the installed TypeScript compiler.
// Where the pointer is on a table's lines and how far a line was dragged (ui/tablelinehit.ts, the Type tool's table line drag).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/tablelinehit.ts'), 'utf8');
const m = {};
vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: m });
const near = (a, b, msg) => assert.ok(Math.abs(a - b) < 1e-9, `${msg}: ${a} != ${b}`);

// a 3 x 2 table at (60, 100) on page 0: columns 100 wide, rows 30 high; frame not turned
const info = {
  table: 'u1', columns: 3, rows: 2,
  pieces: [{ frame: 'f', page: 0, columns: [1, 2, 3].map((k) => ({ line: k, from: [60 + 100 * k, 100], to: [60 + 100 * k, 160], size: 100 })),
    rows: [1, 2].map((k) => ({ line: k, from: [60, 100 + 30 * k], to: [360, 100 + 30 * k], size: 30 })) }],
};
let hit = m.lineAt(info, 0, [160.5, 120], 3);
assert.ok(hit && hit.columns && hit.line === 1 && !hit.outer, 'column line 1');
hit = m.lineAt(info, 0, [360.4, 120], 3);
assert.ok(hit && hit.columns && hit.line === 3 && hit.outer, 'the right edge is the outer line');
hit = m.lineAt(info, 0, [200, 129.5], 3);
assert.ok(hit && !hit.columns && hit.line === 1 && !hit.outer, 'row line 1');
hit = m.lineAt(info, 0, [200, 160.2], 3);
assert.ok(hit && !hit.columns && hit.line === 2 && hit.outer, 'the bottom edge is the outer line');
assert.equal(m.lineAt(info, 0, [200, 145], 3), null, 'the middle of a cell is no line');
assert.equal(m.lineAt(info, 1, [160, 120], 3), null, 'another page');
assert.equal(m.lineAt({ table: null, pieces: [] }, 0, [160, 120], 3), null, 'no table');
// a crossing: the nearer of the two wins
hit = m.lineAt(info, 0, [162, 129], 3);
assert.ok(hit && !hit.columns, 'nearer to the row line');
// the drag moves along the frame's x axis for a column line, y for a row line
const col = m.lineAt(info, 0, [160, 120], 3), row = m.lineAt(info, 0, [200, 130], 3);
near(m.dragAlong(col, [160, 120], [181.5, 140]), 21.5, 'column drag');
near(m.dragAlong(row, [200, 130], [260, 112]), -18, 'row drag');
assert.ok(m.axisParallel(col) && m.axisParallel(row), 'not turned');
const s = m.movedSeg(col, 10);
near(s.from[0], 170, 'moved x'); near(s.to[1], 160, 'moved y');
// a frame turned by 90 degrees clockwise: the column lines run along the page's x axis, the drag follows the frame's x axis (page y)
const turned = { table: 'u2', columns: 2, rows: 1, pieces: [{ frame: 'g', page: 0, columns: [{ line: 1, from: [300, 100], to: [200, 100], size: 50 }, { line: 2, from: [300, 150], to: [200, 150], size: 50 }], rows: [{ line: 1, from: [300, 100], to: [300, 150], size: 50 }] }] };
const tc = m.lineAt(turned, 0, [250, 100.5], 3);
assert.ok(tc && tc.columns && tc.line === 1, 'turned column line');
near(m.dragAlong(tc, [250, 100], [240, 112]), 12, 'turned drag follows the frame axis (page y here)');
console.log('tablelines: ok');
