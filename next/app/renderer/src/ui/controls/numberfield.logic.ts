// Pure logic of the shared number / combo field (numberfield.tsx): parsing, display, steps, scrubbing and list
// navigation. No imports, so node can test it alone (next/app/scripts/test-numberfield.cjs).

export type FieldSpec = {
  min: number;
  max: number;
  /** One arrow press, in the field's unit. */
  step: number;
  /** Digits kept when a value is shown. */
  decimals: number;
  /** Written after the number, as it is shown and typed (" pt", "%", "°"). */
  suffix: string;
};

export type Modifiers = { shift?: boolean; alt?: boolean };

/** Pixels of pointer travel that make one step when a field is scrubbed. */
export const SCRUB_PX_PER_STEP = 2;
/** The pointer must travel this far before a press on a label becomes a scrub (a click stays a click). */
export const SCRUB_START_PX = 3;

/** `v` shown with at most `decimals` digits and no trailing zeros ("-0" is "0"). */
export function trimNumber(v: number, decimals: number): string {
  const f = 10 ** decimals;
  const r = Math.round(v * f) / f;
  return String(Object.is(r, -0) ? 0 : r);
}

/** A value as the field shows it ('' for the mixed value). */
export function formatNumber(v: number | null, spec: Pick<FieldSpec, 'decimals' | 'suffix'>): string {
  return v === null ? '' : `${trimNumber(v, spec.decimals)}${spec.suffix}`;
}

export const clampValue = (v: number, min: number, max: number) => Math.min(max, Math.max(min, v));

/** Float noise gone (0.1 + 0.2): six decimals are far finer than any field. */
export const roundValue = (v: number) => Math.round(v * 1e6) / 1e6;

export const inRange = (v: number, spec: Pick<FieldSpec, 'min' | 'max'>) => Number.isFinite(v) && v >= spec.min && v <= spec.max;

/**
 * What was typed: the number with the suffix and a decimal comma allowed, or null when it is not a number or is outside
 * the range (the old fields refused such an entry and showed the value again; so does this).
 */
export function parseNumber(text: string, spec: Pick<FieldSpec, 'suffix' | 'min' | 'max'>): number | null {
  let t = text.replace(',', '.').trim();
  const suffix = spec.suffix.trim();
  if (suffix && t.endsWith(suffix)) t = t.slice(0, t.length - suffix.length).trim();
  const v = parseFloat(t);
  return inRange(v, spec) ? v : null;
}

/** The size of one step: Shift makes it ten times larger, Alt a tenth (InDesign not measured; STATUS numfield-c1). */
export function stepSize(spec: Pick<FieldSpec, 'step'>, mod: Modifiers = {}): number {
  if (mod.shift && !mod.alt) return spec.step * 10;
  if (mod.alt && !mod.shift) return spec.step / 10;
  return spec.step;
}

/** Where a mixed (blank) field starts from: its `anchor`, else zero kept inside the range. */
export function startOf(current: number | null, spec: Pick<FieldSpec, 'min' | 'max'>, anchor?: number): number {
  return current ?? clampValue(anchor ?? 0, spec.min, spec.max);
}

/** The value one press of ▲ (dir 1) or ▼ (-1) gives, kept inside the range. A mixed field is given a value by the first press. */
export function stepValue(current: number | null, dir: 1 | -1, spec: FieldSpec, mod: Modifiers = {}, anchor?: number): number {
  return clampValue(roundValue(startOf(current, spec, anchor) + dir * stepSize(spec, mod)), spec.min, spec.max);
}

/** The value a drag of `dx` pixels from `start` gives: one step per `SCRUB_PX_PER_STEP`, kept inside the range. */
export function scrubValue(start: number | null, dx: number, spec: FieldSpec, mod: Modifiers = {}, anchor?: number): number {
  return clampValue(roundValue(startOf(start, spec, anchor) + (dx / SCRUB_PX_PER_STEP) * stepSize(spec, mod)), spec.min, spec.max);
}

/** A wheel turn up (negative `deltaY`) raises the value, down lowers it. */
export const wheelDirection = (deltaY: number): 1 | -1 | 0 => (deltaY < 0 ? 1 : deltaY > 0 ? -1 : 0);

/** The list item an arrow, Home or End key moves to (the first or last when nothing is active yet). */
export function listMove(active: number, key: string, count: number): number {
  if (count <= 0) return -1;
  const last = count - 1;
  switch (key) {
    case 'ArrowDown': return active < 0 ? 0 : Math.min(last, active + 1);
    case 'ArrowUp': return active < 0 ? last : Math.max(0, active - 1);
    case 'PageDown': case 'End': return last;
    case 'PageUp': case 'Home': return 0;
    default: return active;
  }
}

/** The index of the list item holding `v` (-1 when none does). */
export function indexOfValue(values: number[], v: number | null): number {
  return v === null ? -1 : values.findIndex((x) => Math.abs(x - v) < 1e-9);
}

/** What a commit does: nothing changed (the previewed value is the one the field had) takes the preview back instead. */
export const commitAction = (previewed: number | null, base: number | null): 'commit' | 'cancel' =>
  previewed !== null && (base === null || Math.abs(previewed - base) > 1e-9) ? 'commit' : 'cancel';
