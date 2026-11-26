// Platform keyboard conventions. Stored shortcut specs remain Windows-compatible.
export const isMac = typeof navigator !== 'undefined' && /mac/i.test(navigator.platform);

export function commandKey(e: { ctrlKey: boolean; metaKey: boolean }): boolean {
  return isMac ? e.metaKey : e.ctrlKey;
}

type Modifiers = { ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean };
type TextKeyEvent = Modifiers & { key: string; code?: string };
export type TextMove = { unit: 'character' | 'word' | 'line' | 'story' | 'vertical' | 'paragraph'; direction: -1 | 1; extend: boolean };
export type TextKey =
  | { action: 'history'; redo: boolean }
  | { action: 'selectAll' | 'exit' | 'selectLine' }
  | { action: 'insert'; text: string }
  | { action: 'delete'; direction: -1 | 1; unit: 'character' | 'word' | 'line' }
  | { action: 'move'; move: TextMove };

/** Interpret keys here; the editor owns text geometry, graphemes and the edit queue. */
export function textKey(e: TextKeyEvent): TextKey | null {
  const mod = commandKey(e) && (!isMac || !e.ctrlKey);
  const letter = e.code?.startsWith('Key') ? e.code.slice(3).toLowerCase() : e.key.toLowerCase();
  if (mod && !e.altKey) {
    // Ctrl+Shift+Y is Apply Normal (ui/textkeys.ts), not a redo
    if (letter === 'z' || (letter === 'y' && !e.shiftKey)) return { action: 'history', redo: letter === 'y' || e.shiftKey };
    if (letter === 'a') return { action: 'selectAll' };
  }
  // Select Line (InDesign: Ctrl+Shift+\): the whole line of the caret, as a triple click selects it
  if (mod && e.shiftKey && !e.altKey && e.code === 'Backslash') return { action: 'selectLine' };
  // Windows/Linux: Alt with an arrow, Backspace or Delete is character formatting (kerning, leading, baseline shift, word space: ui/textkeys.ts), not a caret key
  if (!isMac && e.altKey && (e.key.startsWith('Arrow') || e.key === 'Backspace' || e.key === 'Delete')) return null;
  if (e.key === 'Escape') return { action: 'exit' };
  if (e.key === 'Backspace' || e.key === 'Delete') return {
    action: 'delete', direction: e.key === 'Backspace' ? -1 : 1,
    // Ctrl+Backspace / Ctrl+Delete take the word (InDesign Windows); on a Mac ⌘ takes the line and ⌥ the word
    unit: isMac && mod ? 'line' : (isMac ? e.altKey && !e.ctrlKey : mod) ? 'word' : 'character',
  };
  if (e.key === 'Enter') return { action: 'insert', text: e.shiftKey ? '\u2028' : '\r' };
  // Ctrl+Tab is Next Window (Ctrl+Shift+Tab Previous Window), not a tab character (tools-c1)
  if (e.key === 'Tab') return mod ? null : { action: 'insert', text: '\t' };
  const direction = ['ArrowLeft', 'ArrowUp', 'Home'].includes(e.key) ? -1 : 1;
  let unit: TextMove['unit'];
  if (e.key === 'Home' || e.key === 'End') unit = mod ? 'story' : 'line';
  // Ctrl+Up/Down move to the paragraph start before / after the caret (InDesign Windows); a Mac's ⌘ goes to the story ends
  else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') unit = isMac ? (mod ? 'story' : 'vertical') : mod ? 'paragraph' : 'vertical';
  else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight')
    unit = isMac && mod ? 'line' : (isMac ? e.altKey && !e.ctrlKey : mod) ? 'word' : 'character';
  else return null;
  return { action: 'move', move: { unit, direction, extend: e.shiftKey } };
}

/** Vertical text: Up/Down move along the line (a character), Left goes to the next line and Right to the one before (InDesign's vertical type). */
export function verticalMove(m: TextMove): TextMove {
  if (m.unit === 'vertical') return { ...m, unit: 'character' };
  if (m.unit === 'character') return { ...m, unit: 'vertical', direction: m.direction < 0 ? 1 : -1 };
  return m;
}

/** Option on Mac is the same physical modifier as Alt on Windows. */
export const clearStyleOverrides = (e: { altKey: boolean }) => e.altKey;
export const fieldKey = (e: { key: string }) => e.key === 'Enter' ? 'commit' : e.key === 'Escape' ? 'cancel' : null;

const NAMED: Record<string, string> = {
  '=': 'Equal', '+': 'Equal', '-': 'Minus', ';': 'Semicolon', "'": 'Quote', '[': 'BracketLeft', ']': 'BracketRight',
  '\\': 'Backslash', ',': 'Comma', '.': 'Period', '/': 'Slash', '`': 'Backquote',
  Esc: 'Escape', Escape: 'Escape', Space: 'Space', Tab: 'Tab', Enter: 'Enter', Delete: 'Delete', Backspace: 'Backspace',
  PageUp: 'PageUp', PageDown: 'PageDown', Home: 'Home', End: 'End', Up: 'ArrowUp', Down: 'ArrowDown', Left: 'ArrowLeft', Right: 'ArrowRight',
};

type Key = { ctrl: boolean; alt: boolean; shift: boolean; codes: string[] };

const parsed = new Map<string, Key>();

export function parseKey(spec: string): Key {
  const hit = parsed.get(spec);
  if (hit) return hit;
  const parts = spec.endsWith('++') ? [...spec.slice(0, -2).split('+'), '+'] : spec.split('+');
  const key = parts.pop() ?? '';
  const mods = new Set(parts);
  let codes: string[];
  if (/^[A-Za-z]$/.test(key)) codes = [`Key${key.toUpperCase()}`];
  else if (/^[0-9]$/.test(key)) codes = [`Digit${key}`, `Numpad${key}`];
  else if (/^F\d{1,2}$/.test(key)) codes = [key];
  else if (key === '=' || key === '+') codes = ['Equal', 'NumpadAdd'];
  else if (key === '-') codes = ['Minus', 'NumpadSubtract'];
  else codes = [NAMED[key] ?? key];
  const k = { ctrl: mods.has('Ctrl'), alt: mods.has('Alt'), shift: mods.has('Shift'), codes };
  parsed.set(spec, k);
  return k;
}

export function matches(e: KeyboardEvent, spec: string) {
  const k = parseKey(spec);
  const ctrl = commandKey(e);
  return (!isMac || !e.ctrlKey) && ctrl === k.ctrl && e.altKey === k.alt && e.shiftKey === k.shift && k.codes.includes(e.code);
}

/** Editable menu shortcuts avoid unmodified keys used by text editing and tools. */
export function shortcutSpec(e: TextKeyEvent): string | null {
  const ctrl = commandKey(e);
  if ((isMac && e.ctrlKey) || (!ctrl && !e.altKey && !/^F([1-9]|1[0-9]|2[0-4])$/.test(e.code ?? ''))) return null;
  const code = e.code ?? '';
  if (/^(Control|Meta|Alt|Shift)/.test(code) || ['Escape', 'Space', 'Tab'].includes(code)) return null;
  const key = code.startsWith('Key') ? code.slice(3) : code.startsWith('Digit') ? code.slice(5)
    : Object.entries(NAMED).find(([, v]) => v === code)?.[0] ?? (/^F\d{1,2}$/.test(code) ? code : '');
  return key ? [...(ctrl ? ['Ctrl'] : []), ...(e.altKey ? ['Alt'] : []), ...(e.shiftKey ? ['Shift'] : []), key].join('+') : null;
}

export function sameShortcut(a: string, b: string): boolean {
  const x = parseKey(a), y = parseKey(b);
  return x.ctrl === y.ctrl && x.alt === y.alt && x.shift === y.shift && x.codes.some(c => y.codes.includes(c));
}

/** "Ctrl+Shift+D" as shown in menus on this platform. */
export function keyLabel(spec: string | undefined): string {
  if (!spec) return '';
  if (!isMac) return spec;
  const k = parseKey(spec);
  const key = spec.endsWith('++') ? '+' : spec.split('+').pop() ?? '';
  return `${k.alt ? '⌥' : ''}${k.shift ? '⇧' : ''}${k.ctrl ? '⌘' : ''}${key.length === 1 ? key.toUpperCase() : key}`;
}

/** Inline shortcut hints use the same platform labels as menus. */
export function shortcutText(text: string): string {
  if (!isMac) return text;
  return text.replace(/\b(?:(?:Ctrl|Alt|Shift)\+)+(?:[A-Za-z0-9]+|[=+\-\[\]';,./\\])/g, keyLabel)
    .replace(/\bAlt\b/g, 'Option');
}

export function quickApplyKey(e: { key: string; isComposing: boolean; ctrlKey: boolean; metaKey: boolean; altKey: boolean; shiftKey: boolean }) {
  if (e.isComposing || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return null;
  const actions: Record<string, 'close' | 'apply' | 'previous' | 'next'> = { Escape: 'close', Enter: 'apply', ArrowUp: 'previous', ArrowDown: 'next' };
  return actions[e.key] ?? null;
}

