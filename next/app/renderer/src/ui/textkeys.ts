// Character and paragraph formatting keys that InDesign has no menu item for (keyaudit-c1; they are in its shortcut editor only): the text size,
// leading, kerning/tracking, baseline shift and scale keys, Bold / Italic / Normal, paragraph alignment, auto leading, hyphenation on/off.
// Each is a command (so Edit > Keyboard Shortcuts lists and rebinds it) that works on the text selection through the same `apply` as the
// Character and Paragraph panels. Without a text selection `run` is absent and `fallThrough` lets the key go on (Alt+arrows nudge a copy of an object).
// The steps are the document's Units & Increments (leading/size, baseline shift, kerning) as the panels' arrow keys use them.
import type { TextProperties } from '../engine';
import type { DocumentInfo } from '../inspect';
import type { Command } from './commands';
import type { TextFormatting } from './textformat';
import { round4 as round } from './util/round';

type View = NonNullable<DocumentInfo['view']>;
type Format = NonNullable<TextFormatting['selection']>['format'];
const num = (v: unknown): number | null => (typeof v === 'number' && Number.isFinite(v) ? v : null);
const clamp = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

const BOLD = /\b(bold|semibold|demibold|black|heavy)\b/i;
const ITALIC = /(italic|oblique)/i;
/** The style of the family with the wanted weight and slant ("Bold", "Italic", "Bold Italic", "Regular"), or null when the family has none. */
export function pickFontStyle(styles: string[], bold: boolean, italic: boolean): string | null {
  const wanted = bold && italic ? 'Bold Italic' : bold ? 'Bold' : italic ? 'Italic' : 'Regular';
  const same = styles.filter(s => BOLD.test(s) === bold && ITALIC.test(s) === italic);
  return styles.find(s => s === wanted) ?? same.find(s => !/\b(semi|demi|light|thin|black|heavy|condensed|narrow)\b/i.test(s)) ?? same[0] ?? null;
}

/** The new value of a number in the direction and multiple of a key; null when it is mixed (the selection holds different values). */
function stepped(value: unknown, dir: 1 | -1, step: number, times: 1 | 5, min: number, max: number): number | null {
  const v = num(value);
  return v === null ? null : round(clamp(v + dir * step * times, min, max));
}

type Ctx = { text: TextFormatting; view: View | undefined };

/** The format commands: an id → what it does with the selection's format (`f`), the selection's range and the document's increments. */
function table({ text, view }: Ctx): Record<string, (f: Format, range: { start: number; end: number }) => TextProperties | null> {
  const sizeStep = view?.leadingKeyIncrement ?? 2, baselineStep = view?.baselineShiftKeyIncrement ?? 2, kerningStep = view?.kerningKeyIncrement ?? 20;
  const t: Record<string, (f: Format, r: { start: number; end: number }) => TextProperties | null> = {};
  for (const [name, dir, times] of [['Up', 1, 1], ['Down', -1, 1], ['Up5', 1, 5], ['Down5', -1, 5]] as const) {
    t[`text.key.size${name}`] = f => { const v = stepped(f.pointSize, dir, sizeStep, times, 0.1, 1296); return v === null ? null : { pointSize: v }; };
    t[`text.key.leading${name}`] = f => {
      // "auto" leading steps from the line the engine would use (1.2 × the size), as the panel's field does
      const cur = f.leading === 'auto' ? (num(f.pointSize) ?? 0) * 1.2 : f.leading;
      const v = stepped(cur, dir, sizeStep, times, 0, 5000);
      return v === null ? null : { leading: v };
    };
    // a range is tracked, a caret between two letters kerns them
    t[`text.key.kerning${name}`] = (f, r) => {
      const prop = r.start === r.end ? 'kerningValue' : 'tracking';
      const v = stepped(f[prop], dir, kerningStep, times, -1000, 10000);
      return v === null ? null : { [prop]: v };
    };
    t[`text.key.baseline${name}`] = (f) => { const v = stepped(f.baselineShift, dir, baselineStep, times, -5000, 5000); return v === null ? null : { baselineShift: v }; };
  }
  // the horizontal and vertical scales: 1 % and 5 % (25 % to 400 %, the Character panel's range)
  for (const [axis, prop] of [['h', 'horizontalScale'], ['v', 'verticalScale']] as const) {
    for (const [name, dir, times] of [['Up', 1, 1], ['Down', -1, 1], ['Up5', 1, 5], ['Down5', -1, 5]] as const)
      t[`text.key.${axis}scale${name}`] = f => { const v = stepped(f[prop], dir, 1, times, 25, 400); return v === null ? null : { [prop]: v }; };
  }
  t['text.key.hscaleNormal'] = () => ({ horizontalScale: 100 });
  t['text.key.vscaleNormal'] = () => ({ verticalScale: 100 });
  t['text.key.autoLeading'] = () => ({ leading: 'auto' });
  t['text.key.resetKerning'] = (_f, r): TextProperties => (r.start === r.end ? { kerningValue: 0 } : { tracking: 0, kerningValue: 0 });
  const style = (bold: boolean | null, italic: boolean | null) => (f: Format) => {
    const family = String(f.appliedFont ?? ''), current = String(f.fontStyle ?? '');
    const styles = text.selection?.fonts[family];
    if (!family || !styles) return null;
    const next = pickFontStyle(styles, bold ?? BOLD.test(current), italic ?? ITALIC.test(current));
    return next && next !== current ? { fontStyle: next } : null;
  };
  t['text.key.bold'] = style(true, null);
  t['text.key.italic'] = style(null, true);
  t['text.key.normal'] = style(false, false);
  const align = (value: string) => () => ({ justification: value });
  t['text.key.alignLeft'] = align('LEFT_ALIGN');
  t['text.key.alignCenter'] = align('CENTER_ALIGN');
  t['text.key.alignRight'] = align('RIGHT_ALIGN');
  t['text.key.alignJustify'] = align('LEFT_JUSTIFIED');
  t['text.key.alignForceJustify'] = align('FULLY_JUSTIFIED');
  t['text.key.alignTowardsSpine'] = align('TO_BINDING_SIDE');
  t['text.key.alignAwaySpine'] = align('AWAY_FROM_BINDING_SIDE');
  t['text.key.alignToGrid'] = f => ({ alignToBaseline: f.alignToBaseline !== true });
  t['text.key.hyphenate'] = f => ({ hyphenation: f.hyphenation !== true });
  return t;
}

/** The handlers of the text key commands: `run` only with a text selection (and only when the value is not mixed), else the key falls through. */
export function textKeyCommands(text: TextFormatting, view: DocumentInfo['view'] | undefined): Record<string, Partial<Command>> {
  const sel = text.selection;
  const out: Record<string, Partial<Command>> = {};
  for (const [id, props] of Object.entries(table({ text, view }))) {
    const props_ = sel ? props(sel.format, { start: sel.start, end: sel.end }) : null;
    out[id] = { fallThrough: true, run: props_ ? () => text.apply(props_) : undefined };
  }
  return out;
}
