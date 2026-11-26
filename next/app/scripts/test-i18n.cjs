// No extra dependencies: TypeScript VM modules exercise locale switching before/after imports.
const assert = require('node:assert/strict'), fs = require('node:fs'), path = require('node:path'), vm = require('node:vm');
const { root, runtime, inspect } = require('./i18n-source.cjs');
require('./test-engine-messages.cjs');
const { load, document } = runtime('ko-KR', 'MacIntel');
const i18n = load(path.join(root, 'i18n'));
const { ko } = load(path.join(root, 'i18n/ko')), { en } = load(path.join(root, 'i18n/en'));
const { SPECS, buildMenus, PANEL_MENUS, CONTEXT_MENUS } = load(path.join(root, 'ui/menudef'));
for (const key of Object.keys(ko)) assert.ok(i18n.hasTranslation(key), key);
for (const [key, english] of Object.entries(en)) {
  assert.equal(typeof english, 'string', key);
  assert.ok(english.length, key);
  const placeholders = s => [...s.matchAll(/\{(\w+)\}/g)].map(m => m[1]).sort().join(',');
  assert.equal(placeholders(english), placeholders(ko[key] ?? key), key);
}
assert.equal(i18n.currentLanguage(), 'ko');
assert.equal(buildMenus([])[0].label, '파일');
i18n.setLanguage('en');
assert.equal(document.documentElement.lang, 'en');
assert.equal(buildMenus([])[0].label, 'File');
assert.equal(SPECS['file.place'].label, 'Place…');
assert.equal(SPECS['edit.findChange'].label, 'Find/Change…');
assert.equal(i18n.tr('닫기 (Ctrl+W)'), 'Close (⌘W)');
assert.equal(i18n.actionLabel('글 입력'), 'Typing');
assert.equal(i18n.actionLabel('사용자 문서 제목'), '사용자 문서 제목');
const recent = '사용자 프로젝트 — e\u0301';
const menus = buildMenus([{ cmd: 'recent.0' }]);
function walk(items) {
  for (const item of items) {
    if (item.sub) { assert.ok(!/[가-힣]/.test(item.label), item.label); walk(item.sub); }
    if (item.cmd && !item.cmd.startsWith('recent.') && SPECS[item.cmd]) assert.ok(!/[가-힣]/.test(SPECS[item.cmd].label), item.cmd);
  }
}
for (const m of menus) walk(m.items);
for (const entries of [...Object.values(PANEL_MENUS), ...Object.values(CONTEXT_MENUS)]) walk(entries);
assert.equal(i18n.tr('"{0}"을(를) 닫기 전에 변경 내용을 저장하시겠습니까?', { 0: recent }), `Save changes to “${recent}” before closing?`);
const missing = '아직 영어가 없는 문장 {0}';
assert.equal(i18n.hasTranslation(missing), false);
assert.equal(i18n.tr(missing, { 0: recent }), `아직 영어가 없는 문장 ${recent}`);
for (const key of ['constructor', '__proto__', 'toString']) assert.equal(i18n.tr(key), key);
i18n.setLanguage('ko');
i18n.setLanguage('system', 'ko-KR'); assert.equal(i18n.currentLanguage(), 'ko');
i18n.setLanguage('system', 'fr-FR'); assert.equal(i18n.currentLanguage(), 'en');
i18n.setLanguage('ko', 'en-US'); assert.equal(i18n.currentLanguage(), 'ko');
// Every migrated literal message key must exist (dynamic command/menu keys checked above).
const checked = ['App.tsx', 'project.tsx', 'package.tsx', 'print.tsx', 'ui/docview.tsx', 'ui/menu.tsx', 'ui/dialog-frame.tsx'];
for (const file of checked) {
  for (const { key } of inspect(path.join(root, file)).calls) assert.ok(i18n.hasTranslation(key), `${file}: ${key}`);
}
// Native Mac menus follow the same preference and keep the deliberate Command+H policy.
let template;
const native = require('../electron/i18n.cjs'), moduleObject = { exports: {} };
vm.runInNewContext(fs.readFileSync(path.resolve(__dirname, '../electron/mac.cjs'), 'utf8'), {
  module: moduleObject, process,
  require: n => n === 'electron' ? { app: {}, shell: {}, Menu: {
    buildFromTemplate: t => t, setApplicationMenu: t => { template = t; },
  } } : require(n),
});
for (const lang of ['ko', 'en']) {
  moduleObject.exports.menu('/diagnostics', native.translator(lang, 'en-US'));
  assert.equal(template[1].label, lang === 'ko' ? '창' : 'Window');
  assert.equal(template[2].label, lang === 'ko' ? '도움말' : 'Help');
  const hide = template[0].submenu.find(it => it.click);
  assert.equal(hide.accelerator, undefined);
}
assert.deepEqual(Object.keys(native.ko), Object.keys(native.en));
// The audit must detect regressions, including in a new file and in deferred files.
const { collect } = require('./audit-i18n.cjs');
const good = "import { tr } from '../i18n'; export const a = <div>{tr('그리기 도구')}</div>;";
assert.equal(collect([{ file: 'ui/new-i18n-test.tsx', text: good }]).violations.length, 0);
const bad = "import { t as translate } from '../i18n'; export const a = <div title='새 제목'>새 표시{translate('표에 없는 문장 {0}', {0: 3})}</div>;";
const failures = collect([{ file: 'ui/new-i18n-test.tsx', text: bad }]).violations;
assert.ok(failures.some(v => v.issue === 'unwrapped' && v.text === '새 제목'));
assert.ok(failures.some(v => v.issue === 'unwrapped' && v.text === '새 표시'));
assert.ok(failures.some(v => v.issue === 'missing-English' && v.source === '표에 없는 문장 {0}'));
assert.ok(collect([{ file: 'ui/control.tsx', text: "export const a = '새로 추가한 컨트롤';" }]).violations.some(v => v.text === '새로 추가한 컨트롤'));
console.log(`i18n: ${Object.keys(en).length} complete translations, ${Object.keys(SPECS).length} commands; source fallback, live language switching, placeholders, native Mac menus PASS`);
