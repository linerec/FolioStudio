/** A spread turned by View ▸ Rotate Spread shows its pages turned (the row is drawn with a CSS quarter turn, clockwise). A drag
 *  on it is read as the page is shown, from the top-left corner of the page as shown; the engine (`createByDrag`) puts the
 *  shape in the spread's own space. Only the drag preview needs the own-space position of that rect: the same map as the
 *  engine's (rules/drawtools.toml, drawtools.turned.*), checked against the created item by drawtools-turned-ui-check.py. */
import type { Box } from '../inspect';

/** The quarter turn (0, 90, 180, 270) of a page's spread. */
export const quarterTurn = (turn: number | undefined): 0 | 90 | 180 | 270 => (turn === 90 || turn === 180 || turn === 270 ? turn : 0);

/** A point of the page as shown (from its top-left corner) in the page's own space (`width` × `height`, unturned). */
export function ownPoint(turn: number | undefined, width: number, height: number, [x, y]: [number, number]): [number, number] {
  switch (quarterTurn(turn)) {
    case 90: return [y, height - x];
    case 180: return [width - x, height - y];
    case 270: return [width - y, x];
    default: return [x, y];
  }
}

/** The inverse of `ownPoint`: a point of the page's own space where it is shown (from the top-left corner of the page as shown). */
export function shownPoint(turn: number | undefined, width: number, height: number, [x, y]: [number, number]): [number, number] {
  switch (quarterTurn(turn)) {
    case 90: return [height - y, x];
    case 180: return [width - x, height - y];
    case 270: return [y, width - x];
    default: return [x, y];
  }
}

/** A move as shown (a pointer drag, an arrow key: right, down) in the page's own space: the quarter turn taken back, so the object goes the way
 *  the pointer went on the screen. The one map of every move on a turned spread (Selection, Direct Selection, the arrow keys, the transform tools). */
export function ownDelta(turn: number | undefined, [dx, dy]: [number, number]): [number, number] {
  switch (quarterTurn(turn)) {
    case 90: return [dy, -dx];
    case 180: return [-dx, -dy];
    case 270: return [-dy, dx];
    default: return [dx, dy];
  }
}

/** A rect as shown in the page's own space; a line keeps its two end points as given. */
export function ownRect(turn: number | undefined, width: number, height: number, r: Box, line = false): Box {
  if (!quarterTurn(turn)) return r;
  const a = ownPoint(turn, width, height, [r[0], r[1]]), b = ownPoint(turn, width, height, [r[2], r[3]]);
  return line ? [a[0], a[1], b[0], b[1]] : [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])];
}
