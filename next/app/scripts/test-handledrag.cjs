// Run with node next/app/scripts/test-handledrag.cjs; uses only the installed TypeScript compiler.
// The same vectors run in the engine (cargo test -p folio-edit --test handledrag): the two must agree.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const dir = path.join(__dirname, '../renderer/src/ui');
const source = fs.readFileSync(path.join(dir, 'handledrag.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const m = {};
vm.runInNewContext(js, { exports: m, Math });
const vectors = JSON.parse(fs.readFileSync(path.join(dir, 'handledrag.vectors.json'), 'utf8'));

const close = (a, b, msg) => assert.ok(a.length === b.length && a.every((v, i) => Math.abs(v - b[i]) < 1e-9), `${msg}: ${JSON.stringify(a)} != ${JSON.stringify(b)}`);
for (const c of vectors.handleRect) {
  const got = m.handleBox(c.box, c.handle, c.by, c.keep, c.center);
  close(Array.from(got.rect), c.want, c.name);
  assert.deepEqual(Array.from(got.flip), c.flip, `${c.name}: flip`);
  close(Array.from(m.handleRect(c.box, c.handle, c.by, c.keep, c.center)), c.want, c.name);
}
for (const c of vectors.turnAngle) {
  const got = m.turnAngle(c.center, c.from, c.to, c.constrain);
  if (c.want === null) assert.equal(got, null, c.name);
  else assert.ok(Math.abs(got - c.want) < 1e-9, `${c.name}: ${got} != ${c.want}`);
}

console.log('handledrag: ok');
