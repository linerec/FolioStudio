// Snapping while dragging (View ▸ Grids & Guides ▸ Snap to Guides / Snap to Document Grid). Guides are the page edges,
// the margin and column guides and the ruler guides; they pull an object's edges and middle onto them. The document grid
// pulls its edges onto a gridline, subdivisions included. Both work within a zone of a few screen pixels, as InDesign's
// Snap to Zone (default 4 px); a nearer target wins. Points from the page's top-left, y down. No runtime imports: the
// node test (scripts/test-snap.cjs) transpiles this file alone.
import type { Box, PageItems } from '../inspect';

/** InDesign's Snap to Zone (Preferences ▸ Guides & Pasteboard), in screen pixels. */
export const SNAP_ZONE_PX = 4;

/** A document grid along one axis: a line every `gap`, counted from `from` (the page's place on the spread). */
export type GridAxis = { gap: number; from: number };

export type Targets = { x: number[]; y: number[]; gridX: GridAxis | null; gridY: GridAxis | null };

/** The column guides of a page, as drawn: pairs (a column's right edge, the next one's left edge) between the margins. */
export function columnLines(items: PageItems, width: number, margins: [number, number, number, number]): number[] {
  const cols = items.columns;
  const xs: number[] = [];
  if (!(cols.count > 1 || cols.positions)) return xs;
  if (cols.positions && cols.positions.length >= 2) {
    xs.push(...cols.positions.slice(1, -1));
  } else {
    const ml = margins[1], inner = width - ml - margins[3];
    const cw = (inner - (cols.count - 1) * cols.gutter) / cols.count;
    for (let i = 1; i < cols.count; i++) {
      const x = ml + i * cw + (i - 1) * cols.gutter;
      xs.push(x, x + cols.gutter);
    }
  }
  return xs;
}

export type SnapInput = {
  items: PageItems | null | undefined;
  size: [number, number];
  margins: [number, number, number, number];
  /** Snap to Guides is on and guides are showing. */
  guides: boolean;
  /** Layers whose guides are hidden. */
  hiddenGuides: (layer: string | null | undefined) => boolean;
  /** Snap to Document Grid: the grid's lines and where this page sits on the spread. */
  grid: { horizontal: number; vertical: number; horizontalSub: number; verticalSub: number } | null;
  pageLeft: number;
  pageTop: number;
};

export function snapTargets(s: SnapInput): Targets {
  const t: Targets = { x: [], y: [], gridX: null, gridY: null };
  const [w, h] = s.size;
  if (s.guides) {
    t.x.push(0, w);
    t.y.push(0, h);
    const [mt, ml, mb, mr] = s.margins;
    if (mt || ml || mb || mr) {
      t.x.push(ml, w - mr);
      t.y.push(mt, h - mb);
    }
    if (s.items) {
      t.x.push(...columnLines(s.items, w, s.margins));
      for (const g of s.items.guides) {
        if (s.hiddenGuides(g.layer)) continue;
        (g.orientation === 'horizontal' ? t.y : t.x).push(g.location);
      }
    }
  }
  if (s.grid) {
    const gap = (step: number, sub: number) => step / Math.max(1, Math.round(sub));
    const gx = gap(s.grid.vertical, s.grid.verticalSub), gy = gap(s.grid.horizontal, s.grid.horizontalSub);
    if (gx > 0) t.gridX = { gap: gx, from: s.pageLeft };
    if (gy > 0) t.gridY = { gap: gy, from: s.pageTop };
  }
  return t;
}

/** The shift that brings the nearest of `edges` onto a target, 0 when none is within `zone`. */
function pull(edges: number[], list: number[], grid: GridAxis | null, gridEdges: number[], zone: number): number {
  let best = 0, nearest = zone + 1e-9;
  const take = (shift: number) => { if (Math.abs(shift) < nearest) { nearest = Math.abs(shift); best = shift; } };
  for (const e of edges) for (const t of list) take(t - e);
  if (grid) for (const e of gridEdges) take(Math.round((e + grid.from) / grid.gap) * grid.gap - grid.from - e);
  return best;
}

/** How far to move a box (page points) so that its edges and middle meet the guides, and its edges the grid. */
export function snapBox(box: Box, t: Targets, zone: number): [number, number] {
  const xs = [box[0], (box[0] + box[2]) / 2, box[2]], ys = [box[1], (box[1] + box[3]) / 2, box[3]];
  return [pull(xs, t.x, t.gridX, [box[0], box[2]], zone), pull(ys, t.y, t.gridY, [box[1], box[3]], zone)];
}

/** How far to move a point (a handle being dragged), per axis. */
export function snapPoint(p: [number, number], t: Targets, zone: number, axes: { x: boolean; y: boolean } = { x: true, y: true }): [number, number] {
  return [axes.x ? pull([p[0]], t.x, t.gridX, [p[0]], zone) : 0, axes.y ? pull([p[1]], t.y, t.gridY, [p[1]], zone) : 0];
}
