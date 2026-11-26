const fs = require('fs');
const KEYS = ['language', 'linkRoots', 'exportFolder', 'export', 'packageFolder', 'package', 'packageContact', 'documentPresets', 'preferences', 'shortcuts', 'flattenerPresets', 'autoCorrectTables', 'userDictionaries', 'printPresets', 'dock'];
const LOCAL_EDITORS = 'folio.editWith.programs';
const object = v => v && typeof v === 'object' && !Array.isArray(v);

function portableSettings(settings) {
  const portable = Object.fromEntries(KEYS.filter(k => settings[k] !== undefined).map(k => [k, settings[k]]));
  if (object(portable.preferences)) {
    portable.preferences = { ...portable.preferences };
    delete portable.preferences[LOCAL_EDITORS];
  }
  return portable;
}

function encode(settings) {
  return JSON.stringify({ format: 'folio-user-settings', version: 1, settings: portableSettings(settings) }, null, 2);
}
function decode(text) {
  const packet = JSON.parse(text);
  if (packet?.format !== 'folio-user-settings' || packet.version !== 1 || !object(packet.settings)) throw Error('invalidSettings');
  const s = packet.settings;
  if (s.language !== undefined && !['system', 'ko', 'en'].includes(s.language)) throw Error('invalidSettings');
  if (s.linkRoots !== undefined && (!Array.isArray(s.linkRoots) || s.linkRoots.some(v => typeof v !== 'string'))) throw Error('invalidSettings');
  for (const k of ['exportFolder', 'packageFolder']) if (s[k] !== undefined && s[k] !== null && typeof s[k] !== 'string') throw Error('invalidSettings');
  for (const k of ['export', 'package', 'packageContact']) if (s[k] !== undefined && s[k] !== null && !object(s[k])) throw Error('invalidSettings');
  if (s.documentPresets !== undefined && !Array.isArray(s.documentPresets)) throw Error('invalidSettings');
  if (s.flattenerPresets !== undefined && !Array.isArray(s.flattenerPresets)) throw Error('invalidSettings');
  if (s.printPresets !== undefined && (!Array.isArray(s.printPresets) || s.printPresets.some(p => !object(p) || typeof p.name !== 'string'))) throw Error('invalidSettings');
  if (s.autoCorrectTables !== undefined && (!object(s.autoCorrectTables) || Object.values(s.autoCorrectTables).some(rows => !Array.isArray(rows) || rows.some(pair => !Array.isArray(pair) || pair.length !== 2 || pair.some(word => typeof word !== 'string'))))) throw Error('invalidSettings');
  if (s.userDictionaries !== undefined && (!object(s.userDictionaries) || Object.values(s.userDictionaries).some(d => !object(d) || ['addedWords', 'removedWords'].some(k => d[k] !== undefined && (!Array.isArray(d[k]) || d[k].some(w => typeof w !== 'string')))))) throw Error('invalidSettings');
  // the panel layout: the dock repairs what it reads (ui/docklayout.ts normalizeDock), so only its shape is checked here
  if (s.dock !== undefined && s.dock !== null && (!object(s.dock) || !Array.isArray(s.dock.strip) || !Array.isArray(s.dock.column) || !Array.isArray(s.dock.floating))) throw Error('invalidSettings');
  if (s.preferences !== undefined && (!object(s.preferences) || Object.entries(s.preferences).some(([k, v]) => /^(TextPreference|DictionaryPreference|TransparencyPreference)\./.test(k) || !['string', 'boolean', 'number'].includes(typeof v) || typeof v === 'number' && !Number.isFinite(v)))) throw Error('invalidSettings');
  if (s.shortcuts !== undefined && (!object(s.shortcuts) || Object.values(s.shortcuts).some(v => !Array.isArray(v) || v.some(k => typeof k !== 'string')))) throw Error('invalidSettings');
  return portableSettings(s);
}
function register(ipc, dialog, window, read, save, tr) {
  ipc.handle('folio:transfer-settings', async (_e, mode) => {
    if (!['import', 'export'].includes(mode)) throw Error('invalid mode');
    const filters = [{ name: tr('userSettings'), extensions: ['json'] }];
    let file = process.env.FOLIO_TEST_SETTINGS_FILE;
    if (!file) {
      const r = mode === 'export'
        ? await dialog.showSaveDialog(window(), { title: tr('exportSettings'), defaultPath: 'Folio-user-settings.json', filters })
        : await dialog.showOpenDialog(window(), { title: tr('importSettings'), filters, properties: ['openFile'] });
      if (r.canceled) return null;
      file = mode === 'export' ? r.filePath : r.filePaths[0];
    }
    if (!file) return null;
    if (mode === 'export') { fs.writeFileSync(file, encode(read()), 'utf8'); return null; }
    let patch;
    try {
      if (fs.statSync(file).size > 10 * 1024 * 1024) throw Error('invalidSettings');
      patch = decode(fs.readFileSync(file, 'utf8'));
    } catch (e) { throw Error(tr('invalidSettings') + ': ' + e.message); }
    if (patch.preferences) {
      const editors = read().preferences?.[LOCAL_EDITORS];
      if (editors !== undefined) patch.preferences[LOCAL_EDITORS] = editors;
    }
    return save(patch);
  });
}
module.exports = { encode, decode, register };
