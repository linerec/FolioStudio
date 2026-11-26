// Live corners: the yellow square on a selected rectangle or text frame turns on four yellow diamonds (one per corner); dragging a diamond changes
// the corners, Alt-click cycles their shape. The rules (measured in InDesign 2025, next/oracle/rules/liveinput-c1.ko.md) live in the engine
// (`liveCornerDrag`, `cycleCornerShape`, edit/src/ops/livecorners.rs); the screen sends how far the diamond moved and which keys were down,
// and draws the diamonds with the same formula (the shared vectors are in livecorners.test.ts and the engine's unit tests).
import { corners as boxCorners, type Corner, type CornerShape, type ItemInfo } from '../../inspect';

export type Point = [number, number];

/** The pointer must move this many screen pixels before a diamond follows it (InDesign's dead zone, measured 4 to 10 px, 5 at 100 % to 200 %). */
export const DRAG_THRESHOLD_PX = 5;
/** Half the diagonal of a diamond and how close a press has to be to hit it (screen pixels). */
export const DIAMOND_HALF = 5;

const SHAPE_ORDER: CornerShape[] = ['None', 'RoundedCorner', 'FancyCorner', 'BevelCorner', 'InsetCorner', 'InverseRoundedCorner'];

/** The radius a diamond leaves after being dragged `by` points inward from `start`, in a frame whose shorter side is `shorter` (same as the engine). */
export const radiusAfter = (start: number, by: number, shorter: number) => Math.min(Math.max(start + by, 0), Math.max(shorter / 2, 0));

/** The shape after `shape` in the Alt-click cycle. */
export const nextShape = (shape: CornerShape): CornerShape => SHAPE_ORDER[(Math.max(SHAPE_ORDER.indexOf(shape), -1) + 1) % SHAPE_ORDER.length];

/** The corners after dragging corner `corner` (0 top left, 1 top right, 2 bottom left, 3 bottom right) inward by `by` points: what the diamonds show while dragging. */
export function draggedCorners(now: Corner[], corner: number, by: number, only: boolean, shorter: number): Corner[] {
  const had = now[corner].shape !== 'None';
  const radius = radiusAfter(had ? now[corner].radius : 0, by, shorter);
  return now.map((c, k) => (only && k !== corner ? c : { shape: !had && c.shape === 'None' ? 'RoundedCorner' : c.shape, radius }));
}

export const shorterSide = (it: ItemInfo) => Math.min(it.inner[2] - it.inner[0], it.inner[3] - it.inner[1]);

/** The frame's corners in screen pixels (`s` = pixels per point): top left, top right, bottom left, bottom right. */
function frame(it: ItemInfo, s: number): Point[] {
  const [tl, tr_, br, bl] = boxCorners(it.transform, it.inner);
  return [tl, tr_, bl, br].map(([x, y]) => [x * s, y * s] as Point);
}

/** Where the four diamonds are: on the frame's own top or bottom edge, `radius` in from the corner (on the corner while it has no shape). */
/** The corners an item has now (the reader's, or four corners with no shape). */
export const cornersOfItem = (it: ItemInfo): Corner[] => (it.corners?.length === 4 ? it.corners : [0, 1, 2, 3].map(() => ({ shape: 'None' as CornerShape, radius: 12 })));

export function diamondPlacement(it: ItemInfo, s: number, corners: Corner[] = cornersOfItem(it)): Point[] {
  const f = frame(it, s);
  const toward = [1, 0, 3, 2]; // the other end of the same edge
  return f.map((p, k) => {
    const q = f[toward[k]];
    const len = Math.hypot(q[0] - p[0], q[1] - p[1]);
    const r = corners[k].shape === 'None' || len < 1e-9 ? 0 : corners[k].radius * s;
    return len < 1e-9 ? p : [p[0] + ((q[0] - p[0]) / len) * r, p[1] + ((q[1] - p[1]) / len) * r];
  });
}

/**
 * How many points a diamond moved inward along its edge when the pointer went from `from` to `to` (screen pixels). Measured: InDesign follows the
 * screen axis (x or y) the frame's own horizontal edge is nearest to (45 degrees counts as x), in the direction that edge points into the frame
 * (a turned frame: 0 to 45 and 135 to 225 degrees follow x, 60 to 120 follow y), and the first `DRAG_THRESHOLD_PX` of it do nothing.
 */
export function dragBy(it: ItemInfo, corner: number, from: Point, to: Point, s: number): number {
  const f = frame(it, s);
  const ux = f[1][0] - f[0][0], uy = f[1][1] - f[0][1];
  if (Math.hypot(ux, uy) < 1e-9 || !(s > 0)) return 0;
  const axis = Math.abs(ux) >= Math.abs(uy) ? 0 : 1;
  const inward = (corner % 2 === 0 ? 1 : -1) * (axis === 0 ? ux : uy); // along the frame's top edge, into the frame
  const d = (to[axis] - from[axis]) * (inward >= 0 ? 1 : -1);
  return (Math.sign(d) * Math.max(0, Math.abs(d) - DRAG_THRESHOLD_PX)) / s;
}

