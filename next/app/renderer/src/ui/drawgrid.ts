// Drawing a grid of frames (InDesign "draw multiple objects as a grid", tools-c1): while a rectangle, ellipse, polygon, their frame tools or the
// Type tool drags, Right/Left add and remove a column and Up/Down a row; Ctrl (Cmd) with the arrows widens or narrows the space between the cells
// (Right/Up wider, Left/Down narrower). The Polygon tools take Space with Up/Down for the number of sides and Space with Left/Right for the star
// inset (the engine's polygon settings: they stay for the next drawing, as InDesign's tool setting does). The cells are what the drag's box is cut
// into; the drawing is one edit of one `createByDrag` per cell. Plain TypeScript: no screen state here.
import type { Box } from '../inspect';
import type { DrawKind } from '../engine';

export type Grid = { cols: number; rows: number; gap: [number, number] };
export const NO_GRID: Grid = { cols: 1, rows: 1, gap: [12, 12] };
const MAX_CELLS = 40;
const GAP_STEP = 1;

/** Whether a kind can be drawn as a grid (a line cannot: InDesign keeps one line). */
export const griddable = (kind: DrawKind) => kind !== 'line';

/** The grid after an arrow key (`null`: not a grid key). `command`: Ctrl or Cmd held (the space between the cells changes). */
export function gridKey(g: Grid, key: string, command: boolean): Grid | null {
  const d = key === 'ArrowRight' || key === 'ArrowUp' ? 1 : key === 'ArrowLeft' || key === 'ArrowDown' ? -1 : 0;
  if (!d) return null;
  const across = key === 'ArrowRight' || key === 'ArrowLeft';
  if (command) {
    const gap: [number, number] = [...g.gap];
    const i = across ? 0 : 1;
    gap[i] = Math.max(0, Math.round((gap[i] + d * GAP_STEP) * 1000) / 1000);
    return { ...g, gap };
  }
  const clamp = (n: number) => Math.max(1, Math.min(MAX_CELLS, n));
  return across ? { ...g, cols: clamp(g.cols + d) } : { ...g, rows: clamp(g.rows + d) };
}

/** The polygon settings after Space with an arrow (Up/Down: one side more or less, 3 to 100; Left/Right: the star inset by 10 %, 0 to 100). */
export function polygonKey(s: { sides: number; inset: number }, key: string): { sides: number; inset: number } | null {
  if (key === 'ArrowUp' || key === 'ArrowDown') return { ...s, sides: Math.max(3, Math.min(100, Math.round(s.sides) + (key === 'ArrowUp' ? 1 : -1))) };
  if (key === 'ArrowRight' || key === 'ArrowLeft') return { ...s, inset: Math.max(0, Math.min(100, Math.round((s.inset + (key === 'ArrowRight' ? 10 : -10)) * 1e4) / 1e4)) };
  return null;
}

/** The cells a box is cut into, left to right then top to bottom; none when the space between them leaves no room. */
export function gridCells(rect: Box, g: Grid): Box[] {
  if (g.cols === 1 && g.rows === 1) return [rect];
  const w = (rect[2] - rect[0] - (g.cols - 1) * g.gap[0]) / g.cols, h = (rect[3] - rect[1] - (g.rows - 1) * g.gap[1]) / g.rows;
  if (!(w > 0.01 && h > 0.01)) return [];
  const out: Box[] = [];
  for (let r = 0; r < g.rows; r++) {
    for (let c = 0; c < g.cols; c++) {
      const x = rect[0] + c * (w + g.gap[0]), y = rect[1] + r * (h + g.gap[1]);
      out.push([x, y, x + w, y + h]);
    }
  }
  return out;
}

/**
 * Alt-dragging a selection with the Selection tool, the arrows make a grid of copies (tools-c2, the same keys and gaps as drawing a grid): the
 * area from where the selection was (`box`) to where it is dragged (`by`) is cut into the cells as `gridCells` cuts a drawn box, and each copy
 * keeps its size, its corner on the corner of its cell on the side the drag came from. The cell where the selection already is gets no copy.
 * Returns the offsets of the copies from the selection (empty: a plain 1 x 1 copy).
 */
export function copyGridOffsets(box: Box, by: [number, number], g: Grid): [number, number][] {
  if (g.cols === 1 && g.rows === 1) return [];
  const area: Box = [Math.min(box[0], box[0] + by[0]), Math.min(box[1], box[1] + by[1]), Math.max(box[2], box[2] + by[0]), Math.max(box[3], box[3] + by[1])];
  const cells = gridCells(area, g);
  const w = box[2] - box[0], h = box[3] - box[1];
  const out: [number, number][] = [];
  cells.forEach((c, i) => {
    const col = i % g.cols, row = Math.floor(i / g.cols);
    // the copy's corner sits on its cell's corner on the side of the original (a drag to the left: right edges, upwards: bottom edges)
    const x = by[0] >= 0 ? c[0] : c[2] - w, y = by[1] >= 0 ? c[1] : c[3] - h;
    const home = col === (by[0] >= 0 ? 0 : g.cols - 1) && row === (by[1] >= 0 ? 0 : g.rows - 1);
    if (!home) out.push([x - box[0], y - box[1]]);
  });
  return out;
}
