// Audit with the TypeScript AST. New unwrapped strings and missing English fail.
// Deferred A/B work is enumerated by exact source/count in i18n-deferred.json.
const fs = require('node:fs'), path = require('node:path');
const { root, files, inspect, runtime } = require('./i18n-source.cjs');
const { collectEngineMessages } = require('./audit-engine-messages.cjs');
const baselinePath = path.join(__dirname, 'i18n-deferred.json');
const out = path.resolve(root, '../../../../artifacts/i18n-source-20261020');
const identity = e => JSON.stringify([e.file, e.kind, e.text]);
function excluded(file) {
  // Catalogues are not UI call sites. Generated menu coverage is tested separately.
  return file.startsWith('i18n/') && file !== 'i18n/settings.tsx' ||
    ['ui/menudef.ts', 'ui/menutree.ts'].includes(file);
}
function collect(extra = []) {
  const { load } = runtime(), { hasTranslation } = load(path.join(root, 'i18n'));
  // Fragments were recorded in a CRLF checkout; sources are read with LF (i18n-source.cjs).
  const baseline = JSON.parse(fs.readFileSync(baselinePath, 'utf8').replace(/\\r\\n/g, '\\n'));
  const pending = new Map(baseline.entries.map(e => [identity(e), e]));
  const seen = new Map(), unwrapped = [], missing = [], legacy = [], violations = [];
  const sources = files().map(p => ({ file: path.relative(root, p).replace(/\\/g, '/') })).concat(extra);
  for (const { file, text } of sources) {
    if (excluded(file)) continue;
    const parsed = inspect(path.join(root, file), text);
    for (const row of parsed.raw) {
      const id = identity({ file, ...row }), allowed = pending.get(id);
      const n = (seen.get(id) ?? 0) + 1; seen.set(id, n);
      const deferred = !!allowed && n <= allowed.count;
      const entry = { file, ...row, deferred, reason: deferred ? allowed.reason : 'New unwrapped UI source', missingEnglish: !hasTranslation(row.text) };
      unwrapped.push(entry);
      if (!deferred) violations.push({ issue: 'unwrapped', ...entry });
    }
    for (const call of parsed.calls) {
      if (!hasTranslation(call.key)) {
        const entry = { file, line: call.line, source: call.key, issue: 'missing-English' };
        missing.push(entry); violations.push(entry);
      }
      if (/^ui\.[0-9a-f]{10}$/.test(call.key)) {
        const deferred = (baseline.legacy[file] ?? []).includes(call.key);
        const entry = { file, line: call.line, key: call.key, deferred };
        legacy.push(entry);
        if (!deferred) violations.push({ issue: 'new-hash-key', ...entry });
      }
    }
  }
  const engine = collectEngineMessages();
  violations.push(...engine.violations);
  return { unwrapped, missing, legacy, engine, violations };
}
function main() {
  const report = collect();
  fs.mkdirSync(out, { recursive: true });
  fs.writeFileSync(path.join(out, 'i18n-audit.json'), JSON.stringify(report, null, 2) + '\n');
  const inventory = report.unwrapped.filter(e => e.reason !== 'Language autonym' && e.reason !== 'Engine-suggested folder suffix contract');
  fs.writeFileSync(path.join(out, 'panel-strings.json'), JSON.stringify({
    note: 'Deferred by user: A/B own panels, controls and document window. Exact source/count allowances only; new strings fail.',
    entries: inventory, legacyWindowCalls: report.legacy,
  }, null, 2) + '\n');
  for (const v of report.violations.slice(0, 12)) console.error(`${v.file}:${v.line}: ${v.issue}: ${v.text ?? v.source ?? v.key}`);
  const strict = process.argv.includes('--strict') && (inventory.length || report.legacy.length);
  console.log(`i18n audit: ${report.violations.length} violations (${report.missing.length} translated sources missing English); ${inventory.length} deferred UI fragments, ${report.legacy.length} protected window calls.`);
  console.log(path.join(out, 'i18n-audit.json'));
  if (report.violations.length || strict) process.exitCode = 1;
}
module.exports = { collect, excluded, identity };
if (require.main === module) main();
