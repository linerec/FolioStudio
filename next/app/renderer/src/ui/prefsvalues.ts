// Reading the values typed in the Edit ▸ Preferences dialogs: each returns the number, or the message to show when the
// text is not a valid value or is out of the range InDesign accepts (the engine holds the same ranges).
import { tr } from '../i18n';
import { formatLength, parseLength } from '../inspect';

/** Hundred-thousandths of a point: what a length typed in millimetres or inches rounds to. */
const tidy = (v: number) => Math.round(v * 1e5) / 1e5;

/** A length typed in the field's unit (or with its own), in points, between `lo` and `hi` points. */
export function length(label: string, text: string, unit: string, lo: number, hi: number): number | string {
  const v = parseLength(text, unit);
  if (v === null) return tr('{0}: 올바른 길이를 넣으세요.', { 0: label });
  const pt = tidy(v);
  return pt < lo || pt > hi ? tr('{0}: {1}에서 {2} 사이여야 합니다.', { 0: label, 1: formatLength(lo, unit), 2: formatLength(hi, unit) }) : pt;
}

/** Keep full precision when the user has not edited the rounded display value. */
export function editedLength(label: string, text: string, unit: string, lo: number, hi: number, before: number, displayUnit = unit): number | string {
  return text === formatLength(before, displayUnit) ? before : length(label, text, unit, lo, hi);
}

/** A whole number between `lo` and `hi`. */
export function whole(label: string, text: string, lo: number, hi: number): number | string {
  const v = Number(text.replace(',', '.'));
  return text.trim() === '' || !Number.isInteger(v) || v < lo || v > hi ? tr('{0}: {1}에서 {2} 사이의 정수여야 합니다.', { 0: label, 1: lo, 2: hi }) : v;
}

/** A number (a percentage may carry its %) between `lo` and `hi`. */
export function real(label: string, text: string, lo: number, hi: number): number | string {
  const v = Number(text.replace(',', '.').replace('%', ''));
  return text.trim() === '' || !Number.isFinite(v) || v < lo || v > hi ? tr('{0}: {1}에서 {2} 사이여야 합니다.', { 0: label, 1: lo, 2: hi }) : v;
}
