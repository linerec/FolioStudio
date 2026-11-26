// Smart Guides for a single point (pen-c2): where the Pen puts a point, where the Direct Selection tool drags an anchor or a handle, where the
// point tools drag a handle. With View ▸ Grids & Guides ▸ Smart Guides on, the point is caught (within the Snap to Zone, screen pixels) by
//   1. an anchor point of another path (or of the same path, not the points being moved): the point goes onto it exactly, the anchor is
//      marked while the pointer is there;
//   2. else, along each axis on its own, the edges (Align to Object Edges) and the middles (Align to Object Center) of the other objects,
//      the page's middle, edges and margins, the same lines a dragged object aligns to (ui/smartguides.ts) and drawn the same way.
// Folio's rule (InDesign's Pen follows the Smart Guides and its own anchor hint; the zone is the document's Snap to Zone, as for objects).
// Pure functions in page points; no runtime imports beyond smartguides (node test: scripts/test-pointsnap.cjs).
import type { Box, ItemInfo, PageItems } from '../inspect';
import { type SmartDraw, type SmartPage, type SmartPrefs, alignLines, noDraw } from './smartguides';

type Pt = [number, number];
/** What a point snapped to: where it goes, the anchor it landed on (null: none), the lines to draw, which axes were pulled. */
export type PointSnap = { pt: Pt; anchor: Pt | null; draw: SmartDraw; pulled: [boolean, boolean] };
/** Points of a path that are not targets (the ones being moved): item id, path index, point indices (empty: every point of that path). */
export type SkipPoints = { id: string; path: number; indices: number[] } | null;

/** The anchor points (page points) of the visible paths of `items` and, shifted by their place, of the other pages of the spread. */
export function anchorPoints(items: PageItems | null | undefined, skip: SkipPoints, others: { items: PageItems | null | undefined; dx: number; dy: number }[] = []): Pt[] {
  const out: Pt[] = [];
  const walk = (list: ItemInfo[], dx: number, dy: number) => {
    for (const it of list) {
      if (!it.visible) continue;
      const m = it.transform;
      (it.paths ?? []).forEach((sp, pi) => sp.points.forEach((q, k) => {
        if (skip && skip.id === it.id && skip.path === pi && (!skip.indices.length || skip.indices.includes(k))) return;
        out.push([m[0] * q[0] + m[2] * q[1] + m[4] + dx, m[1] * q[0] + m[3] * q[1] + m[5] + dy]);
      }));
      if (it.children?.length) walk(it.children, dx, dy);
    }
  };
  for (const pg of [{ items, dx: 0, dy: 0 }, ...others]) if (pg.items) walk(pg.items.items, pg.dx, pg.dy);
  return out;
}

/** The lines along one axis a point can be pulled onto: object edges and middles, the page's middle, edges and margins (as the prefs allow). */
function lines(axis: 0 | 1, boxes: Box[], page: SmartPage | undefined, p: SmartPrefs): number[] {
  const out: number[] = [];
  for (const b of boxes) {
    if (p.edges) out.push(b[axis], b[axis + 2]);
    if (p.center) out.push((b[axis] + b[axis + 2]) / 2);
  }
  if (page) {
    const len = page.size[axis];
    if (p.center) out.push(len / 2);
    if (p.edges) {
      out.push(0, len);
      const m = page.margins; // top, left, bottom, right
      if (m) out.push(axis === 0 ? m[1] : m[0], len - (axis === 0 ? m[3] : m[2]));
    }
  }
  return out;
}

/** A point `pt` caught by the Smart Guides: `anchors` (page points) first, then the alignment lines of `boxes` and the page, within `zone` (points). */
export function smartPoint(pt: Pt, anchors: Pt[], boxes: Box[], page: SmartPage | undefined, p: SmartPrefs, zone: number): PointSnap {
  const none: PointSnap = { pt, anchor: null, draw: noDraw(), pulled: [false, false] };
  if (!p.enabled || !(zone > 0)) return none;
  let best: Pt | null = null, bestD = zone;
  for (const a of anchors) {
    const d = Math.hypot(a[0] - pt[0], a[1] - pt[1]);
    if (d <= bestD) { best = a; bestD = d; }
  }
  if (best) return { pt: [best[0], best[1]], anchor: best, draw: noDraw(), pulled: [true, true] };
  const at: Pt = [pt[0], pt[1]];
  const pulled: [boolean, boolean] = [false, false];
  for (const axis of [0, 1] as const) {
    let d0 = zone, v: number | null = null;
    for (const l of lines(axis, boxes, page, p)) {
      const d = Math.abs(l - pt[axis]);
      if (d <= d0) { d0 = d; v = l; }
    }
    if (v !== null) { at[axis] = v; pulled[axis] = true; }
  }
  if (!pulled[0] && !pulled[1]) return none;
  // the lines through the point where it is now, drawn as an object's (a point is a box of no size)
  const box: Box = [at[0], at[1], at[0], at[1]];
  const draw = noDraw();
  draw.lines = alignLines(box, boxes, page, p).filter((l) => (l.axis === 'x' ? pulled[0] : pulled[1]));
  return { pt: at, anchor: null, draw, pulled };
}
