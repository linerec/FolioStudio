// The pure helpers a dialog's or panel's number field needs to read and write what a person types. No imports, so node can test them
// alone (next/app/scripts/test-numberfield.cjs); lengthfield.tsx re-exports them with the parts that need React and the document's units.

/** A whole number as typed ("3"), or null ("3.5", "", "abc"). */
export function parseInteger(text: string): number | null {
  const t = text.trim();
  const n = Number(t);
  return t !== '' && Number.isInteger(n) ? n : null;
}

/** A plain number as typed (a decimal comma and a trailing % allowed), or null. */
export function parseNumeric(text: string): number | null {
  const t = text.replace(',', '.').replace('%', '').trim();
  const n = Number(t);
  return t !== '' && Number.isFinite(n) ? n : null;
}

/** Writes a plain number (no unit): up to four decimals, no trailing zeros. */
export const formatNumeric = (n: number) => String(Math.round(n * 1e4) / 1e4);

/** Writes a whole number. */
export const formatInteger = (n: number) => String(Math.round(n));

/**
 * One arrow press of a length field whose unit is known only through the `format` / `parse` pair a dialog is given (points to text and
 * back): a pica or cicero field steps a point, any other one unit (a millimetre, an inch), as `lengthSpec` does.
 */
export function stepOfLength(format: (pt: number) => string, parse: (text: string) => number | null): number {
  if (/^-?\d+[pc]\d/i.test(format(30))) return 1;
  const one = parse('1');
  return one !== null && one > 0 ? one : 1;
}
