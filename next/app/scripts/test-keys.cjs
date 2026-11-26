// Run with node next/app/scripts/test-keys.cjs; uses only the installed TypeScript compiler.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const ts = require('typescript');
const source = fs.readFileSync(path.join(__dirname, '../renderer/src/ui/keys.ts'), 'utf8');
const js = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText;
const event = (mods = {}) => ({ code: 'KeyS', ctrlKey: false, metaKey: false, altKey: false, shiftKey: false, ...mods });
for (const platform of ['Win32', 'Linux x86_64', 'MacIntel']) {
  const exports = {};
  vm.runInNewContext(js, { exports, navigator: { platform } });
  const mac = platform === 'MacIntel';
  const command = mac ? { metaKey: true } : { ctrlKey: true };
  assert.equal(exports.matches(event(command), 'Ctrl+S'), true);
  assert.equal(exports.matches(event(mac ? { ctrlKey: true } : {}), 'Ctrl+S'), false);
  assert.equal(exports.matches(event({ ...command, code: 'KeyE', altKey: true, shiftKey: true }), 'Ctrl+Alt+Shift+E'), true);
  // Layout-independent physical codes still work with Korean or Option-produced glyphs.
  assert.equal(exports.matches(event({ ...command, code: 'KeyS', key: 'ㄴ' }), 'Ctrl+S'), true);
  assert.equal(exports.keyLabel('Ctrl+Alt+Shift+E'), mac ? '⌥⇧⌘E' : 'Ctrl+Alt+Shift+E');
  assert.equal(exports.matches(event({ code: 'KeyS' }), 'S'), true);
  assert.equal(exports.matches(event({ code: 'KeyS', altKey: true }), 'S'), false);
  const key = (key, mods = {}) => exports.textKey(event({ key, code: key, ...mods }));
  const move = (keyName, mods, unit, direction, extend = false) =>
    assert.equal(JSON.stringify(key(keyName, mods)), JSON.stringify({ action: 'move', move: { unit, direction, extend } }));
  move('ArrowLeft', {}, 'character', -1);
  move('ArrowRight', { shiftKey: true }, 'character', 1, true);
  move('ArrowLeft', mac ? { altKey: true } : command, 'word', -1);
  move('ArrowRight', { ...(mac ? { altKey: true } : command), shiftKey: true }, 'word', 1, true);
  move('ArrowUp', {}, 'vertical', -1);
  move('Home', {}, 'line', -1);
  move('End', command, 'story', 1);
  if (mac) {
    for (const shiftKey of [false, true]) {
      move('ArrowLeft', { metaKey: true, shiftKey }, 'line', -1, shiftKey);
      move('ArrowRight', { metaKey: true, shiftKey }, 'line', 1, shiftKey);
      move('ArrowUp', { metaKey: true, shiftKey }, 'story', -1, shiftKey);
      move('ArrowDown', { metaKey: true, shiftKey }, 'story', 1, shiftKey);
    }
    assert.equal(key('Backspace', { altKey: true }).unit, 'word');
    assert.equal(key('Backspace', command).unit, 'line');
    move('ArrowLeft', { ctrlKey: true }, 'character', -1);
    assert.equal(exports.textKey(event({ key: 'z', code: 'KeyZ', ctrlKey: true })), null);
  } else {
    // keyaudit-c1: Ctrl+Up/Down are paragraph moves, Ctrl+Backspace/Delete take a word, Ctrl+Shift+\ selects the line, Alt+arrows are formatting keys (not caret keys)
    move('ArrowUp', command, 'paragraph', -1);
    move('ArrowDown', { ...command, shiftKey: true }, 'paragraph', 1, true);
    assert.equal(key('Backspace', command).unit, 'word');
    assert.equal(key('Delete', command).unit, 'word');
    assert.equal(key('Backspace', {}).unit, 'character');
    assert.equal(JSON.stringify(exports.textKey(event({ key: '\\', code: 'Backslash', shiftKey: true, ...command }))), JSON.stringify({ action: 'selectLine' }));
    assert.equal(exports.textKey(event({ key: 'ArrowLeft', code: 'ArrowLeft', altKey: true })), null);
    assert.equal(exports.textKey(event({ key: 'Backspace', code: 'Backspace', altKey: true, ...command })), null);
  }
  assert.equal(exports.textKey(event({ key: 'ㅋ', code: 'KeyZ', ...command })).action, 'history');
  assert.equal(exports.textKey(event({ key: 'ㅁ', code: 'KeyA', ...command })).action, 'selectAll');
  assert.equal(exports.clearStyleOverrides({ altKey: true }), true);
  assert.equal(exports.fieldKey({ key: 'Escape' }), 'cancel');
  assert.equal(exports.fieldKey({ key: 'Enter' }), 'commit');
  assert.equal(exports.shortcutText('Close (Ctrl+W), Alt'), mac ? 'Close (⌘W), Option' : 'Close (Ctrl+W), Alt');
  // A newly registered contextual default must not take an existing user shortcut,
  // and the dialog must agree with dispatch even when the contextual command is first.
  const load = file => {
    const source = fs.readFileSync(path.join(__dirname, '../renderer/src', file), 'utf8');
    const module = { exports: {} };
    vm.runInNewContext(ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.CommonJS } }).outputText,
      { exports: module.exports, require: id => { assert.ok(id.endsWith('/keys')); return exports; } });
    return module.exports;
  };
  const { applyShortcuts, shortcutConflict } = load('features/preferences/shortcuts.ts');
  const { commandFor } = load('ui/commands.ts');
  const custom = { 'edit.preferences': ['Alt+Right'] };
  const altRight = event({ key: 'ArrowRight', code: 'ArrowRight', altKey: true });
  for (const active of [false, true]) {
    const defaults = () => ({
      'text.key.kerningUp': { label: 'Kerning', keys: ['Alt+Right', 'Alt+Shift+Right'], fallThrough: true, run: active ? () => {} : undefined },
      'file.open': { label: 'Open', keys: ['Ctrl+O'], run: () => {} },
      'edit.preferences': { label: 'Preferences', keys: ['Ctrl+K'], run: () => {} },
      'other': { label: 'Other', keys: [], run: () => {} },
    });
    assert.equal(shortcutConflict(defaults(), {}, 'edit.preferences', 'Alt+Right'), undefined);
    assert.equal(shortcutConflict(defaults(), {}, 'edit.preferences', 'Ctrl+O')[0], 'file.open');
    const assigned = applyShortcuts(defaults(), custom);
    assert.equal(commandFor(altRight, assigned), 'edit.preferences');
    assert.equal(JSON.stringify(assigned['text.key.kerningUp'].keys), JSON.stringify(['Alt+Shift+Right']));
    assert.equal(shortcutConflict(assigned, custom, 'other', 'Alt+Right')[0], 'edit.preferences');
    assert.equal(commandFor(altRight, applyShortcuts(defaults(), {})), active ? 'text.key.kerningUp' : null);
    assert.equal(commandFor(altRight, applyShortcuts(defaults(), { 'edit.preferences': [] })), active ? 'text.key.kerningUp' : null);
    const rebound = { 'text.key.kerningUp': ['Alt+Right'] };
    assert.equal(shortcutConflict(applyShortcuts(defaults(), rebound), rebound, 'edit.preferences', 'Alt+Right')[0], 'text.key.kerningUp');
    const unknown = applyShortcuts(defaults(), { removedCommand: ['Alt+Right'] });
    assert.equal(commandFor(altRight, unknown), active ? 'text.key.kerningUp' : null);
  }
}
console.log('Keyboard conventions and custom/contextual shortcut precedence: Windows, Linux, Mac passed');
