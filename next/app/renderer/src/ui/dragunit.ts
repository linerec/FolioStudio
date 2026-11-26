// How InDesign 2025 rounds what a drag in the document window does (real mouse, docinput-c3, rules/docinput-c3.ko.md):
//  - the pointer position of a handle drag, a move, a ruler guide and the drawing tools lands on a grid whose unit depends on the zoom;
//  - Shift on a ruler guide pulls it onto the ruler's ticks, whose spacing depends on the zoom the same way.
// Measured at 56 % and 100 % (0.75 pt), 150 % (0.5), 200 % (1/3), 300 and 400 % (0.2), 800 % (0.1) for the unit and at 56-100 % (3 pt),
// 200 % (2 pt), 400 % (1.5 pt) for the ticks (and at 20 more zooms from 125 to 1200 %: rulerTickStep); for the unit the values between the measured
// zooms are the nearest measured one. InDesign rounds a dragged
// position DOWN to the grid (tabledrag-c1, rules/tabledrag-c1.ko.md; pairs of the unrounded and the dropped position of a ruler guide at fit page,
// 100, 200, 400, 800 %, approached from either side), counting the pointer pixel from its middle. No runtime imports: the node test transpiles this file alone.

// (A thousandth of a unit is added before rounding down: a position that is on the grid within the pixel math's noise stays on it.)

/** InDesign's zoom (percent) for a view that shows `pxPerPt` CSS pixels per point: its 100 % is the 96 dpi actual size, 4/3 px per point. */
export const inDesignZoom = (pxPerPt: number) => pxPerPt * 75;

/** The units InDesign rounds a pointer position to, smallest first (points). */
const POINTER_UNITS = [0.1, 0.2, 1 / 3, 0.5, 0.75, 1, 2, 3];

/** The unit (points) the pointer position is rounded to at `zoom` percent (InDesign's scale): the smallest of 0.1, 0.2, 1/3, 0.5, 0.75, 1, 2, 3 that is at least
 * `60 / zoom` points (1.25 screen pixels of the measuring PC's display). One constant explains all measured zooms (rules/draground-c1.ko.md): 800 % 0.1, 300-400 % 0.2,
 * 200 % 1/3, 150 % 0.5, 100-115 % 0.75, 60-75 % 1, 50-56 % 2, 25 % 3 (line drawing, 240-320 coordinates per zoom, the unit's fit 84-100 %; docinput-c3 had read 0.75 at 56 %,
 * which this measurement does not see: 2 pt in 85 of 88 coordinates). Below 20 % (above 3 pt) not measured: it stays 3. */
export function pointerUnit(zoom: number): number {
  const need = 60 / Math.max(zoom, 1);
  for (const u of POINTER_UNITS) if (u >= need - 1e-9) return u;
  return POINTER_UNITS[POINTER_UNITS.length - 1];
}

/** The spacing (points) of the ruler ticks a Shift-dragged guide snaps to at `zoom` percent. */
export function rulerTickStep(zoom: number): number {
  // smartguides-c3 (rules/smartguides-c3.ko.md): 20 zooms from 125 to 1200 %, Shift-drops of a ruler guide at 7-8 places each. 3 pt up to 146.875 %,
  // 2 pt from 150 to 268.75 %, 1.5 pt from 275 to 500 %, 1 pt from 537.5 to 900 %, 0.5 pt at 1000 and 1200 %. Each boundary lies between the two
  // measured zooms next to it (146.875|150, 268.75|275, 500|537.5, 900|1000). Below 56 % and above 1200 % are not measured.
  if (zoom < 150) return 3;
  if (zoom < 275) return 2;
  if (zoom <= 500) return 1.5;
  if (zoom < 1000) return 1;
  return 0.5;
}

/** `v` rounded to the nearest multiple of `unit` (offset by `origin`). */
export const roundTo = (v: number, unit: number, origin = 0) => Math.round((v - origin) / unit) * unit + origin;

/** A pointer position `v` (points, the pointer pixel's left/top edge) on InDesign's drag grid at `zoom` percent: rounded down to the unit, the
 * pointer counted from its pixel's middle (about a quarter of a unit: guides 0.1-0.35, drawn rectangles 0.2-0.35, the Gap tool's distance 0.2-0.3,
 * docdrag-c1 rules/docdrag-c1.ko.md). */
export const snapPointer = (v: number, zoom: number): number => {
  const unit = pointerUnit(zoom);
  return Math.floor(v / unit + 0.25 + 1e-3) * unit;
};

/** The edge a handle drag puts down (page position `v` = the edge where it was plus how far the pointer went): rounded down onto the grid like a pointer
 * position. Measured with off-grid edges and the exact pixel map (docdrag-c1, rules/docdrag-c1.ko.md): the right/bottom edge `floor(v / unit + 0.3)`
 * (right edge dragged out 29/35, in 29/36), the left/top edge `+ 0.2` (out 34/40, in 32/40). Earlier constants (0.85 / 0.4) came from edges that
 * were already on the grid and a press a whole pixel off the handle: that fixed phase looks like a bigger `c`. */
export const snapEdge = (v: number, zoom: number, far: boolean): number => {
  const unit = pointerUnit(zoom);
  return Math.floor(v / unit + (far ? 0.3 : 0.2) + 1e-3) * unit;
};

/** The top-left corner a move puts down (page position `v` = where the corner was plus how far the pointer went): rounded down onto the grid (docdrag-c1:
 * 60 moves of an off-grid rectangle, the left edge on the grid in 52, the top in 60; `floor(v / unit + 0)`; the other 8 had the right edge on the grid
 * instead, which no input of ours told apart). */
export const snapCorner = (v: number, zoom: number): number => {
  const unit = pointerUnit(zoom);
  return Math.floor(v / unit + 1e-3) * unit;
};

/** The way a Shift-drawn line goes (draground-c1, rules/draground-c1.ko.md): `v` is the vector between the two pointer points AFTER they were put on the grid.
 * The angle is taken to the nearest multiple of 45 degrees; along an axis the line keeps the longer way, on a diagonal both ways become the SHORTER of the two
 * (not the length of the drag): 18/19 axis lines and 16/21 diagonals as modelled, the longer or the mean way 0/21. */
export function shiftLine(v: [number, number]): [number, number] {
  const [vx, vy] = v;
  const k = (Math.round(Math.atan2(vy, vx) / (Math.PI / 4)) + 8) % 8;
  if (k === 0 || k === 4) return [vx, 0];
  if (k === 2 || k === 6) return [0, vy];
  const l = Math.min(Math.abs(vx), Math.abs(vy));
  return [vx < 0 ? -l : l, vy < 0 ? -l : l];
}

/** Whether a matrix only scales and moves (no turn, no shear, no flip): its box is the same on the page as in its own space. */
export const flatMatrix = (m: readonly number[]): boolean => Math.abs(m[1]) < 1e-9 && Math.abs(m[2]) < 1e-9 && m[0] > 0 && m[3] > 0;

/** A handle drag's pointer displacement `(dx, dy)` (page points, from where the pointer was pressed at `from`) with the dragged handle put on InDesign's drag grid;
 * `gx`/`gy`: no guide or gridline pulled that axis. `at`: where the handle stood, `(fx, fy)` which handle (0, 0.5 or 1 each), `m` the item's matrix. The result is again
 * a displacement of the HANDLE: it goes to the grid point of the pointer itself, not of where the handle was plus how far the pointer went (draground-c1: an unrotated
 * corner handle pressed up to 3 px beside the handle ended on the pointer's grid point in 28 of 40, on the handle's own 2 of 40). Not for an item on a turned spread. */
export function handleBy(d: { fx: number; fy: number; at: [number, number]; from: [number, number]; m: readonly number[] }, dx: number, dy: number, gx: boolean, gy: boolean, zoom: number): [number, number] {
  const px = d.from[0] + dx, py = d.from[1] + dy;
  if (d.m[1] || d.m[2]) {
    // a turned item: the pointer's grid point in the page's x and y, then the handle's own axes take what they need of it (corner handles on the grid 36 of 36 at 30
    // degrees; edge handles, Shift and Alt: the rounded pointer worked into the size in 75-83 % of 30-36 trials at 15, 30 and 45 degrees, as for an unrotated item)
    if (gx) dx = snapPointer(px, zoom) - d.at[0];
    if (gy) dy = snapPointer(py, zoom) - d.at[1];
    return [dx, dy];
  }
  // the right and bottom edge and the left and top edge differ a little in where they round (a flipped item has them the other way round)
  if (gx && d.fx !== 0.5) dx = snapEdge(px, zoom, (d.fx === 1) !== (d.m[0] < 0)) - d.at[0];
  if (gy && d.fy !== 0.5) dy = snapEdge(py, zoom, (d.fy === 1) !== (d.m[3] < 0)) - d.at[1];
  return [dx, dy];
}
