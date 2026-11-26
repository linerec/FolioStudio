// Path points for the Pen and Direct Selection tools: the same rules as the engine's (edit/src/ops/pathedit.rs, measured
// against InDesign: oracle basic-paths), here only to show a drag before it is applied. No runtime imports: the node test
// (scripts/test-pathgeom.cjs) transpiles this file alone.

/** `[anchor x, y, left x, y, right x, y]`, as the engine lists a path point. */
export type Pt = [number, number, number, number, number, number];

export type Side = 'left' | 'right';

/** Two handles in line on opposite sides of the anchor (a handle with no length is in line with any other). */
export function isSmooth(p: Pt): boolean {
  const l = [p[2] - p[0], p[3] - p[1]], r = [p[4] - p[0], p[5] - p[1]];
  const ll = Math.hypot(l[0], l[1]), rl = Math.hypot(r[0], r[1]);
  if (ll < 1e-9 || rl < 1e-9) return true;
  const cross = (l[0] * r[1] - l[1] * r[0]) / (ll * rl);
  const dot = (l[0] * r[0] + l[1] * r[1]) / (ll * rl);
  return Math.abs(cross) < 1e-6 && dot < 0;
}

/** The anchors `indices` and their handles moved by `d`. */
export function moveAnchors(points: Pt[], indices: number[], d: [number, number]): Pt[] {
  return points.map((p, i) => (indices.includes(i) ? [p[0] + d[0], p[1] + d[1], p[2] + d[0], p[3] + d[1], p[4] + d[0], p[5] + d[1]] as Pt : p));
}

/** One handle moved to `to`; a smooth point turns its other handle to stay opposite, at its own length (not when `independent`). */
export function moveHandle(points: Pt[], index: number, side: Side, to: [number, number], independent = false): Pt[] {
  const p = points[index];
  const smooth = isSmooth(p);
  const q: Pt = [...p];
  const [h, o] = side === 'left' ? [2, 4] : [4, 2];
  q[h] = to[0];
  q[h + 1] = to[1];
  if (smooth && !independent) {
    const dir = [to[0] - p[0], to[1] - p[1]];
    const d = Math.hypot(dir[0], dir[1]);
    if (d > 1e-9) {
      const keep = Math.hypot(p[o] - p[0], p[o + 1] - p[1]);
      q[o] = p[0] - (dir[0] / d) * keep;
      q[o + 1] = p[1] - (dir[1] / d) * keep;
    }
  }
  return points.map((x, i) => (i === index ? q : x));
}

/** An SVG path through the points (curves where a handle is off its anchor), in the units of the points times `k`. */
export function svgPath(points: Pt[], open: boolean, k = 1): string {
  if (!points.length) return '';
  const f = (v: number) => (v * k).toFixed(2);
  const seg = (a: Pt, b: Pt) => (a[4] === a[0] && a[5] === a[1] && b[2] === b[0] && b[3] === b[1]
    ? `L${f(b[0])},${f(b[1])}`
    : `C${f(a[4])},${f(a[5])} ${f(b[2])},${f(b[3])} ${f(b[0])},${f(b[1])}`);
  let d = `M${f(points[0][0])},${f(points[0][1])}`;
  for (let i = 1; i < points.length; i++) d += seg(points[i - 1], points[i]);
  if (!open && points.length > 1) d += seg(points[points.length - 1], points[0]) + 'Z';
  return d;
}

/** The point of the segment from `a` to `b` nearest to `p`: how far along it (0 to 1) and how far away it is. */
export function nearestOnSegment(a: Pt, b: Pt, p: [number, number]): { t: number; dist: number } {
  // a straight segment (no handle off either anchor): `t` is the share of its length, as the engine splits it (addPathPoint lerps the
  // anchors); its cubic with the handles on the anchors runs at another pace (3t² − 2t³), so the cubic's t would put the point elsewhere
  if (a[4] === a[0] && a[5] === a[1] && b[2] === b[0] && b[3] === b[1]) {
    const dx = b[0] - a[0], dy = b[1] - a[1], len2 = dx * dx + dy * dy;
    const t = len2 > 0 ? Math.min(1, Math.max(0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2)) : 0;
    return { t, dist: Math.hypot(a[0] + dx * t - p[0], a[1] + dy * t - p[1]) };
  }
  // the cubic: anchor, its right handle, the next point's left handle, its anchor
  const pointAt = (t: number): [number, number] => {
    const u = 1 - t;
    return [
      u * u * u * a[0] + 3 * u * u * t * a[4] + 3 * u * t * t * b[2] + t * t * t * b[0],
      u * u * u * a[1] + 3 * u * u * t * a[5] + 3 * u * t * t * b[3] + t * t * t * b[1],
    ];
  };
  const d = (t: number) => Math.hypot(pointAt(t)[0] - p[0], pointAt(t)[1] - p[1]);
  let best = 0, bestD = Infinity;
  for (let i = 0; i <= 48; i++) {
    const t = i / 48, v = d(t);
    if (v < bestD) { best = t; bestD = v; }
  }
  // a few rounds of narrowing around the best sample
  let lo = Math.max(0, best - 1 / 48), hi = Math.min(1, best + 1 / 48);
  for (let k = 0; k < 24; k++) {
    const m1 = lo + (hi - lo) / 3, m2 = hi - (hi - lo) / 3;
    if (d(m1) < d(m2)) hi = m2; else lo = m1;
  }
  const t = (lo + hi) / 2;
  return { t, dist: d(t) };
}

/** Shift with the Pen and Direct Selection tools: `to` put on the nearest of the eight directions (every 45°) from `from`, at the
 *  length of its projection on that direction (a horizontal line keeps the pointer's x). */
export function constrain45(from: [number, number], to: [number, number]): [number, number] {
  const dx = to[0] - from[0], dy = to[1] - from[1];
  if (Math.hypot(dx, dy) < 1e-12) return [to[0], to[1]];
  const a = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
  const c = Math.cos(a), s = Math.sin(a);
  // cos and sin of a multiple of 45° are exact where they should be 0
  const cx = Math.abs(c) < 1e-12 ? 0 : c, sy = Math.abs(s) < 1e-12 ? 0 : s;
  const len = dx * cx + dy * sy;
  return [from[0] + cx * len, from[1] + sy * len];
}

/**
 * Direct Selection dragging a segment from `a` to `b`, grabbed at `t` (0 to 1), by `d`: a straight segment (no handle off either anchor) moves
 * with its two anchors (`move`); a curve keeps its anchors and changes the two handles that shape it so the grabbed place follows the pointer
 * exactly (the handle nearer the grabbed place takes more of the change; a smooth anchor turns its other handle to stay in line).
 * Returns the two points as they become.
 */
export function reshapeSegment(a: Pt, b: Pt, t: number, d: [number, number]): { move: boolean; a: Pt; b: Pt } {
  const straight = a[4] === a[0] && a[5] === a[1] && b[2] === b[0] && b[3] === b[1];
  if (straight) {
    const mv = (p: Pt): Pt => [p[0] + d[0], p[1] + d[1], p[2] + d[0], p[3] + d[1], p[4] + d[0], p[5] + d[1]];
    return { move: true, a: mv(a), b: mv(b) };
  }
  const u = Math.min(Math.max(t, 0.02), 0.98);
  // the share of the change the far handle takes, smooth from 0 (grabbed near a) to 1 (near b)
  const w = u <= 1 / 6 ? 0 : u <= 0.5 ? Math.pow((6 * u - 1) / 2, 3) / 2 : u <= 5 / 6 ? (1 - Math.pow((6 * (1 - u) - 1) / 2, 3)) / 2 + 0.5 : 1;
  const k0 = (1 - w) / (3 * u * (1 - u) * (1 - u)), k1 = w / (3 * u * u * (1 - u));
  const ra: [number, number] = [a[4] + d[0] * k0, a[5] + d[1] * k0], lb: [number, number] = [b[2] + d[0] * k1, b[3] + d[1] * k1];
  const na = moveHandle([a], 0, 'right', ra)[0], nb = moveHandle([b], 0, 'left', lb)[0];
  return { move: false, a: na, b: nb };
}

/**
 * The handles of the points next to the selected ones that shape the segments touching a selected point: the right handle of the point
 * before and the left handle of the point after (the Direct Selection tool shows them with the selected points' own handles; screens-doc).
 * The ends of an open path have no neighbour past them, a closed path wraps round. A handle lying on its anchor is no handle and is left out,
 * and so is one of a selected point (its own handles are shown anyway).
 */
export function neighborHandles(points: Pt[], open: boolean, indices: number[]): { index: number; side: Side }[] {
  const n = points.length;
  const out: { index: number; side: Side }[] = [];
  const add = (index: number, side: Side) => {
    const p = points[index];
    if (!p || indices.includes(index) || out.some((o) => o.index === index && o.side === side)) return;
    const [hx, hy] = side === 'left' ? [p[2], p[3]] : [p[4], p[5]];
    if (hx !== p[0] || hy !== p[1]) out.push({ index, side });
  };
  for (const i of indices) {
    const before = i > 0 ? i - 1 : open ? -1 : n - 1;
    const after = i < n - 1 ? i + 1 : open ? -1 : 0;
    if (before >= 0 && before !== i) add(before, 'right');
    if (after >= 0 && after !== i) add(after, 'left');
  }
  return out;
}
