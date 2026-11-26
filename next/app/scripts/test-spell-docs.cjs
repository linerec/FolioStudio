// Run with node next/app/scripts/test-spell-docs.cjs; uses only the installed TypeScript compiler.
// Check Spelling > Search: All Documents. The order the documents are walked in (renderer/src/features/spelling/spelling-docs.ts) and the thin relay of the
// main process (electron/spell-docs.cjs): the window list, the read-only queries asked of another window's engine, the hand-over.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const relay = require('../electron/spell-docs.cjs');

const docs = {};
vm.runInNewContext(ts.transpileModule(fs.readFileSync(path.join(__dirname, '../renderer/src/features/spelling/spelling-docs.ts'), 'utf8'),
  { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText, { exports: docs });

// the module runs in its own context: compare plain values
const plain = (v) => JSON.parse(JSON.stringify(v));
const win = (window, title, self = false) => ({ window, title, self });

// --- the round of documents (InDesign 2025: open order, round once from the document the check started in) -------------------
const abc = [win(3, 'a.folio'), win(5, 'b.folio', true), win(9, 'c.folio')];
assert.deepEqual(plain(docs.documentRing(abc)), [9, 3], 'from b: c, then a');
assert.deepEqual(plain(docs.documentRing(abc, 'backward')), [3, 9], 'backwards from b: a, then c');
assert.deepEqual(plain(docs.documentRing([win(1, 'a', true), win(2, 'b'), win(3, 'c')])), [2, 3], 'from the first');
assert.deepEqual(plain(docs.documentRing([win(1, 'a'), win(2, 'b'), win(3, 'c', true)])), [1, 2], 'from the last: round to the first ones');
assert.deepEqual(plain(docs.documentRing([win(1, 'a'), win(2, null), win(3, 'c', true), win(4, 'd')])), [4, 1], 'a window without a document is not part of the round');
assert.deepEqual(plain(docs.documentRing([win(1, 'a', true)])), [], 'one document: nothing after it');
assert.deepEqual(plain(docs.documentRing([win(1, 'a'), win(2, 'b'), win(3, 'c'), win(4, 'd', true)], 'backward')), [3, 2, 1], 'four documents backwards');

// --- the next document with a flagged word (the ones without any are passed over) -----------------------------------------------
(async () => {
  const asked = [];
  const answers = { 4: { ok: true, header: { ok: true, issues: [] } }, 5: { ok: false, error: 'no document open' }, 6: { ok: true, header: { ok: true, issues: [{ word: 'xqz' }] } }, 7: { ok: true, header: { ok: true, issues: [{ word: 'y' }] } } };
  const ask = async (w, message) => { asked.push([w, message.cmd, message.scope, message.ignored]); return answers[w]; };
  const found = await docs.nextDocumentWithIssues([4, 5, 6, 7], { ignored: ['q'] }, ask);
  assert.deepEqual(plain(found), { window: 6, after: [7] }, 'a clean document and one without a document are passed over');
  assert.deepEqual(asked.map(a => a[0]), [4, 5, 6], 'nothing after the one found is asked');
  assert.ok(asked.every(a => a[1] === 'spellCheck' && a[2] === 'document' && a[3][0] === 'q'), 'each document is asked as a whole, with the skipped words');
  assert.equal(await docs.nextDocumentWithIssues([4, 5], {}, ask), null, 'no document has a flagged word: the check is complete');
  assert.equal(await docs.nextDocumentWithIssues([], {}, ask), null, 'nothing to go on to');

  // --- the relay of the main process -------------------------------------------------------------------------------------------
  const handlers = {};
  const ipcMain = { handle: (name, fn) => { handlers[name] = fn; } };
  const sent = [];
  const shown = [];
  const ctx = (id, live = true) => ({ win: { isDestroyed: () => !live, isMinimized: () => false, show: () => shown.push(['show', id]), focus: () => shown.push(['focus', id]), webContents: { isDestroyed: () => !live, send: (...a) => sent.push([id, ...a]) } } });
  const contexts = new Map([[9, ctx(9)], [3, ctx(3)], [5, ctx(5)], [7, ctx(7, false)]]);
  const requests = [];
  const request = async (message, c) => { requests.push([message.cmd, [...contexts.entries()].find(([, v]) => v === c)[0]]); return { header: { ok: true, issues: [] } }; };
  const activated = [];
  const appwin = { state: { docs: { 3: { title: 'a.folio' }, 5: { title: null }, 9: { title: 'c.folio' } } }, command: (c, caller) => { activated.push([c, caller]); return { ok: true }; } };
  relay.install({ ipcMain, contexts, request, appwin, log: () => {} });
  const e = (id) => ({ sender: { id } });
  assert.deepEqual(await handlers['folio:spell-windows'](e(5)), [
    { window: 3, title: 'a.folio', self: false }, { window: 5, title: null, self: true }, { window: 9, title: 'c.folio', self: false }], 'windows in the order they were made, closed ones left out');
  assert.deepEqual(await handlers['folio:window-query'](e(3), 9, { cmd: 'spellCheck', scope: 'document' }), { ok: true, header: { ok: true, issues: [] } });
  assert.deepEqual(requests, [['spellCheck', 9]], 'the query went to the engine of window 9');
  assert.deepEqual(await handlers['folio:window-query'](e(3), 9, { cmd: 'save' }), { ok: false, error: 'notRelayed' }, 'only the read-only spelling queries');
  assert.deepEqual(await handlers['folio:window-query'](e(3), 9, { cmd: 'close' }), { ok: false, error: 'notRelayed' });
  assert.deepEqual(await handlers['folio:window-query'](e(3), 7, { cmd: 'spellCheck' }), { ok: false, error: 'unknownWindow' }, 'a closed window');
  assert.deepEqual(await handlers['folio:window-query'](e(3), 99, { cmd: 'spellCheck' }), { ok: false, error: 'unknownWindow' });
  assert.deepEqual(await handlers['folio:spell-handoff'](e(3), 9, { rest: [3], ignored: ['q'] }), { ok: true, window: 9 });
  assert.deepEqual(activated, [[{ cmd: 'activate', doc: 9 }, 3]], 'the window comes to the front');
  assert.deepEqual(shown, [['show', 9], ['focus', 9]], 'and is shown and focused');
  assert.deepEqual(sent, [[9, 'folio:event', { event: 'spell-handoff', handoff: { rest: [3], ignored: ['q'] }, from: 3 }]], 'and is told what the check carries');
  assert.deepEqual(await handlers['folio:spell-handoff'](e(3), 7, {}), { ok: false, error: 'unknownWindow' });
  console.log('PASS test-spell-docs');
})().catch((error) => { console.error(error); process.exit(1); });
