// Window > Arrange: where N windows sit in the document area. Pure geometry, no screen state (windows.ts keeps the state).
// Rects of docked layouts are fractions of the document area (0..1); a floating window's rect is in pixels.
//
// "Vertically" is a row of tall windows side by side (columns); "Horizontally" is a stack of wide windows (rows), as Photoshop and
// InDesign name them. "Stacked": the first window is the big one on the left (half the width), the others are stacked in a column
// beside it. "4-up" and "6-up" are a grid (2 by 2, 3 by 2).
//
// The proportions are Folio's own, by decision (user, 2026-10-28): InDesign's exact ones were not measured and need not be. They are
// all equal shares of the document area, with no gaps and no overlaps (test-windows.cjs checks that for every arrangement and number):
//   columns / rows (Tile All Vertically / Horizontally, 2-up to 4-up in one direction): n equal strips, each 1/n of the width / height.
//   stacked (3-, 4-, 5-, 6-up Stacked): the first window is the left half, full height; the other n-1 are the right half, each 1/(n-1)
//     of the height.
//   4-up: a 2 by 2 grid of quarters. 6-up: a 3 by 2 grid of sixths, in reading order.
//   Tile (any number from 2): a grid of ceil(sqrt(n)) columns in reading order; a last row that is not full shares the whole width.
//   Cascade (floating): 70% of the area each, a step of 32 px down and right, starting again at the corner when one would not fit.

export type Rect = { x: number; y: number; w: number; h: number };

const columns = (n: number): Rect[] => Array.from({ length: n }, (_, i) => ({ x: i / n, y: 0, w: 1 / n, h: 1 }));
const rows = (n: number): Rect[] => Array.from({ length: n }, (_, i) => ({ x: 0, y: i / n, w: 1, h: 1 / n }));
const stacked = (n: number): Rect[] => [
  { x: 0, y: 0, w: 0.5, h: 1 },
  ...Array.from({ length: n - 1 }, (_, i) => ({ x: 0.5, y: i / (n - 1), w: 0.5, h: 1 / (n - 1) })),
];
/** Windows in reading order on a grid of `cols` columns; a last row that is not full shares the whole width. */
const grid = (n: number, cols: number): Rect[] => {
  const lines = Math.ceil(n / cols);
  return Array.from({ length: n }, (_, i) => {
    const r = Math.floor(i / cols);
    const inRow = r === lines - 1 ? n - r * cols : cols;
    const c = i - r * cols;
    return { x: c / inRow, y: r / lines, w: 1 / inRow, h: 1 / lines };
  });
};
/** A grid as square as it can be (columns = ceil(sqrt(n))), for Tile. */
const square = (n: number): Rect[] => grid(n, Math.ceil(Math.sqrt(n)));

/** The arrangements: the number of windows each wants (null: any number from 2), and the rects for that number. */
export const ARRANGEMENTS: Record<string, { count: number | null; rects: (n: number) => Rect[] }> = {
  tileAllVertically: { count: null, rects: columns },
  tileAllHorizontally: { count: null, rects: rows },
  '2UpHorizontal': { count: 2, rects: rows },
  '2UpVertical': { count: 2, rects: columns },
  '3UpHorizontal': { count: 3, rects: rows },
  '3UpVertical': { count: 3, rects: columns },
  '3UpStacked': { count: 3, rects: stacked },
  '4Up': { count: 4, rects: (n) => grid(n, 2) },
  '4UpVertical': { count: 4, rects: columns },
  '4UpHorizontal': { count: 4, rects: rows },
  '4UpStacked': { count: 4, rects: stacked },
  '5UpStacked': { count: 5, rects: stacked },
  '6Up': { count: 6, rects: (n) => grid(n, 3) },
  '6UpStacked': { count: 6, rects: stacked },
  tile: { count: null, rects: square },
};

/** Whether an arrangement can lay out `n` windows (at least two for the ones with no fixed number). */
export function fits(id: string, n: number): boolean {
  const a = ARRANGEMENTS[id];
  return !!a && (a.count === null ? n >= 2 : a.count === n);
}

/** The rects of `n` windows in an arrangement, or null when it does not fit that number. */
export function layoutRects(id: string, n: number): Rect[] | null {
  return fits(id, n) ? ARRANGEMENTS[id].rects(n) : null;
}

export const FLOAT_MIN = { w: 320, h: 220 };
const STEP = 32;

/** Floating rect number `i` of a cascade in an area of `w` by `h` pixels: each window a step down and right of the last. */
export function cascadeRect(i: number, area: { w: number; h: number }): Rect {
  const w = Math.max(FLOAT_MIN.w, Math.round(area.w * 0.7)), h = Math.max(FLOAT_MIN.h, Math.round(area.h * 0.7));
  // after the window no longer fits, the steps start over from the top left corner
  const room = Math.max(1, Math.floor(Math.min((area.w - w) / STEP, (area.h - h) / STEP)) + 1);
  const k = i % room;
  return { x: 12 + k * STEP, y: 12 + k * STEP, w, h };
}

/** The same rect kept inside an area (a window dragged or resized, an area that got smaller): its title stays reachable. */
export function keepInside(r: Rect, area: { w: number; h: number }): Rect {
  const w = Math.max(FLOAT_MIN.w, Math.min(r.w, area.w)), h = Math.max(FLOAT_MIN.h, Math.min(r.h, area.h));
  return { w, h, x: Math.max(0, Math.min(r.x, Math.max(0, area.w - 80))), y: Math.max(0, Math.min(r.y, Math.max(0, area.h - 32))) };
}
