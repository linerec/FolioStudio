// Where a pointer is on a table's lines and how far a line was dragged (Type tool; InDesign 2025, rules/tabledrag-c1.ko.md).
// Pure geometry: no runtime imports, so the node test transpiles this file alone (scripts/test-tablelines.cjs).

export type Pt = [number, number];
/** `min` (rows): what the cells of the row before the line need at least (the layout's content height), the least the row may be dragged to. */
export type LineSeg = { line: number; from: Pt; to: Pt; size: number; min?: number };
export type TablePiece = { frame: string; page: number; columns: LineSeg[]; rows: LineSeg[] };
export type TableLineInfo = { table: string | null; pieces: TablePiece[]; columns?: number; rows?: number; header?: number };
/** `minAfter` (row lines): what the row after the line needs at least. */
export type LineHit = { table: string; columns: boolean; line: number; page: number; seg: LineSeg; outer: boolean; minAfter?: number };

/** Distance from `p` to the segment `a`-`b`. */
export function segDistance(p: Pt, a: Pt, b: Pt): number {
  const dx = b[0] - a[0], dy = b[1] - a[1], len2 = dx * dx + dy * dy;
  const t = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** The table line nearest to page point `pt` of `page` within `tol` points (a column line wins a tie at a crossing: the first found). */
export function lineAt(info: TableLineInfo | null, page: number, pt: Pt, tol: number): LineHit | null {
  if (!info?.table) return null;
  let best: LineHit | null = null, bestD = tol + 1e-9;
  for (const piece of info.pieces) {
    if (piece.page !== page) continue;
    for (const [columns, segs] of [[true, piece.columns], [false, piece.rows]] as const) {
      const n = columns ? info.columns : info.rows;
      for (const seg of segs) {
        const d = segDistance(pt, seg.from, seg.to);
        if (d < bestD) {
          bestD = d;
          const after = columns ? undefined : info.pieces.flatMap((q) => q.rows).find((r) => r.line === seg.line + 1)?.min;
          best = { table: info.table, columns, line: seg.line, page, seg, outer: seg.line === n, ...(after === undefined ? {} : { minAfter: after }) };
        }
      }
    }
  }
  return best;
}

/** The unit vector the line moves along when dragged: the frame's +x for a column line (its segment runs down the frame's +y), +y for a row line. */
export function lineNormal(hit: LineHit): Pt {
  const { from, to } = hit.seg;
  const len = Math.hypot(to[0] - from[0], to[1] - from[1]) || 1;
  const dx = (to[0] - from[0]) / len, dy = (to[1] - from[1]) / len;
  return hit.columns ? [dy, -dx] : [-dy, dx];
}

/** How far (points) the pointer has moved the line, along the line's normal. */
export const dragAlong = (hit: LineHit, from: Pt, to: Pt): number => {
  const n = lineNormal(hit);
  return (to[0] - from[0]) * n[0] + (to[1] - from[1]) * n[1];
};

/** The dragged line's segment moved by `by` along its normal (the preview). */
export function movedSeg(hit: LineHit, by: number): { from: Pt; to: Pt } {
  const n = lineNormal(hit), m = (p: Pt): Pt => [p[0] + n[0] * by, p[1] + n[1] * by];
  return { from: m(hit.seg.from), to: m(hit.seg.to) };
}

/** Whether the frame is not turned (the lines are axis-parallel): only then is the line's page position on InDesign's drag grid. */
export const axisParallel = (hit: LineHit): boolean => {
  const n = lineNormal(hit);
  return Math.abs(n[hit.columns ? 1 : 0]) < 1e-9;
};
