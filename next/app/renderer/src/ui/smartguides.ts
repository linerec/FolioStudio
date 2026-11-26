// Smart Guides (View ▸ Grids & Guides ▸ Smart Guides; Preferences ▸ Guides & Pasteboard: Align to Object Center / Edges, Smart Dimensions,
// Smart Spacing). Pure functions in page points (y down); No runtime imports (node test: scripts/test-smartguides.cjs).
// Rules: next/oracle/rules/smartguides-c2.ko.md (InDesign 2025, real mouse, ~200 drags, screenshots taken while the button is held).
//   alignment  an edge or the middle of the dragged objects is pulled onto an edge or the middle of another object, onto the page's middle, the
//              page edges and the margins, within the Snap to Zone (4 px); edge-to-edge needs "Align to Object Edges", anything with a middle
//              "Align to Object Center". Drawn: a 1 px light-green line through the aligned coordinate, from the dragged objects to the
//              farthest aligned object (the page middle: a pink line over the whole page height).
//   spacing    the dragged object, in a row (or column) with objects it is aligned with, is pulled so that its gap equals the gap of the two
//              objects next to it (between two of them: both gaps equal; beside a pair: the gap of the pair). Zone 1.5 x (between: 2 x).
//              Drawn: a double-headed arrow in every equal gap, 11 px below the lowest bottom edge (right of the rightmost edge in a column).
//   size       a handle drag that makes the width (height) equal to another object's is pulled onto it within the zone; drawn: a bracket
//              (a line with a cap at each end) 10 px below (right of) both objects.
//   Shift      switches all of it off during that drag; Alt (a copy): the original stays where it is and is a target like the others.
// rules/smartguides-c3.ko.md (2026-11-12, about 110 more drags):
//   a turned object and a group count with their bounding box (as a target and as the dragged one: not the corners of the turned shape, not the
//   members of the group); equal size compares the width and height of a turned target along its own sides (its bracket is drawn turned with it);
//   an object that lies wholly on the pasteboard is no target and is not pulled; the objects on the other pages of the spread are targets; the
//   page edges and the margins catch the middle too; an equal-spacing place within its zone wins over a nearer alignment; beside the spacing
//   arrows the lines join the dragged object with one partner only; a handle drag of a turned object compares its own width / height.
import type { Box, PageItems } from '../inspect';

/** `color`: the interface colour the lines are drawn in (UIColors name as the preference spells it, GRID_GREEN by default; ui/smartcolor.ts). */
export type SmartPrefs = { enabled: boolean; center: boolean; edges: boolean; dimensions: boolean; spacing: boolean; color?: string };
export const SMART_ALL: SmartPrefs = { enabled: true, center: true, edges: true, dimensions: true, spacing: true };

/** Drawing, in page points plus the screen-pixel offsets the overlay adds (SMART_PX). */
export type SmartLine = { axis: 'x' | 'y'; at: number; from: number; to: number; kind: 'object' | 'page' };
export type SmartGap = { axis: 'h' | 'v'; from: number; to: number; /** the edge the arrow hangs off (y of the lowest bottom for 'h', x of the rightmost right for 'v') */ at: number;
  /** the bottom (right) edges of the two objects the gap lies between: each end mark starts 4 px past its own object and ends 7 px past the arrow */ ends?: [number, number] };
export type Seg = [number, number, number, number];
export type SmartSize = { axis: 'w' | 'h'; from: number; to: number; /** y of the bottom (w) / x of the right (h) edge */ at: number;
  /** a turned target: the side (page points, x1 y1 x2 y2) the bracket runs along and the unit vector pointing away from the object */ seg?: Seg; out?: [number, number] };
export type SmartDraw = { lines: SmartLine[]; gaps: SmartGap[]; sizes: SmartSize[] };
export const noDraw = (): SmartDraw => ({ lines: [], gaps: [], sizes: [] });
export const hasDraw = (d: SmartDraw | null | undefined): boolean => !!d && (d.lines.length > 0 || d.gaps.length > 0 || d.sizes.length > 0);

/** Screen pixels (measured, 3821 px wide capture, 1 px lines in rgb(156,222,156) on white; the page middle rgb(255,79,255)). */
export const SMART_PX = {
  /** InDesign's device pixels per CSS pixel on the measuring PC (125 % display scaling): the offsets below are device pixels */
  scale: 1.25,
  /** arrow baseline below the lowest bottom edge, and the tick length (the tick is centred on the baseline) */
  gapOffset: 11, gapTick: 15, gapHead: [6, 2] as [number, number],
  /** size bracket: distance from the edge and the cap length */
  sizeOffset: 10, sizeCap: 7,
};
export const SMART_COLOR = '#9cde9c';
export const SMART_PAGE_COLOR = '#ff4fff';

/** The zones, as factors of the Snap to Zone (4 px = 5 device px), measured per side of the target (`off` = how far the dragged line lies past
 *  the target in the direction of growing coordinates; InDesign pulled from this many device px before to this many after):
 *  edge to edge -4.5..+6.5 px, with a middle -4.5..+4.5 px (the page's middle -4.5..+6), equal gaps with the object between two others -9..+11 px, beside a pair -3.5..+10.5 px, equal size (width or height) -2.5..+5 px. */
export const ZONE = { neg: 0.9, pos: 1.3, centerPos: 0.9, pagePos: 1.2, betweenNeg: 1.8, betweenPos: 2.2, besideNeg: 0.8, besidePos: 2.1, sizeNeg: 0.5, sizePos: 1.0 };
/** s = target - line: inside the zone when the line is up to `pos` before the target or `neg` after it (a shift of -pos..+neg). */
const within = (s: number, zone: number, neg: number, pos: number): boolean => s >= -pos * zone - 1e-9 && s <= neg * zone + 1e-9;
const EPS = 0.02;

/** A target's box (its bounding box in page points). A turned object also carries its own width and height and the sides its size brackets hang off. */
export type SmartBox = Box & { local?: { w: number; h: number; bottom: Seg; right: Seg; outBottom: [number, number]; outRight: [number, number] } };

/** A box touches the page (shares some area with it): an object wholly on the pasteboard takes no part in the Smart Guides (measured). */
export const onPage = (b: Box, size: [number, number] | undefined): boolean => !size || (b[2] > 0 && b[0] < size[0] && b[3] > 0 && b[1] < size[1]);

type SmartItem = { id: string; visible: boolean; bounds: number[]; kind?: string; transform?: number[]; inner?: number[] };
// an upright object and a group are measured by their bounding box (measured: the group's box, never its members)
const localOf = (it: SmartItem, dx: number, dy: number): SmartBox['local'] => (it.kind === 'group' ? undefined : localFrame(it.transform, it.inner, dx, dy));

/** The own width and height (page points) of a turned frame `r` (inner coordinates) under `m` (inner to page), with the two sides its size brackets
 *  hang off; undefined for an upright one. */
export function localFrame(m: number[] | undefined, r: number[] | undefined, dx = 0, dy = 0): SmartBox['local'] {
  if (!m || !r || (Math.abs(m[1]) < 1e-9 && Math.abs(m[2]) < 1e-9)) return undefined;
  const at = (x: number, y: number): [number, number] => [m[0] * x + m[2] * y + m[4] + dx, m[1] * x + m[3] * y + m[5] + dy];
  const bl = at(r[0], r[3]), br = at(r[2], r[3]), tr = at(r[2], r[1]);
  const unit = (x: number, y: number): [number, number] => { const n = Math.hypot(x, y) || 1; return [x / n, y / n]; };
  return { w: Math.hypot(br[0] - bl[0], br[1] - bl[1]), h: Math.hypot(br[0] - tr[0], br[1] - tr[1]), bottom: [...bl, ...br], right: [...tr, ...br],
    outBottom: unit(br[0] - tr[0], br[1] - tr[1]), outRight: unit(br[0] - bl[0], br[1] - bl[1]) };
}

/** The visible objects of a page except the dragged ones (`exclude`), as boxes. `keep` adds back the ones that stay behind (an Alt copy).
 *  `others`: the other pages of the spread with their place relative to this page (`dx`, `dy` points); their objects are targets too. */
export function smartObjects(items: PageItems | null | undefined, exclude: string[], keepOriginals = false, others: { items: PageItems | null | undefined; dx: number; dy: number }[] = []): SmartBox[] {
  const out: SmartBox[] = [];
  const seen: string[] = [];
  for (const pg of [{ items, dx: 0, dy: 0 }, ...others]) {
    if (!pg.items) continue;
    for (const it of pg.items.items as unknown as SmartItem[]) {
      if (!it.visible || seen.includes(it.id)) continue;
      if (!keepOriginals && exclude.includes(it.id)) continue;
      const own: Box = [it.bounds[0], it.bounds[1], it.bounds[2], it.bounds[3]];
      // on its own page or on this one; an object that lies wholly on the pasteboard is no target
      const b: SmartBox = [own[0] + pg.dx, own[1] + pg.dy, own[2] + pg.dx, own[3] + pg.dy];
      if (!onPage(own, pg.items.size) && !onPage(b, items?.size)) continue;
      seen.push(it.id);
      const local = localOf(it, pg.dx, pg.dy);
      if (local) b.local = local;
      out.push(b);
    }
  }
  return out;
}

export type SmartPage = { size: [number, number]; margins?: [number, number, number, number] };

// ---- alignment ----------------------------------------------------------------------------------------------------
type Ref = { v: number; center: boolean };
const refsOf = (lo: number, hi: number): Ref[] => [{ v: lo, center: false }, { v: (lo + hi) / 2, center: true }, { v: hi, center: false }];
type Tgt = { v: number; center: boolean; from: number; to: number; kind: 'object' | 'page' | 'margin' | 'edge' };

/** The lines the dragged box can be pulled onto along one axis (0 = x, 1 = y), with the extent of the object they belong to along the other axis. */
function targets(axis: 0 | 1, others: Box[], page: SmartPage | undefined): Tgt[] {
  const a = axis === 0 ? 0 : 1;
  const t: Tgt[] = [];
  for (const b of others) {
    const from = b[a === 0 ? 1 : 0], to = b[a === 0 ? 3 : 2];
    t.push({ v: b[a], center: false, from, to, kind: 'object' }, { v: (b[a] + b[a + 2]) / 2, center: true, from, to, kind: 'object' }, { v: b[a + 2], center: false, from, to, kind: 'object' });
  }
  if (page) {
    const len = page.size[axis], other = page.size[axis === 0 ? 1 : 0];
    t.push({ v: len / 2, center: true, from: 0, to: other, kind: 'page' }, { v: 0, center: false, from: 0, to: other, kind: 'edge' }, { v: len, center: false, from: 0, to: other, kind: 'edge' });
    if (page.margins) {
      const m = page.margins; // top, left, bottom, right
      const lo = axis === 0 ? m[1] : m[0], hi = axis === 0 ? m[3] : m[2];
      // the line of a margin is drawn as long as the margin box (measured: y 36.6..755.4 for 36 pt margins on a 792 pt page), not the page
      const from = axis === 0 ? m[0] : m[1], to = other - (axis === 0 ? m[2] : m[3]);
      if (lo || hi) t.push({ v: lo, center: false, from, to, kind: 'margin' }, { v: len - hi, center: false, from, to, kind: 'margin' });
    }
  }
  return t;
}

// the page edges and the margins catch the middle of the object as well as its edges (smartguides-c3 i2_e: the middle landed on the right, left and
// top page edge, on both margins and on the spine of a two-page spread; smartguides-c2 had taken a lost race against equal spacing for "never")
const allowed = (r: Ref, t: Tgt, _p: SmartPrefs): boolean => (r.center || t.center ? _p.center : _p.edges);

/** The shift along one axis that brings a reference line of `box` onto a target line, 0 when none is within `zone`. */
export function alignShift(box: Box, others: Box[], page: SmartPage | undefined, p: SmartPrefs, zone: number, axis: 0 | 1): number {
  if (!p.enabled) return 0;
  const refs = refsOf(box[axis], box[axis + 2]);
  let best = 0, bestD = Infinity, bestCenter = false;
  for (const t of targets(axis, others, page)) {
    for (const r of refs) {
      if (!allowed(r, t, p)) continue;
      const s = t.v - r.v, d = Math.abs(s);
      if (!within(s, zone, ZONE.neg, t.kind === 'page' ? ZONE.pagePos : r.center || t.center ? ZONE.centerPos : ZONE.pos)) continue;
      // nearest wins; on a tie a pair with a middle in it (measured: the middle line, not the edge one, was drawn)
      if (d < bestD - 1e-9 || (Math.abs(d - bestD) <= 1e-9 && !bestCenter && (r.center || t.center))) { best = s; bestD = d; bestCenter = r.center || t.center; }
    }
  }
  return best;
}

/** The alignment lines for `box` where it stands (after the shift): one per aligned coordinate, from the box to the farthest aligned object. */
export function alignLines(box: Box, others: Box[], page: SmartPage | undefined, p: SmartPrefs, only: { x: number[]; y: number[] } = { x: [0, 1, 2], y: [0, 1, 2] }): SmartLine[] {
  const out: SmartLine[] = [];
  if (!p.enabled) return out;
  for (const axis of [0, 1] as const) {
    const use = axis === 0 ? only.x : only.y; // which lines of the box count: 0 start edge, 1 middle, 2 end edge
    if (!use.length) continue;
    const refs = refsOf(box[axis], box[axis + 2]).filter((_, i) => use.includes(i));
    const found: { v: number; from: number; to: number; kind: SmartLine['kind']; cross: boolean }[] = [];
    const lo = axis === 0 ? box[1] : box[0], hi = axis === 0 ? box[3] : box[2];
    for (const t of targets(axis, others, page)) {
      for (const r of refs) {
        if (!allowed(r, t, p) || Math.abs(r.v - t.v) > EPS) continue;
        if (t.kind === 'edge') continue; // the page edge is drawn by the page itself
        const kind = t.kind === 'page' ? 'page' : 'object';
        const same = found.find((f) => Math.abs(f.v - t.v) <= EPS && f.kind === kind);
        const span = [t.from, t.to];
        const cross = r.center !== t.center;
        if (same) { same.from = Math.min(same.from, span[0]); same.to = Math.max(same.to, span[1]); same.cross = same.cross || cross; }
        else found.push({ v: t.v, from: Math.min(lo, span[0]), to: Math.max(hi, span[1]), kind, cross });
      }
    }
    // edge to middle pairs were drawn alone at their coordinate (measured): drop the edge to edge lines of the other coordinates then
    const keep = found.some((f) => f.cross) ? found.filter((f) => f.cross || f.kind === 'page') : found;
    for (const f of keep) {
      out.push({ axis: axis === 0 ? 'x' : 'y', at: f.v, from: Math.min(f.from, lo), to: Math.max(f.to, hi), kind: f.kind });
    }
  }
  return out;
}

// ---- spacing ------------------------------------------------------------------------------------------------------
const aligned = (a: Box, b: Box, axis: 0 | 1): boolean => {
  // the perpendicular reference lines (start, middle, end) of the two boxes: one shared line makes them a row / column
  const o = axis === 0 ? 1 : 0;
  const ra = [a[o], (a[o] + a[o + 2]) / 2, a[o + 2]], rb = [b[o], (b[o] + b[o + 2]) / 2, b[o + 2]];
  return ra.some((x) => rb.some((y) => Math.abs(x - y) <= EPS));
};

export type SpaceHit = { shift: number; gaps: { from: number; to: number }[]; boxes: Box[] };

/** Along `axis` (0: a row, gaps are horizontal), how far the box moves so that its gap equals the gap of two objects next to it. null: none within the zone. */
export function spaceShift(box: Box, others: Box[], p: SmartPrefs, zone: number, axis: 0 | 1): SpaceHit | null {
  if (!p.enabled || !p.spacing) return null;
  const lo = axis, hi = axis + 2;
  const row = others.filter((b) => aligned(box, b, axis)).sort((a, b) => a[lo] - b[lo]);
  if (row.length < 1) return null;
  const w = box[hi] - box[lo], mid = (box[lo] + box[hi]) / 2;
  const left = row.filter((b) => (b[lo] + b[hi]) / 2 <= mid), right = row.filter((b) => (b[lo] + b[hi]) / 2 > mid);
  const found: SpaceHit[] = [];
  const take = (shift: number, neg: number, pos: number, gaps: { from: number; to: number }[], boxes: Box[]) => { if (within(shift, zone, neg, pos)) found.push({ shift, gaps, boxes }); };
  const A = left.length ? left.reduce((m, b) => (b[hi] > m[hi] ? b : m)) : null;   // the nearest object on the left
  const B = right.length ? right.reduce((m, b) => (b[lo] < m[lo] ? b : m)) : null;  // the nearest on the right
  if (A && B && B[lo] - A[hi] >= w) {
    const g = (B[lo] - A[hi] - w) / 2;
    take(A[hi] + g - box[lo], ZONE.betweenNeg, ZONE.betweenPos, [{ from: A[hi], to: A[hi] + g }, { from: A[hi] + g + w, to: B[lo] }], [A, B]);
  }
  if (A) { // beside a pair on its left: the gap between the object before it and A, repeated
    const before = left.filter((b) => b !== A && b[hi] <= A[lo] + EPS);
    const Z = before.length ? before.reduce((m, b) => (b[hi] > m[hi] ? b : m)) : null;
    if (Z) { const g = A[lo] - Z[hi]; take(A[hi] + g - box[lo], ZONE.besideNeg, ZONE.besidePos, [{ from: Z[hi], to: A[lo] }, { from: A[hi], to: A[hi] + g }], [Z, A]); }
  }
  if (B) {
    const after = right.filter((b) => b !== B && b[lo] >= B[hi] - EPS);
    const Z = after.length ? after.reduce((m, b) => (b[lo] < m[lo] ? b : m)) : null;
    if (Z) { const g = Z[lo] - B[hi]; take(B[lo] - g - w - box[lo], ZONE.besideNeg, ZONE.besidePos, [{ from: B[lo] - g, to: B[lo] }, { from: B[hi], to: Z[lo] }], [B, Z]); }
  }
  return found.reduce<SpaceHit | null>((m, h) => (!m || Math.abs(h.shift) < Math.abs(m.shift) ? h : m), null);
}

/** The arrows for a spacing hit: the baseline hangs off the lowest bottom (rightmost right) edge of the objects involved. */
export function spaceGaps(shifted: Box, hit: { gaps: { from: number; to: number }[]; boxes: Box[] }, axis: 0 | 1): SmartGap[] {
  const o = axis === 0 ? 3 : 2, lo = axis, hi = axis + 2;
  const all = [shifted, ...hit.boxes];
  const at = Math.max(...all.map((b) => b[o]));
  // the object a gap starts at (its far edge is the gap's start) and the one it ends at: the end marks hang off their bottom (right) edges
  const edge = (v: number, side: number) => all.find((b) => Math.abs(b[side] - v) <= EPS)?.[o] ?? at;
  return hit.gaps.map((g) => ({ axis: axis === 0 ? 'h' : 'v', from: g.from, to: g.to, at, ends: [edge(g.from, hi), edge(g.to, lo)] }) as SmartGap);
}

/** Beside the spacing arrows the alignment lines join the dragged box with ONE partner (measured, 15 layouts): the object of the row (column) that
 *  shares the most lines (start, middle, end) with it, the leftmost (topmost) one when several share as many. One line per shared coordinate, from
 *  the box to the partner. `axis` 0: a row (the lines are horizontal). */
export function partnerLines(box: Box, others: Box[], p: SmartPrefs, axis: 0 | 1): SmartLine[] {
  const o = axis === 0 ? 1 : 0;
  const refs = refsOf(box[o], box[o + 2]);
  let best: { b: Box; shared: number[] } | null = null;
  for (const b of others) {
    const shared: number[] = [];
    for (const t of refsOf(b[o], b[o + 2])) for (const r of refs) {
      if (Math.abs(r.v - t.v) <= EPS && (r.center || t.center ? p.center : p.edges) && !shared.some((v) => Math.abs(v - t.v) <= EPS)) shared.push(t.v);
    }
    if (shared.length && (!best || shared.length > best.shared.length || (shared.length === best.shared.length && b[axis] < best.b[axis]))) best = { b, shared };
  }
  if (!best) return [];
  const from = Math.min(box[axis], best.b[axis]), to = Math.max(box[axis + 2], best.b[axis + 2]);
  return best.shared.sort((a, b) => a - b).map((at) => ({ axis: axis === 0 ? 'y' : 'x', at, from, to, kind: 'object' }) as SmartLine);
}

// ---- the moving drag ------------------------------------------------------------------------------------------------
/** `held`: per axis, the box stands on a target of that axis where it ends (also when it did not have to move): the drag grid leaves that axis alone
 *  (measured: an object dragged along a row it is aligned with keeps its exact top, 270.09; without a target the idle axis is rounded too). */
export type SmartMove = { dx: number; dy: number; draw: SmartDraw; held?: [boolean, boolean] };

/** A reference line of `box` lies on a target line of that axis. */
function standsOn(box: Box, others: Box[], page: SmartPage | undefined, p: SmartPrefs, axis: 0 | 1): boolean {
  const refs = refsOf(box[axis], box[axis + 2]);
  return targets(axis, others, page).some((t) => refs.some((r) => allowed(r, t, p) && Math.abs(r.v - t.v) <= EPS));
}

/** How far the dragged box moves to land on the other objects and what is drawn there. `base` is the shift the guides and the grid already
 *  asked for ([0 or the shift, per axis]); the nearer of the two wins per axis. Shift or no preference: nothing. */
export function smartMove(box: Box, others: Box[], page: SmartPage | undefined, p: SmartPrefs, zone: number, base: [number, number] = [0, 0]): SmartMove {
  const draw = noDraw();
  // an object that lies wholly on the pasteboard is not pulled onto anything (measured: neither onto another pasteboard object nor the page's middle)
  if (!p.enabled || !onPage(box, page?.size)) return { dx: base[0], dy: base[1], draw };
  const pick = (a: number, b: number) => (a !== 0 && (b === 0 || Math.abs(a) <= Math.abs(b)) ? a : b);
  const ax = alignShift(box, others, page, p, zone, 0), ay = alignShift(box, others, page, p, zone, 1);
  let dx = pick(ax, base[0]), dy = pick(ay, base[1]);
  // spacing: the other axis has to be aligned first (the objects share a line), then the shift along the row (column) is tried. An equal-spacing
  // place within its zone wins over an alignment, a nearer one too (measured: the middle 2.3 px from the page edge, the spacing place 3 px away:
  // spacing; out of the spacing zone: the edge). Against the guides' and the grid's own pull (base) the nearer wins, spacing on a tie.
  const at = (sx: number, sy: number): Box => [box[0] + sx, box[1] + sy, box[2] + sx, box[3] + sy];
  let hGap = spaceShift(at(0, dy), others, p, zone, 0), vGap = spaceShift(at(dx, 0), others, p, zone, 1);
  if (hGap && (dx === 0 || dx === ax || Math.abs(hGap.shift) <= Math.abs(dx))) dx = hGap.shift; else hGap = null;
  if (vGap && (dy === 0 || dy === ay || Math.abs(vGap.shift) <= Math.abs(dy))) dy = vGap.shift; else vGap = null;
  const fin = at(dx, dy);
  draw.lines = alignLines(fin, others, page, p);
  // beside the arrows the lines of the row (column) join the box with one partner only (partnerLines)
  if (hGap) {
    draw.gaps.push(...spaceGaps(fin, hGap, 0));
    let row = partnerLines(fin, others.filter((b) => aligned(fin, b, 0)), p, 0);
    // a dragged object that hangs over the page edge: only the line along the bottom edge was drawn (smartguides-c2 s2 and c3 i2_e, the same layout)
    if (page && row.length > 1 && (fin[0] < 0 || fin[2] > page.size[0] || fin[1] < 0 || fin[3] > page.size[1])) row = [row[row.length - 1]];
    draw.lines = [...draw.lines.filter((l) => l.axis === 'x' || l.kind === 'page'), ...row];
  }
  if (vGap) {
    draw.gaps.push(...spaceGaps(fin, vGap, 1));
    draw.lines = [...draw.lines.filter((l) => l.axis === 'y' || l.kind === 'page'), ...partnerLines(fin, others.filter((b) => aligned(fin, b, 1)), p, 1)];
  }
  return { dx, dy, draw, held: [!!hGap || standsOn(fin, others, page, p, 0), !!vGap || standsOn(fin, others, page, p, 1)] };
}

// ---- the handle drag (resize) -------------------------------------------------------------------------------------------
/** A target's width (axis 0) or height: a turned object's along its own sides (measured: a frame turned 90 or 45 degrees matched with the width it
 *  has unturned, not with its bounding box), anything else by its box (a group too). */
export const sizeOf = (b: SmartBox, axis: 0 | 1): number => (b.local ? (axis === 0 ? b.local.w : b.local.h) : b[axis + 2] - b[axis]);

/** The size along an axis that matches another object's width/height within the zone: [target box, shift of the size], or null. */
export function sizeMatch(size: number, others: SmartBox[], axis: 0 | 1, zone: number, p: SmartPrefs): { target: Box; delta: number } | null {
  if (!p.enabled || !p.dimensions) return null;
  let best: { target: Box; delta: number } | null = null;
  for (const b of others) {
    const s = sizeOf(b, axis), d = s - size;
    if (s > 0 && within(d, zone, ZONE.sizeNeg, ZONE.sizePos) && (!best || Math.abs(d) < Math.abs(best.delta))) best = { target: b, delta: d };
  }
  return best;
}

/** What is drawn for a resized box: the alignment lines of its edges and, where a width/height equals another object's, the brackets. */
export function resizeDraw(box: Box, others: SmartBox[], page: SmartPage | undefined, p: SmartPrefs, moving: { x: number[]; y: number[] }, own?: SmartBox['local']): SmartDraw {
  const draw = noDraw();
  if (!p.enabled) return draw;
  // a turned object (`own`) is compared by its own width and height and gets its brackets along its own sides (measured with a frame turned 90 degrees)
  const w = own ? own.w : box[2] - box[0], h = own ? own.h : box[3] - box[1];
  const me: SmartBox = [box[0], box[1], box[2], box[3]];
  if (own) me.local = own;
  if (p.dimensions) {
    // the resized object's own bracket once, and one for every object of the same width (height)
    const same = (axis: 0 | 1, v: number) => others.filter((b) => sizeOf(b, axis) > 0 && Math.abs(sizeOf(b, axis) - v) <= EPS);
    // a turned target's bracket runs along its own bottom (right) side, turned with it
    const bracket = (b: SmartBox, axis: 'w' | 'h'): SmartSize => (b.local
      ? { axis, from: 0, to: 0, at: 0, seg: axis === 'w' ? b.local.bottom : b.local.right, out: axis === 'w' ? b.local.outBottom : b.local.outRight }
      : axis === 'w' ? { axis, from: b[0], to: b[2], at: b[3] } : { axis, from: b[1], to: b[3], at: b[2] });
    // only the dimension that is being changed (the handle moves that side)
    const sw = moving.x.length ? same(0, w) : [], sh = moving.y.length ? same(1, h) : [];
    if (sw.length) draw.sizes.push(bracket(me, 'w'), ...sw.map((b) => bracket(b, 'w')));
    if (sh.length) draw.sizes.push(bracket(me, 'h'), ...sh.map((b) => bracket(b, 'h')));
  }
  // the edges of a turned object are no straight lines of the page: no alignment lines for it (not measured)
  draw.lines = own ? [] : alignLines(box, others, page, { ...p, center: false }, moving);
  return draw;
}
