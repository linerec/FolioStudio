const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const preferenceKeys = require('../../shared/renderer-preferences.json');
const transfer = require('../electron/settings-transfer.cjs');

async function main() {
  const migrated = {
    'folio.editWith.programs': '["editor-a"]',
    'folio.layersSmallRows': '1',
    'folio.linksPanel.autoSize': '1',
    'folio.linksPanel.columns': '{"status":false,"page":true}',
    'folio.linksPanel.info': '0',
    'folio.objectStyles.smallRows': '1',
    'folio.pagesPanel.options': '{"pageSize":"l","parentSize":"s","top":"pages"}',
    'folio.pagesPanel.view': 'horizontal',
    'folio.panelOptions': '{"paragraph":false}',
    'folio.preflight.rowLimit': '50',
  };
  assert.deepEqual([...preferenceKeys].sort(), Object.keys(migrated).sort());
  const legacy = new Map([...Object.entries(migrated), ['folio.view.guides', 'false'], ['folio.theme', 'dark'], ['unknown.ui', '1']]);
  let stored = { preferences: { 'folio.theme': 'light', 'keep.setting': 17 } };
  let releaseSave;
  let event;
  let failMigration = true;
  const exports = {};
  const folio = {
    onEvent(callback) { event = callback; return () => { event = null; }; },
    async request({ migrate, values }) {
      if (migrate && failMigration) return { header: { ok: false, error: 'migration unavailable' } };
      for (const [key, value] of Object.entries(migrate || values || {})) {
        if (!migrate || !(key in stored.preferences)) stored.preferences[key] = value;
      }
      return { header: { ok: true, settings: structuredClone(stored) } };
    },
    saveSettings(patch) {
      stored = { ...stored, ...structuredClone(patch) };
      const response = structuredClone(stored);
      return new Promise(resolve => { releaseSave = () => resolve(response); });
    },
  };
  const source = fs.readFileSync(path.join(__dirname, '../renderer/src/user-settings.ts'), 'utf8');
  vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022, esModuleInterop: true } }).outputText,
    { exports, require: name => { assert.equal(name, '../../../shared/renderer-preferences.json'); return preferenceKeys; },
      console: { error: (...args) => assert.equal(args[0], 'user settings migration') },
      window: { folio }, localStorage: { getItem: k => legacy.get(k) ?? null, setItem: (k, v) => legacy.set(k, v), removeItem: k => legacy.delete(k) } });
  const { initializeUserSettings, userSettings, saveUserSettings } = exports;
  await initializeUserSettings();
  assert.equal(legacy.size, preferenceKeys.length + 3, 'a failed migration preserves every old value');
  failMigration = false;
  await initializeUserSettings();
  assert.equal(userSettings.getItem('folio.theme'), 'light');
  assert.equal(userSettings.getItem('folio.view.guides'), 'false');
  for (const [key, value] of Object.entries(migrated)) {
    assert.equal(userSettings.getItem(key), value, key);
    assert.equal(stored.preferences[key], value, key);
  }
  assert.deepEqual([...legacy.keys()], ['unknown.ui']);
  const changes = [];
  const unsubscribe = userSettings.subscribe(() => changes.push(userSettings.getItem('folio.layersSmallRows')));
  stored.preferences['folio.stepRepeat'] = '{"h":13}';
  event({event:'user-settings', detail:{settings:structuredClone(stored)}});
  assert.equal(userSettings.getItem('folio.stepRepeat'), '{"h":13}', 'external preferences reach existing readers');
  stored.preferences['folio.layersSmallRows'] = '0';
  event({event:'user-settings', detail:{settings:structuredClone(stored)}});
  assert.deepEqual(changes, ['1', '0']);
  unsubscribe();
  userSettings.setItem('folio.layersSmallRows', '1');
  assert.equal(stored.preferences['folio.layersSmallRows'], '1');
  assert.equal(legacy.has('folio.layersSmallRows'), false);
  userSettings.setItem('unknown.ui', '0');
  assert.equal(legacy.get('unknown.ui'), '0');

  const previous = structuredClone(stored);
  const pending = saveUserSettings({ preferences: { ...previous.preferences, 'other.setting': true } }, previous);
  userSettings.setItem('folio.view.guides', 'true');
  event({event:'user-settings', detail:{settings:previous}});
  assert.equal(userSettings.getItem('folio.view.guides'), 'true', 'an earlier event preserves a pending local change');
  releaseSave();
  const saved = await pending;
  assert.equal(userSettings.getItem('folio.view.guides'), 'true', 'a delayed save response must preserve newer toggles');
  const next = saveUserSettings({ preferences: { ...saved.preferences, 'other.setting': false } }, saved);
  releaseSave();
  await next;
  assert.equal(stored.preferences['folio.view.guides'], 'true', 'later preferences saves must preserve the newer toggle');
  assert.equal(stored.preferences['keep.setting'], 17);

  const key = 'folio.editWith.programs';
  const portable = JSON.parse(transfer.encode(stored));
  const preferences = portable.settings.preferences;
  assert.equal(preferences[key], undefined);
  assert.equal(preferences['folio.layersSmallRows'], '1');
  preferences[key] = '["other-editor"]';
  const imported = transfer.decode(JSON.stringify(portable));
  assert.equal(imported.preferences[key], undefined);
  assert.equal(stored.preferences[key], migrated[key]);
  console.log('PASS user settings: migration, subscriptions, overlapping saves, and local editor paths');
}
main().catch(error => { console.error(error); process.exitCode = 1; });
