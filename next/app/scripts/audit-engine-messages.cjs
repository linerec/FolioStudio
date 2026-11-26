// Engine metadata is outside the JSX audit: every emitted ID needs an English template.
const fs = require('node:fs'), path = require('node:path');
const { root, runtime } = require('./i18n-source.cjs');
function collectEngineMessages(catalogue) {
  const english = catalogue ?? runtime().load(path.join(root, 'i18n/engine-messages.en')).default;
  const crates = path.resolve(root, '../../../engine/crates');
  const files = dir => fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
    entry.isDirectory() ? files(path.join(dir, entry.name)) : entry.name.endsWith('.rs') ? [path.join(dir, entry.name)] : []);
  const violations = [], used = new Set();
  for (const crate of ['preflight', 'project', 'textimport', 'package']) {
    for (const file of files(path.join(crates, crate, 'src'))) {
      const text = fs.readFileSync(file, 'utf8');
      for (const match of text.matchAll(/"((?:preflight|import|textimport|package)\.[A-Za-z][A-Za-z0-9.]*)"/g)) {
        const key = match[1]; used.add(key);
        if (!Object.hasOwn(english, key) || !english[key].trim()) violations.push({
          issue: 'engine-missing-English', file: path.relative(root, file).replace(/\\/g, '/'),
          line: text.slice(0, match.index).split('\n').length, key,
        });
      }
    }
  }
  // the sentences the engine recognises at the wire (server/src/engine_messages.tsv: id, source files, Korean template)
  const table = path.join(crates, 'server/src/engine_messages.tsv');
  fs.readFileSync(table, 'utf8').split(/\r?\n/).forEach((row, index) => {
    const [key, , korean = ''] = row.split('\t');
    if (!key) return;
    used.add(key);
    const file = path.relative(root, table).replace(/\\/g, '/'), line = index + 1;
    if (!Object.hasOwn(english, key) || !english[key].trim()) return violations.push({ issue: 'engine-missing-English', file, line, key });
    const names = text => [...text.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort().join();
    if (names(korean) !== names(english[key])) violations.push({ issue: 'engine-placeholders-differ', file, line, key });
  });
  return { used: [...used], violations };
}
module.exports = { collectEngineMessages };
