// Keyboard shortcuts of paragraph and character styles (Style Options "Shortcut" field; a key press applies the style). What a combination
// means (valid, label, who owns it, which style wins) is the engine's (`styleShortcut` query, document info `styleShortcuts`); this file
// only turns a key event into the spelling the engine reads (`ctrl+alt+num1`) and applies the style the engine named.
import { useEffect, useRef, type MutableRefObject } from 'react';
import type { Commands } from './commands';
import { commandOwner, setCommandSource } from './commandowner';
import type { TextFormatting } from './textformat';
import type { EditCommand } from '../engine';
import { t as tr } from '../i18n';

type KeyLike = { code: string; ctrlKey: boolean; metaKey: boolean; shiftKey: boolean; altKey: boolean };

/** Main-keyboard punctuation a style shortcut can use (measured in InDesign 2025: every one is accepted), by `KeyboardEvent.code`; the spelling is the character. */
const PUNCT: Record<string, string> = {
  Semicolon: ';', Equal: '=', Comma: ',', Minus: '-', Period: '.', Slash: '/', Backquote: '`', BracketLeft: '[', Backslash: '\\', BracketRight: ']', Quote: "'",
};
/** Cursor-movement keys (Insert, Delete and Backspace are refused by InDesign's field). */
const NAV: Record<string, string> = {
  ArrowLeft: 'left', ArrowRight: 'right', ArrowUp: 'up', ArrowDown: 'down', Home: 'home', End: 'end', PageUp: 'pageup', PageDown: 'pagedown',
};

/** `ctrl+alt+num1` for a key event; null for a key that cannot be part of a style shortcut (modifier alone, Enter, Tab...). */
export function shortcutSpecFromEvent(e: KeyLike): string | null {
  const c = e.code;
  let key = '';
  if (/^Numpad[0-9]$/.test(c)) key = `num${c.slice(6)}`;
  else if (c === 'NumpadMultiply') key = 'nummul';
  else if (c === 'NumpadAdd') key = 'numadd';
  else if (c === 'NumpadSubtract') key = 'numsub';
  else if (c === 'NumpadDecimal') key = 'numdot';
  else if (c === 'NumpadDivide') key = 'numdiv';
  else if (/^F([1-9]|1[0-2])$/.test(c)) key = c.toLowerCase();
  else if (/^Key[A-Z]$/.test(c)) key = c.slice(3).toLowerCase();
  else if (/^Digit[0-9]$/.test(c)) key = c.slice(5);
  else if (c === 'Space') key = 'space';
  else if (c in PUNCT) key = PUNCT[c];
  else if (c in NAV) key = NAV[c];
  else return null;
  // Shift does not count with a keypad digit (the engine drops it too: Ctrl+Shift+Num1 is Ctrl+Num1)
  const mods = [(e.ctrlKey || e.metaKey) && 'ctrl', e.shiftKey && !/^num\d$/.test(key) && 'shift', e.altKey && 'alt'].filter(Boolean);
  return [...mods, key].join('+');
}

/** The same spelling for a menu command's key as the menus write it (`Ctrl+Alt+Shift+P`, `Ctrl+=`, `Shift+PageUp`); null for a key a style shortcut cannot be (Esc, Tab, Enter...). */
export function shortcutSpecFromMenuKey(spec: string): string | null {
  const parts = spec.split('+');
  const raw = parts.pop() ?? '';
  const named: Record<string, string> = { Space: 'space', Up: 'up', Down: 'down', Left: 'left', Right: 'right', Home: 'home', End: 'end', PageUp: 'pageup', PageDown: 'pagedown' };
  let key: string | null = null;
  if (/^[A-Za-z]$/.test(raw)) key = raw.toLowerCase();
  else if (/^[0-9]$/.test(raw)) key = raw;
  else if (/^F([1-9]|1[0-2])$/.test(raw)) key = raw.toLowerCase();
  else if (raw in named) key = named[raw];
  else if (Object.values(PUNCT).includes(raw)) key = raw;
  if (key === null) return null;
  const has = (m: string) => parts.includes(m);
  return [...(has('Ctrl') ? ['ctrl'] : []), ...(has('Shift') ? ['shift'] : []), ...(has('Alt') ? ['alt'] : []), key].join('+');
}

/** What the key handler needs beyond the text selection: the object selection (InDesign applies a style key to the whole story of the selected text frames and, with nothing selected, changes the default style). */
export type StyleShortcutObjects = {
  /** The document can be edited. */
  editable: boolean;
  /** How many objects are selected (text frames or not). */
  selected: number;
  /** The selected text frames (ids): the ones that hold a story. */
  frames: string[];
  edit: (label: string, commands: EditCommand[]) => unknown;
};

/**
 * A key press that is a style's shortcut applies that style (InDesign 2025, measured: oracle/rules/stylekeys.ko.md and stylekeys-c2.ko.md):
 * to the text selection as a click in the panel does (overrides stay); with text frames selected to the whole story of each; with nothing selected the
 * document's default style changes; with only objects that hold no text (a rectangle) selected nothing happens.
 */
export function useStyleShortcuts(text: TextFormatting, commands: MutableRefObject<Commands>, objects: StyleShortcutObjects) {
  const latest = useRef(text);
  latest.current = text;
  const objectsRef = useRef(objects);
  objectsRef.current = objects;
  useEffect(() => {
    setCommandSource(() => commands.current);
    // capture phase: the story editor stops the keys it does not use; a menu command with the same keys wins (measured in InDesign)
    const onKey = (e: KeyboardEvent) => {
      const t = latest.current, o = objectsRef.current;
      const list = t.info?.styleShortcuts;
      // not while a dialog or menu is open (the keys belong to it)
      if (e.defaultPrevented || !list?.length || !(t.selection || o.editable) || document.querySelector('[role=dialog], .dropdown')) return;
      const el = e.target as HTMLElement | null;
      if (el && el.dataset?.textEditor !== 'true' && (el.tagName === 'INPUT' || el.tagName === 'TEXTAREA' || el.tagName === 'SELECT' || el.isContentEditable)) return;
      const spec = shortcutSpecFromEvent(e);
      if (!spec) return;
      const hit = list.find((row) => row.shortcut === spec);
      if (!hit || commandOwner(spec, commands.current)) return;
      e.preventDefault();
      e.stopPropagation();
      if (t.selection) t.style(hit.kind, hit.id, false);
      else if (o.frames.length) void o.edit(tr('글 스타일'), [{ op: 'applyStyleToFrames', kind: hit.kind, style: hit.id, ids: o.frames, clear: false }]);
      else if (!o.selected) void o.edit(tr('기본 스타일'), [{ op: 'setTextDefaultStyle', kind: hit.kind, style: hit.id }]);
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [commands]);
}
