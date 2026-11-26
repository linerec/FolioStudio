const assert = require('node:assert/strict'), path = require('node:path');
const { root, runtime } = require('./i18n-source.cjs');
const { load } = runtime('en-US');
const { setLanguage } = load(path.join(root, 'i18n'));
const { engineMessage } = load(path.join(root, 'i18n/engine-message'));
const catalogue = load(path.join(root, 'i18n/engine-messages.en')).default;
const message = { message: '빈 쪽: 사용자 쪽 이름', messageId: 'preflight.page.blank', messageArgs: { name: '사용자 쪽 이름' } };
setLanguage('en');
assert.equal(engineMessage(message), 'Blank page: 사용자 쪽 이름');
assert.equal(engineMessage({ message: 'old engine' }), 'old engine');
assert.equal(engineMessage({ message: 'unknown', messageId: '__proto__' }), 'unknown');
assert.equal(engineMessage({ messageId: 'preflight.bleed', messageArgs: { edges: [
  { messageId: 'preflight.edge.top' }, { messageId: 'preflight.edge.right' },
] } }), 'Touches the trim edge but does not reach the bleed (top, right)');
assert.equal(engineMessage({ messageId: 'preflight.link.missing', messageArgs: {
  name: '문서{what}.png', what: { messageId: 'preflight.preview.image' },
} }), 'Missing linked file: 문서{what}.png — printed using the low-resolution preview stored in the document');
assert.equal(engineMessage({ messageId: 'package.destination.missingParent', messageArgs: { path: '한글 {path}' } }), 'The destination folder does not exist: 한글 {path}');
// The preload (electron/engine-message.cjs) turns every failure answer into the screen's language with the same rules.
const preload = require('../electron/engine-message.cjs');
const failure = () => ({ header: { ok: false, code: 'invalid', error: '문단 스타일을 찾을 수 없습니다: 본문 {스타일}', messageId: 'edit.style.paragraphNotFound', messageArgs: { id: '본문 {스타일}' } } });
for (const sample of [message, { message: 'x', messageId: 'package.diskSpace', messageArgs: { need: '12', free: '3' } },
  { messageId: 'package.leftover', messageArgs: { message: { message: 'k', messageId: 'package.cancelled', messageArgs: {} }, staging: 'D:/임시' } }]) {
  assert.equal(preload.render(catalogue, sample.message, sample.messageId, sample.messageArgs), engineMessage(sample));
}
assert.equal(preload.localize(failure(), { language: 'en', english: catalogue }).header.error, 'Paragraph style not found: 본문 {스타일}');
assert.equal(preload.localize(failure(), { language: 'ko', english: {} }).header.error, '문단 스타일을 찾을 수 없습니다: 본문 {스타일}');
assert.equal(preload.localize(failure(), { language: 'en', english: null }).header.error, '문단 스타일을 찾을 수 없습니다: 본문 {스타일}');
const nested = { header: { ok: true, plan: { dir: { ok: false, error: '취소했습니다', messageId: 'package.cancelled', messageArgs: {} } } } };
assert.equal(preload.localize(nested, { language: 'en', english: catalogue }).header.plan.dir.error, 'Cancelled');
const unknown = { header: { ok: false, error: '모르는 문장', messageId: 'no.such.id' } };
assert.equal(preload.localize(unknown, { language: 'en', english: catalogue }).header.error, '모르는 문장');
const success = { header: { ok: true, error: '취소했습니다', messageId: 'package.cancelled' } };
assert.equal(preload.localize(success, { language: 'en', english: catalogue }).header.error, '취소했습니다');
assert.equal(preload.localize({ header: { ok: false, error: '일반 오류' } }, { language: 'en', english: catalogue }).header.error, '일반 오류');
setLanguage('ko');
assert.equal(engineMessage(message), message.message);
const { collectEngineMessages } = require('./audit-engine-messages.cjs');
assert.equal(collectEngineMessages().violations.length, 0);
const incomplete = { ...catalogue }; delete incomplete['preflight.page.blank'];
assert.ok(collectEngineMessages(incomplete).violations.some(v => v.key === 'preflight.page.blank'));
console.log(`engine messages: ${Object.keys(catalogue).length} English templates; user data, nested arguments, legacy responses, language switch, missing-English detection PASS`);
