// Run with node next/app/scripts/test-numberfield.cjs; uses only the installed TypeScript compiler.
// The shared number field's pure logic (ui/controls/numberfield.logic.ts) and the window's preview session queue
// (ui/controls/previewsession.ts).
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');

function load(file) {
  const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/controls', file), 'utf8');
  const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 } }).outputText;
  const m = { exports: {} };
  vm.runInNewContext(js, { exports: m.exports, module: m, Promise, Math, Number, String, Object, Array });
  return m.exports;
}
const L = load('numberfield.logic.ts');
const spec = { min: 0, max: 1000, step: 1, decimals: 3, suffix: ' pt' };

// display: trimmed digits and the suffix, blank for the mixed value, no "-0"
assert.equal(L.formatNumber(0.25, spec), '0.25 pt');
assert.equal(L.formatNumber(12, spec), '12 pt');
assert.equal(L.formatNumber(null, spec), '');
assert.equal(L.formatNumber(-0.0001, spec), '0 pt');
assert.equal(L.formatNumber(1 / 3, spec), '0.333 pt');
assert.equal(L.formatNumber(50, { decimals: 1, suffix: '%' }), '50%');

// typing: the suffix is optional, a comma is a decimal point, out of range or not a number is refused
assert.equal(L.parseNumber('8 pt', spec), 8);
assert.equal(L.parseNumber('8', spec), 8);
assert.equal(L.parseNumber('0,5 pt', spec), 0.5);
assert.equal(L.parseNumber('  12.5pt ', spec), 12.5);
assert.equal(L.parseNumber('abc', spec), null);
assert.equal(L.parseNumber('', spec), null);
assert.equal(L.parseNumber('-1', spec), null);
assert.equal(L.parseNumber('1001', spec), null);
assert.equal(L.parseNumber('1000', spec), 1000);
assert.equal(L.parseNumber('150 %', { suffix: ' %', min: 1, max: 1000 }), 150);
assert.equal(L.parseNumber('150%', { suffix: ' %', min: 1, max: 1000 }), 150);

// steps: one, Shift ten times, Alt a tenth, kept inside the range, float noise gone
assert.equal(L.stepSize(spec), 1);
assert.equal(L.stepSize(spec, { shift: true }), 10);
assert.equal(L.stepSize(spec, { alt: true }), 0.1);
assert.equal(L.stepSize(spec, { shift: true, alt: true }), 1);
assert.equal(L.stepValue(5, 1, spec), 6);
assert.equal(L.stepValue(5, -1, spec), 4);
assert.equal(L.stepValue(5, 1, spec, { shift: true }), 15);
assert.equal(L.stepValue(0.2, 1, spec, { alt: true }), 0.3, '0.2 + 0.1 is 0.3, not 0.30000000000000004');
assert.equal(L.stepValue(0.4, -1, spec), 0, 'kept at the minimum');
assert.equal(L.stepValue(999.5, 1, spec), 1000, 'kept at the maximum');
// a mixed field is given a value by the first press, from the anchor (or zero inside the range)
assert.equal(L.stepValue(null, 1, spec), 1);
assert.equal(L.stepValue(null, -1, spec), 0);
assert.equal(L.stepValue(null, 1, spec, {}, 4), 5);
assert.equal(L.stepValue(null, 1, { ...spec, min: 1, max: 500 }, {}, undefined), 2, 'zero is outside 1..500: the minimum is the start');

// scrubbing: two pixels a step, Shift ten times, either way, kept inside the range
assert.equal(L.scrubValue(10, 20, spec), 20);
assert.equal(L.scrubValue(10, -8, spec), 6);
assert.equal(L.scrubValue(10, 20, spec, { shift: true }), 110);
assert.equal(L.scrubValue(10, 20, spec, { alt: true }), 11);
assert.equal(L.scrubValue(10, -100, spec), 0);
assert.equal(L.scrubValue(null, 10, spec, {}, 100), 105);
assert.equal(L.scrubValue(10, 0, spec), 10);

// the wheel
assert.equal(L.wheelDirection(-120), 1);
assert.equal(L.wheelDirection(120), -1);
assert.equal(L.wheelDirection(0), 0);

// the list: the arrows stop at the ends, the first press from nothing goes to the first (down) or last (up) item
assert.equal(L.listMove(-1, 'ArrowDown', 5), 0);
assert.equal(L.listMove(-1, 'ArrowUp', 5), 4);
assert.equal(L.listMove(2, 'ArrowDown', 5), 3);
assert.equal(L.listMove(4, 'ArrowDown', 5), 4);
assert.equal(L.listMove(0, 'ArrowUp', 5), 0);
assert.equal(L.listMove(2, 'Home', 5), 0);
assert.equal(L.listMove(2, 'End', 5), 4);
assert.equal(L.listMove(2, 'x', 5), 2);
assert.equal(L.listMove(-1, 'ArrowDown', 0), -1);
assert.equal(L.indexOfValue([0.25, 0.5, 1, 2], 1), 2);
assert.equal(L.indexOfValue([0.25, 0.5, 1, 2], 0.75), -1);
assert.equal(L.indexOfValue([1], null), -1);

// a commit of the value the field already had gives the preview back instead
assert.equal(L.commitAction(3, 1), 'commit');
assert.equal(L.commitAction(1, 1), 'cancel');
assert.equal(L.commitAction(1, null), 'commit', 'a mixed field takes the value');
assert.equal(L.commitAction(null, 1), 'cancel');

// --- the preview session queue --------------------------------------------------------------------------------------
const { PreviewSession } = load('previewsession.ts');
// arrays made inside the vm have another prototype: compare through JSON
const same = (a, b, msg) => assert.deepEqual(JSON.parse(JSON.stringify(a)), b, msg);
const tick = () => new Promise((r) => setTimeout(r, 0));
// entries (the dialogs' and panels' number boxes: controls/entries.ts)
const E = load('entries.ts');
assert.equal(E.parseInteger('3'), 3);
assert.equal(E.parseInteger(' 12 '), 12);
assert.equal(E.parseInteger('3.5'), null);
assert.equal(E.parseInteger(''), null);
assert.equal(E.parseInteger('abc'), null);
assert.equal(E.parseNumeric('12,5'), 12.5);
assert.equal(E.parseNumeric('50%'), 50);
assert.equal(E.parseNumeric('-3'), -3);
assert.equal(E.parseNumeric(''), null);
assert.equal(E.parseNumeric('x'), null);
assert.equal(E.formatNumeric(0.30000000000000004), '0.3');
assert.equal(E.formatNumeric(12), '12');
assert.equal(E.formatNumeric(1 / 3), '0.3333');
assert.equal(E.formatInteger(3), '3');
assert.equal(E.formatInteger(2.6), '3');
// one arrow press of a length: a pica or cicero steps a point, any other unit one of itself (the unit comes only through format / parse)
const mm = { format: (pt) => `${Math.round((pt * 25.4 / 72) * 1000) / 1000} mm`, parse: (t) => { const v = parseFloat(t); return Number.isFinite(v) ? v * 72 / 25.4 : null; } };
assert.ok(Math.abs(E.stepOfLength(mm.format, mm.parse) - 72 / 25.4) < 1e-9, 'a millimetre field steps a millimetre');
const picas = { format: (pt) => `${Math.floor(pt / 12)}p${pt % 12}`, parse: (t) => { const v = parseFloat(t); return Number.isFinite(v) ? v * 12 : null; } };
assert.equal(E.stepOfLength(picas.format, picas.parse), 1, 'a pica field steps a point');
assert.equal(E.stepOfLength((pt) => `${pt} pt`, (t) => parseFloat(t)), 1, 'points step a point');
assert.equal(E.stepOfLength((pt) => `${pt}`, () => null), 1, 'a parse that fails falls back to 1');

(async () => {
  const log = [];
  let release = null;
  const send = (label, commands) => {
    log.push([label, commands.map((c) => c.op + (c.commands ? JSON.stringify(c.commands) : ''))]);
    return new Promise((r) => { release = r; });
  };
  const s = new PreviewSession(send);
  assert.equal(s.open, false);
  assert.equal(await s.commit(), null, 'nothing to commit');
  assert.equal(await s.cancel(), null, 'nothing to cancel');
  // many values while the engine answers the first: only the last is sent after it
  s.apply('획 두께', [{ v: 1 }]);
  s.apply('획 두께', [{ v: 2 }]);
  s.apply('획 두께', [{ v: 3 }]);
  assert.equal(log.length, 1, 'one request at a time');
  same(log[0], ['획 두께', ['previewApply[{"v":1}]']]);
  release(1); await tick(); await tick();
  assert.equal(log.length, 2);
  same(log[1], ['획 두께', ['previewApply[{"v":3}]']], 'the values between are skipped');
  assert.equal(s.open, true);
  // a commit waits for the request on its way, then goes
  let done = false;
  const commit = s.commit().then(() => { done = true; });
  await tick();
  assert.equal(log.length, 2);
  release(1); await tick(); await tick();
  assert.equal(log.length, 3);
  same(log[2], ['', ['previewCommit']]);
  assert.equal(s.open, false, 'closed once the commit is sent');
  release(1); await commit;
  assert.equal(done, true);
  // a cancel drops the value that waits and gives the document back
  s.apply('x', [{ v: 1 }]); s.apply('x', [{ v: 2 }]);
  assert.equal(log.length, 4);
  const cancel = s.cancel();
  release(1); await tick(); await tick();
  assert.equal(log.length, 5);
  same(log[4], ['', ['previewCancel']], 'the waiting value is never shown');
  release(1); await cancel;
  assert.equal(s.open, false);
  // discard forgets the session without a request (an edit follows and the engine takes the preview back first)
  s.apply('y', [{ v: 1 }]);
  s.discard();
  assert.equal(s.open, false);
  release(1); await tick();
  assert.equal(await s.commit(), null);
  assert.equal(log.length, 6);
  // a failing send never stops the queue
  const failing = new PreviewSession(() => Promise.reject(new Error('boom')));
  failing.apply('z', [{ v: 1 }]); failing.apply('z', [{ v: 2 }]);
  await failing.idle();
  // a listener hears the reply to the request it asked for, not to one that was replaced before it was sent
  const heard = [];
  let free = null;
  const listened = new PreviewSession(() => new Promise((r) => { free = r; }));
  listened.apply('a', [{ v: 1 }], (r) => heard.push(['first', r]));
  listened.apply('a', [{ v: 2 }], (r) => heard.push(['second', r]));
  listened.apply('a', [{ v: 3 }], (r) => heard.push(['third', r]));
  free({ rev: 1 }); await tick(); await tick();
  free({ rev: 2 }); await tick(); await tick();
  same(heard, [['first', { rev: 1 }], ['third', { rev: 2 }]], 'the replaced value has no reply to hear');
  // a listener that throws does not stop the queue
  const throwing = new PreviewSession(() => Promise.resolve({ rev: 1 }));
  throwing.apply('b', [{ v: 1 }], () => { throw new Error('listener'); });
  throwing.apply('b', [{ v: 2 }]);
  await throwing.idle();
  console.log('numberfield logic and preview queue: all passed');
})().catch((e) => { console.error(e); process.exit(1); });
