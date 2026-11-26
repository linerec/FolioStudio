// The rulers' zero point (Document/@ZeroPoint): the point of the ruler that the rulers, the Control panel and the Info panel call 0.
// InDesign 2025 (real mouse, docinput-c2/c3): a drag from the ruler corner onto the page sets it to the drop point, two clicks on the corner
// put it back to [0, 0], and a spine ruler cannot move it. Only what is shown shifts; items and guides keep their coordinates.
// The document changes through the engine (`setViewPref` key `zeroPoint`); this file only turns screen positions into numbers.
// No runtime imports: the node test transpiles this file alone (scripts/test-zeropoint.cjs).

export type Pair = [number, number];

/** The document's zero point (`[0, 0]` when the engine sends none). */
export const zeroOf = (info: { zeroPoint?: Pair } | null | undefined): Pair => info?.zeroPoint ?? [0, 0];

/** What is added to a position in a page's own axes to get the ruler's coordinates: the page's place in the spread (page-origin rulers
 *  start at the page itself) less the zero point. */
export function rulerOffset(rulerOrigin: string, spreadOffset: Pair | undefined, zero: Pair): Pair {
  const base: Pair = rulerOrigin === 'PageOrigin' ? [0, 0] : spreadOffset ?? [0, 0];
  return [base[0] - zero[0], base[1] - zero[1]];
}

/** The zero point a drop at `local` (page axes, points) of a page whose place in the spread is `spreadOffset` makes, or null when the ruler
 *  runs per spine (InDesign then refuses and shows a warning). The value is in the ruler origin's own coordinates, not less the old zero point. */
export function zeroPointAt(rulerOrigin: string, spreadOffset: Pair | undefined, local: Pair): Pair | null {
  if (rulerOrigin === 'SpineOrigin') return null;
  const base: Pair = rulerOrigin === 'PageOrigin' ? [0, 0] : spreadOffset ?? [0, 0];
  return [local[0] + base[0], local[1] + base[1]];
}
