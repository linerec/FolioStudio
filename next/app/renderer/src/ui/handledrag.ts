// Dragging a selection handle (Shift keeps the proportions, Alt resizes about the middle) and turning from just outside a
// handle (Shift: 45° steps). The document changes through the engine commands `resizeByHandle` and `rotateByDrag`, which work
// out the numbers (ops/handledrag.rs); this twin only draws the preview while the pointer moves, and both are tested against the
// same vectors (handledrag.vectors.json: cargo test -p folio-edit --test handledrag, node scripts/test-handledrag.cjs).
// No runtime imports: the node test transpiles this file alone.

/** The smallest width or height a handle drag leaves (points). InDesign has none of its own (the smallest it was seen to leave is 0.2 pt). */
export const HANDLE_MIN_SIZE = 0.1;
/** How far outside a corner handle (screen pixels) the pointer still turns the selection. InDesign 2025 on a 125 % display turns from
 *  about 8 to 18 device pixels off the corner point (6 to 14 px at 100 %) and only at the four corners, not at the side handles. */
export const ROTATE_ZONE_PX = 15;

type Box = [number, number, number, number];
const side = (f: number) => (f < 0.25 ? -1 : f > 0.75 ? 1 : 0);
/** Rounds half away from zero, as Rust's `f64::round`. */
const roundAway = (v: number) => Math.sign(v) * Math.round(Math.abs(v));

const sign = (v: number) => (v < 0 ? -1 : 1);

/** The box `b` after its `handle` ([fx, fy], each 0, 0.5 or 1) is dragged by `d` (in the box's own axes), and per axis whether the
 *  handle went past the opposite edge (the middle with Alt): the box then lies on the other side of the staying edge and the object is
 *  mirrored about that axis (InDesign 2025). The size is the signed distance from the staying edge to the pointer. */
export function handleBox(b: Box, handle: [number, number], d: [number, number], keep: boolean, center: boolean): { rect: Box; flip: [boolean, boolean] } {
  const w = b[2] - b[0], h = b[3] - b[1];
  if (!(w > 0 && h > 0)) return { rect: b, flip: [false, false] };
  const sx = side(handle[0]), sy = side(handle[1]), k = center ? 2 : 1;
  let nw = sx === 0 ? w : w + sx * d[0] * k;
  let nh = sy === 0 ? h : h + sy * d[1] * k;
  if (keep && (sx !== 0 || sy !== 0)) {
    const fw = nw / w, fh = nh / h;
    // a corner: the larger absolute factor (InDesign 2025), each axis keeps its own sign; a side: the dragged side decides
    const s = Math.max(sx !== 0 && sy !== 0 ? Math.max(Math.abs(fw), Math.abs(fh)) : sx !== 0 ? Math.abs(fw) : Math.abs(fh), HANDLE_MIN_SIZE / w, HANDLE_MIN_SIZE / h);
    nw = w * s * (sx !== 0 ? sign(fw) : 1);
    nh = h * s * (sy !== 0 ? sign(fh) : 1);
  } else {
    if (sx !== 0 && Math.abs(nw) < HANDLE_MIN_SIZE) nw = HANDLE_MIN_SIZE * sign(nw);
    if (sy !== 0 && Math.abs(nh) < HANDLE_MIN_SIZE) nh = HANDLE_MIN_SIZE * sign(nh);
  }
  // the edge (or middle) that stays; a negative size puts the box on the other side of it
  const span = (lo: number, hi: number, n: number, s: number): [number, number] => {
    if (center || s === 0) {
      const m = (lo + hi) / 2;
      return [m - Math.abs(n) / 2, m + Math.abs(n) / 2];
    }
    if (s > 0) return n >= 0 ? [lo, lo + n] : [lo + n, lo];
    return n >= 0 ? [hi - n, hi] : [hi, hi - n];
  };
  const [x0, x1] = span(b[0], b[2], nw, sx), [y0, y1] = span(b[1], b[3], nh, sy);
  return { rect: [x0, y0, x1, y1], flip: [sx !== 0 && nw < 0, sy !== 0 && nh < 0] };
}

/** The box after the drag (see handleBox), without the mirroring. */
export function handleRect(b: Box, handle: [number, number], d: [number, number], keep: boolean, center: boolean): Box {
  return handleBox(b, handle, d, keep, center).rect;
}

/** The turn (degrees, counter-clockwise, y down) from `from` to `to` about `center`; `constrain` rounds it to a multiple of 45°. */
export function turnAngle(center: [number, number], from: [number, number], to: [number, number], constrain: boolean): number | null {
  if (Math.hypot(from[0] - center[0], from[1] - center[1]) < 1e-6 || Math.hypot(to[0] - center[0], to[1] - center[1]) < 1e-6) return null;
  let a = (Math.atan2(center[1] - to[1], to[0] - center[0]) - Math.atan2(center[1] - from[1], from[0] - center[0])) * 180 / Math.PI;
  a = ((((a + 540) % 360) + 360) % 360) - 180;
  return constrain ? roundAway(a / 45) * 45 : a;
}
