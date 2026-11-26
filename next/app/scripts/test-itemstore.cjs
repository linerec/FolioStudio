// node next/app/scripts/test-itemstore.cjs
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/inspect.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const requests = [];
const m = {};
vm.runInNewContext(js, { exports: m, console: { error() {} }, window: { folio: {
  request(q) { return new Promise((resolve, reject) => requests.push({ q, resolve, reject })); },
} } });
const snapshot = (id) => ({ page: 0, size: [600, 800], items: [{ id }] });
const reply = (request, items) => request.resolve({ header: { ok: true, items } });

(async () => {
  const store = new m.ItemStore();
  const first = snapshot('before');
  let pending = store.load(0);
  assert.equal(store.load(0), pending, 'initial readers share a request');
  reply(requests.shift(), first);
  await pending;
  const seen = [];
  store.subscribe(() => seen.push(store.peek(0)));

  store.invalidate([0]);
  assert.equal(store.peek(0), first, 'editing must not remove the displayed page');
  pending = store.load(0);
  assert.equal(requests.length, 1, 'refresh readers share the invalidation request');
  let settled = false;
  pending.then(() => { settled = true; });
  await Promise.resolve();
  assert.equal(settled, false, 'load must wait for fresh data, not return the displayed snapshot');
  const second = snapshot('after');
  reply(requests.shift(), second);
  assert.equal(await pending, second);
  assert.equal(store.peek(0), second);
  assert.ok(seen.every(Boolean), 'subscribers must never see a missing page during refresh');

  // Two edits can finish before the first page response arrives.
  store.invalidate([0]);
  const older = requests.shift();
  const olderLoad = store.load(0);
  store.invalidate([0]);
  const newer = requests.shift();
  pending = store.load(0);
  reply(older, first);
  assert.equal(await olderLoad, null, 'discard replies from before the latest edit');
  assert.equal(store.peek(0), second);
  assert.equal(store.load(0), pending);
  const third = snapshot('latest');
  reply(newer, third);
  await pending;
  assert.equal(store.peek(0), third);

  // An old failed request must not remove a newer pending request either.
  store.invalidate([0]);
  const failed = requests.shift();
  const failedLoad = store.load(0);
  store.invalidate([0]);
  const retry = requests.shift();
  pending = store.load(0);
  failed.reject(new Error('old request failed'));
  await failedLoad;
  assert.equal(store.load(0), pending);
  assert.equal(requests.length, 0);
  reply(retry, third);
  await pending;

  // A current failure retains the display but allows a fresh retry.
  store.invalidate([0]);
  pending = store.load(0);
  requests.shift().reject(new Error('temporary failure'));
  await pending;
  assert.equal(store.peek(0), third);
  pending = store.load(0);
  assert.equal(requests.length, 1);
  reply(requests.shift(), second);
  await pending;
  assert.equal(store.peek(0), second);

  // Opening another document clears the old display and ignores old responses.
  store.invalidate([0]);
  const oldDocument = requests.shift();
  const oldLoad = store.load(0);
  store.reset();
  assert.equal(store.peek(0), undefined);
  pending = store.load(0);
  reply(oldDocument, third);
  await oldLoad;
  assert.equal(store.peek(0), undefined);
  assert.equal(store.load(0), pending);
  reply(requests.shift(), first);
  await pending;
  assert.equal(store.peek(0), first);
  console.log('ItemStore: refresh, overlapping edits, failures and document reset PASS');
})().catch(e => { console.error(e); process.exitCode = 1; });
