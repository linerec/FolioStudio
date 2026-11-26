//! Tracks selected items and handles selection clicks and drags.

import { t as tr } from '../i18n';
import { useMemo } from 'react';
import { type EditCommand } from '../engine';
import { type Box, type DocumentInfo, type Matrix, apply, invert, pageOf, physicalMargins, unionBox } from '../inspect';
import { commandKey } from './keys';
import { type Ghost, type Hit, type SelEntry, findItem, handleAt, hitTest, hitTestAll, itemsInRect, linkBadgeAt, portAt, rotateAt } from './overlay';
import type { ToolId } from './tools';
import { NO_GRID, type Grid } from './drawgrid';
import { ownRect } from './turned';
import type { useGuides } from './guides';
import type { usePathTools } from './pathtools';
import { flatMatrix, handleBy, inDesignZoom, snapCorner } from './dragunit';
import type { useLiveCorners } from '../features/view/useLiveCorners';
import type { useAnchorControl } from '../features/anchored/useAnchorControl';
import type { useTextPathBrackets } from '../features/tools/useTextPathBrackets';
import { SNAP_ZONE_PX, type Targets, snapBox, snapPoint } from './snap';
import { ROTATE_ZONE_PX, handleRect, turnAngle } from './handledrag';
import { type SmartDraw, type SmartPrefs, localFrame, resizeDraw, sizeMatch, smartMove, smartObjects } from './smartguides';
import type { DocWindowProps } from './docview';
import type { PageCoordinates } from './docview-coordinates';

export type SelectionDrag =
  | { kind: 'marquee'; page: number; x: number; y: number; moved: boolean; add: boolean }
  /** Dragging placed content in its frame (direct selection, content grabber). */
  | { kind: 'content'; page: number; x: number; y: number; id: string; moved: boolean; by: [number, number]; corner?: [number, number] | null }
  /** Dragging the selection (selection tool): client start, the ids moved. */
  | { kind: 'move'; x: number; y: number; start: {page:number; x:number; y:number}; ids: string[]; pages: number[]; moved: boolean; by: [number, number]; snap?: { t: Targets | null; box: Box; others: Box[]; size: [number, number]; margins: [number, number, number, number] } | null; corner?: [number, number] | null;
      /** Alt-dragging: the grid of copies the arrow keys asked for (ui/drawgrid.ts copyGridOffsets), `alt` whether Alt is held now. */
      grid?: Grid; alt?: boolean }
  /** Dragging a handle: the item's transform (inner → page) and box, which sides move. */
  | { kind: 'resize'; page: number; id: string; ids: string[]; fx: number; fy: number; m: Matrix; inv: Matrix; inner: Box; rect: Box | null; snap?: Targets | null; smart?: { others: Box[]; size: [number, number]; margins: [number, number, number, number] } | null;
      /** Where the pointer went down, where the handle is, and how far the pointer has gone since (page points); the modifier keys now. */
      from: [number, number]; at: [number, number]; by: [number, number]; keep: boolean; center: boolean }
  /** Turning the selection from just outside a handle: the reference point the turn is about, the angle so far. */
  | { kind: 'rotate'; page: number; ids: string[]; from: [number, number]; center: [number, number]; by: number };

type Pointer = React.PointerEvent<HTMLDivElement>;
type Drag<K extends SelectionDrag['kind']> = Extract<SelectionDrag, { kind: K }>;

export function usePageSelection(selection: DocWindowProps['selection']) {
  return useMemo(() => {
    const m = new Map<number, SelEntry[]>();
    for (const s of selection) m.set(s.page, [...(m.get(s.page) ?? []), s]);
    return m;
  }, [selection]);
}

export const entryFor = (h: Hit, direct: boolean): SelEntry => {
  if (direct) return { page: h.page, id: h.leaf.id, content: !!h.leaf.content };
  return { page: h.page, id: h.top.id, content: false };
};

/** Pointer capture, ignored for synthetic events (tests) that have no live pointer. */
export function capture(el: HTMLElement, id: number) {
  try { el.setPointerCapture(id); } catch { /* no such pointer */ }
}

export type SelectionText = {
  mixedSpreads: () => string;
  duplicate: () => string;
  moveToSpread: () => string;
  move: () => string;
  moveContent: () => string;
  resize: () => string;
};

type SelectionInput = {
  labels: SelectionText;
  p: DocWindowProps;
  scale: number;
  setDrag: (next: SelectionDrag) => void;
  geometry: PageCoordinates;
  guides: Pick<ReturnType<typeof useGuides>, 'deselect' | 'hit' | 'pick' | 'selectIn'>;
  live: Pick<ReturnType<typeof useLiveCorners>, 'down'>;
  anchorControl: Pick<ReturnType<typeof useAnchorControl>, 'down'>;
  tpBrackets: Pick<ReturnType<typeof useTextPathBrackets>, 'down'>;
  paths: Pick<ReturnType<typeof usePathTools>, 'selectInRect'>;
  sides: ReadonlyMap<number, DocumentInfo['pages'][number]['side']>;
  smart: SmartPrefs;
  smartOn: boolean;
  turnedPage: (page: number) => boolean;
  spreadMates: (page: number) => NonNullable<Parameters<typeof smartObjects>[3]>;
  pxPerPt: (el: HTMLElement, r: DOMRect) => number;
  snapOn: (page: number) => Targets | null;
  showMove: (d: Drag<'move'>) => void;
  moveCopies: (d: Drag<'move'>) => [number, number][];
  setGhost: (ghost: { pages: number[]; g: Ghost } | null) => void;
  setSmartDraw: (draw: { page: number; draw: SmartDraw } | null) => void;
  setMarquee: (marquee: { page: number; rect: [number, number, number, number] } | null) => void;
};

export function selectionInput({ labels, p, scale, setDrag, geometry, guides, live, anchorControl, tpBrackets, paths, sides,
  smart, smartOn, turnedPage, spreadMates, pxPerPt, snapOn, showMove, moveCopies, setGhost, setSmartDraw, setMarquee }: SelectionInput) {
  const { doc } = p;
  const { ownAt, hitAt, moveOn, pointOn, turnOf, spreadDrop } = geometry;

  /** Which lines of the resized box move with a handle at `f` (0 start edge, 0.5 middle, 1 end edge): the edge itself, both edges with Alt. */
  const movingEdges = (f: number, center: boolean): number[] => (f === 0.5 ? [] : center ? [0, 2] : [f < 0.5 ? 0 : 2]);

  /** A frame turned by half a turn stands upright again: its box is its own frame (the brackets under and right of it, no edge lines). */
  const uprightFrame = (b: Box) => ({ w: b[2] - b[0], h: b[3] - b[1], bottom: [b[0], b[3], b[2], b[3]] as [number, number, number, number], right: [b[2], b[1], b[2], b[3]] as [number, number, number, number],
    outBottom: [0, 1] as [number, number], outRight: [1, 0] as [number, number] });

  /** A handle drag's pointer offset with the dragged edge put on InDesign's drag grid. With Shift or Alt the edge the pointer sets is
   *  rounded and the other side is worked out from it afterwards (measured: br Shift/Alt, r Alt: the pointer's edge is on the grid, the rest follows from it). */
  const onGrid = (d: { page: number; fx: number; fy: number; at: [number, number]; from: [number, number]; m: Matrix; keep: boolean; center: boolean }, dx: number, dy: number, gx: boolean, gy: boolean): [number, number] =>
    turnedPage(d.page) ? [dx, dy] : handleBy(d, dx, dy, gx, gy, inDesignZoom(scale));

  function down(e: Pointer, here: ReturnType<PageCoordinates['pageAt']>, t: ToolId,
    st: 'selection' | 'direct', view: HTMLDivElement) {
    // the selection tools read the page in its own space (`own`; a spread turned by View ▸ Rotate Spread is shown turned)
    const own = ownAt(e.clientX, e.clientY);
    // the yellow square of a selected frame turns its diamonds on; a diamond changes the corners (before the handles: they stand on the corner handles)
    if (live.down(e, here)) { capture(view, e.pointerId); return; }
    if (anchorControl.down(e, here)) { capture(view, e.pointerId); return; }
    if (tpBrackets.down(e, here)) { capture(view, e.pointerId); return; }
    // a handle of the selected frame: resize; a text port: load its text for threading
    if (p.editable && own && st === 'selection') {
      const items = p.store.peek(own.index);
      const port = portAt(items, p.selection, own.index, [own.x, own.y], scale);
      if (port) { e.preventDefault(); p.threading.load(port.item.id, port.port); return; }
      const hd = handleAt(items, p.selection, own.index, [own.x, own.y], 5 / scale);
      const inv = hd && invert(hd.transform);
      if (hd && inv) {
        const at = apply(hd.transform, hd.box[0] + (hd.box[2] - hd.box[0]) * hd.fx, hd.box[1] + (hd.box[3] - hd.box[1]) * hd.fy);
        setDrag({ kind: 'resize', page: own.index, id: hd.id, ids: hd.ids, fx: hd.fx, fy: hd.fy, m: hd.transform, inv, inner: hd.box, rect: null,
          from: [own.x, own.y], at, by: [0, 0], keep: false, center: false });
        capture(view, e.pointerId);
        return;
      }
      // just outside a corner handle: turn the selection about the middle of its box (InDesign 2025 ignores the reference point here)
      const turn = rotateAt(items, p.selection, own.index, [own.x, own.y], ROTATE_ZONE_PX / scale);
      const bounds = turn && items && turn.ids.map((id) => findItem(items, id)?.item.bounds);
      const box = bounds && bounds.every(Boolean) ? unionBox(bounds as Box[]) : null;
      if (turn && box) {
        const [fx, fy] = [0.5, 0.5];
        e.preventDefault();
        setDrag({ kind: 'rotate', page: own.index, ids: turn.ids, from: [own.x, own.y], center: [box[0] + (box[2] - box[0]) * fx, box[1] + (box[3] - box[1]) * fy], by: 0 });
        capture(view, e.pointerId);
        return;
      }
    }
    // Ctrl+Shift+click on a parent item: it is overridden onto the page and the copy selected
    if (p.editable && here && own && commandKey(e) && e.shiftKey) {
      const items = p.store.peek(here.index);
      const m = items && hitTest(items, here.index, [own.x, own.y], 4 / scale, p.layers, true);
      if (m) {
        e.preventDefault();
        const page = p.info?.pages.find((pg) => pg.index === here.index)?.id;
        if (!m.top.allow_overrides) p.onNote(tr('이 상위 항목은 재정의가 허용되지 않습니다.'));
        else if (page) {
          void p.onEdit(tr('상위 항목 재정의'), [{ op: 'overrideParentItems', page, ids: [m.top.id] }]).then((r) => {
            const id = r?.created?.[0];
            if (id) { p.setSelection([{ page: here.index, id, content: false }]); p.setSelectedPage(null); }
          });
        }
        return;
      }
    }
    // Ctrl+click: the object behind the selected one under the pointer; again: the next one behind, past the backmost the front one again (InDesign)
    if (own && commandKey(e) && !e.shiftKey && !e.altKey) {
      const items = p.store.peek(own.index);
      const hits = items ? hitTestAll(items, own.index, [own.x, own.y], 4 / scale, p.layers) : [];
      if (hits.length) {
        e.preventDefault();
        const at = hits.findIndex((h) => p.selection.some((s) => s.id === (t === 'direct' ? h.leaf.id : h.top.id)));
        const pick = hits[at < 0 ? 0 : (at + 1) % hits.length];
        guides.deselect();
        p.setSelection([entryFor(pick, t === 'direct')]); p.setSelectedPage(null);
        return;
      }
    }
    // a link badge opens its link in the Links panel (it sits inside the frame, so it is asked before the frame)
    if (p.linkBadges && p.onLinkBadge && own) {
      const badge = linkBadgeAt(p.store.peek(own.index), p.linkBadges, own.index, [own.x, own.y], scale);
      if (badge) { p.onLinkBadge(badge.owner); return; }
    }
    const h = hitAt(e.clientX, e.clientY, false, 4, true);
    if (h) {
      guides.deselect();
      const already = p.selection.find((s) => s.id === (st === 'direct' ? h.leaf.id : h.top.id));
      // the content grabber of a selected frame selects its graphic
      if (p.contentGrabber !== false && p.showContentGrabber !== false && st === 'selection' && already && h.top.content && !e.shiftKey) {
        const b = h.top.bounds;
        if (own) {
          const cx = (b[0] + b[2]) / 2, cy = (b[1] + b[3]) / 2;
          if (Math.hypot(own.x - cx, own.y - cy) * scale <= 10) { p.setSelection([{ page: h.page, id: h.top.id, content: true }]); return; }
        }
      }
      const entry = entryFor(h, st === 'direct');
      let next = p.selection;
      if (e.shiftKey) {
        next = already ? p.selection.filter((s) => s.id !== entry.id) : [...p.selection, entry];
        p.setSelection(next);
      } else if (!already) {
        next = [entry];
        p.setSelection(next);
      } else if (already.page !== entry.page) {
        // an undo (or a move to another spread) left the selection naming the page the object was on: it is the page it is on now
        next = p.selection.map((s) => (s === already ? entry : s));
        p.setSelection(next);
      }
      p.setSelectedPage(null);
      // drag placed content in its frame
      if (p.editable && !e.shiftKey && next.length === 1 && next[0].content) {
        setDrag({ kind: 'content', page: next[0].page, x: e.clientX, y: e.clientY, id: next[0].id, moved: false, by: [0, 0] });
        capture(view, e.pointerId);
        return;
      }
      // drag the selection to move it (Alt at the drop: a copy moves)
      // (on a turned spread the drag is read as it is shown: the objects go the way the pointer goes, moveOn)
      if (p.editable && own && !e.shiftKey && next.length && next.every((s) => !s.content)) {
        const gutter = p.store.peek(own.index)?.columns.gutter ?? NO_GRID.gap[0];
        setDrag({ kind: 'move', x: e.clientX, y: e.clientY, start:{page:own.index,x:own.x,y:own.y}, ids: next.map((s) => s.id), pages: [...new Set(next.map((s) => s.page))], moved: false, by: [0, 0], grid: { ...NO_GRID, gap: [gutter, gutter] } });
        capture(view, e.pointerId);
      }
      return;
    }
    const gh = st === 'selection' ? guides.hit(e.clientX, e.clientY) : null;
    if (gh) { guides.pick(e, gh); return; }
    if (!e.shiftKey) guides.deselect();
    // Direct Selection keeps the shapes selected until the marquee is let go: it may take their points
    if (!e.shiftKey && st !== 'direct') p.setSelection([]);
    if (here) {
      setDrag({ kind: 'marquee', page: here.index, x: here.x, y: here.y, moved: false, add: e.shiftKey });
      capture(view, e.pointerId);
    }
  }

  function moveContent(e: Pointer, d: Drag<'content'>) {
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 3) return;
    d.moved = true;
    let by: [number, number] = moveOn(d.page, e.clientX - d.x, e.clientY - d.y);
    // the picture's top-left corner lands on InDesign's drag grid, rounded down like a moved frame's (draground-c1, rules/draground-c1.ko.md: both axes, 42 of 42
    // left edges and 36 of 42 top edges on the grid); not for a turned frame or picture or a turned spread (not measured)
    if (d.corner === undefined) {
      const found = findItem(p.store.peek(d.page), d.id)?.item, c = found?.content;
      d.corner = found && c && flatMatrix(found.transform) && flatMatrix(c.transform) ? [c.bounds[0], c.bounds[1]] : null;
    }
    if (d.corner && !turnedPage(d.page)) {
      const z = inDesignZoom(scale), c = d.corner;
      by = [snapCorner(c[0] + by[0], z) - c[0], snapCorner(c[1] + by[1], z) - c[1]];
    }
    d.by = by;
  }

  function moveItems(e: Pointer, d: Drag<'move'>) {
    let [dx, dy] = moveOn(d.pages[0], e.clientX - d.x, e.clientY - d.y);
    if (!d.moved && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 3) return;
    d.moved = true;
    let pulled: [boolean, boolean] = [false, false];
    if (e.shiftKey) {
      // Shift: in steps of 45°, as InDesign (and no Smart Guides: measured, nothing is pulled and no line is drawn then)
      setSmartDraw(null);
      const a = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
      const len = Math.hypot(dx, dy) * Math.cos(Math.atan2(dy, dx) - a);
      dx = Math.cos(a) * len;
      dy = Math.sin(a) * len;
    } else if (d.pages.length === 1) {
      // the selection's box, from where it was picked up, is pulled onto guides and gridlines and (Smart Guides) onto the other objects
      if (d.snap === undefined) {
        const items = p.store.peek(d.pages[0]);
        const t0 = snapOn(d.pages[0]);
        const found = items && d.ids.map((id) => findItem(items, id));
        const box = found && found.every((f) => f) ? unionBox(found.map((f) => f!.item.bounds)) : null;
        // Alt drags a copy: the original stays and is a target like the other objects
        d.snap = items && box && (t0 || smartOn) ? { t: t0, box, others: smartOn ? smartObjects(items, d.ids, e.altKey, spreadMates(d.pages[0])) : [], size: items.size, margins: physicalMargins(items.margins, items.side ?? sides.get(d.pages[0])) } : null;
      }
      if (d.snap) {
        const b = d.snap.box, zone = (p.info?.view?.snapZone ?? SNAP_ZONE_PX) / scale;
        const moved: Box = [b[0] + dx, b[1] + dy, b[2] + dx, b[3] + dy];
        const base = d.snap.t ? snapBox(moved, d.snap.t, zone) : [0, 0] as [number, number];
        if (smartOn) {
          const r = smartMove(moved, d.snap.others, { size: d.snap.size, margins: d.snap.margins }, smart, zone, base);
          // an axis that stands on a Smart Guide target is left alone by the drag grid, also when it did not have to move
          pulled = [r.dx !== 0 || !!r.held?.[0], r.dy !== 0 || !!r.held?.[1]];
          dx += r.dx; dy += r.dy;
          setSmartDraw({ page: d.pages[0], draw: r.draw });
        } else {
          pulled = [base[0] !== 0, base[1] !== 0];
          dx += base[0]; dy += base[1];
        }
      }
      // nothing pulled the box onto a guide, a gridline or another object: its top-left corner lands on InDesign's drag grid (docdrag-c1, rules/docdrag-c1.ko.md)
      if (d.corner === undefined) {
        const items = p.store.peek(d.pages[0]);
        const found = items && d.ids.map((id) => findItem(items, id));
        const box = found && found.every((f) => f) ? unionBox(found.map((f) => f!.item.bounds)) : null;
        d.corner = box ? [box[0], box[1]] : null;
      }
      if (d.corner && !turnedPage(d.pages[0])) {
        const z = inDesignZoom(scale), c = d.corner;
        if (!pulled[0]) dx = snapCorner(c[0] + dx, z) - c[0];
        if (!pulled[1]) dy = snapCorner(c[1] + dy, z) - c[1];
      }
    }
    d.by = [dx, dy];
    d.alt = e.altKey;
    // the selection is on the pages of one spread; its neighbours draw what reaches them (and the grid of copies, Alt with the arrows)
    showMove(d);
  }

  function resize(e: Pointer, d: Drag<'resize'>) {
    const pt = pointOn(d.page, e.clientX, e.clientY);
    if (!pt) return;
    let dx = pt[0] - d.from[0], dy = pt[1] - d.from[1];
    d.keep = e.shiftKey; d.center = e.altKey;
    // Shift keeps the proportions, so the handle is not pulled onto a guide then (as the 45 degree limit of a move)
    const flatItem = !d.m[1] && !d.m[2] && d.m[0] > 0 && d.m[3] > 0;
    if (d.smart === undefined) {
      // a turned object too: its own width and height are compared (smartguides-c3); a flipped or scaled-negative one is left alone (not measured)
      const items = smartOn && (flatItem || d.m[0] * d.m[3] - d.m[1] * d.m[2] > 0) ? p.store.peek(d.page) : null;
      d.smart = items ? { others: smartObjects(items, d.ids, false, spreadMates(d.page)), size: items.size, margins: physicalMargins(items.margins, items.side ?? sides.get(d.page)) } : null;
    }
    if (d.snap === undefined) {
      // the other objects' edges are targets for the handle too (Align to Object Edges)
      const t0 = snapOn(d.page), ex = d.smart && flatItem && smart.edges ? d.smart.others : [];
      d.snap = ex.length ? { x: [...(t0?.x ?? []), ...ex.flatMap((b) => [b[0], b[2]])], y: [...(t0?.y ?? []), ...ex.flatMap((b) => [b[1], b[3]])], gridX: t0?.gridX ?? null, gridY: t0?.gridY ?? null } : t0;
    }
    const zone = (p.info?.view?.snapZone ?? SNAP_ZONE_PX) / scale;
    let sx = 0, sy = 0;
    if (d.snap && !d.keep) {
      [sx, sy] = snapPoint([d.at[0] + dx, d.at[1] + dy], d.snap, zone, { x: d.fx !== 0.5, y: d.fy !== 0.5 });
      dx += sx;
      dy += sy;
    }
    // Smart Dimensions: a width or height equal to another object's pulls the handle onto it (not with Shift, which keeps the proportions)
    let wx = 0, wy = 0;
    if (d.smart && !d.keep) {
      const pageRect = (rect: Box): Box => { const a = apply(d.m, rect[0], rect[1]), b = apply(d.m, rect[2], rect[3]); return [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])]; };
      const rectAt = (ex: number, ey: number) => handleRect(d.inner, [d.fx, d.fy], [d.inv[0] * ex + d.inv[2] * ey, d.inv[1] * ex + d.inv[3] * ey], d.keep, d.center);
      const k = d.center ? 2 : 1, sideX = d.fx < 0.25 ? -1 : d.fx > 0.75 ? 1 : 0, sideY = d.fy < 0.25 ? -1 : d.fy > 0.75 ? 1 : 0;
      if (flatItem) {
        const r0 = pageRect(rectAt(dx, dy));
        const mx = sideX && !sx ? sizeMatch(r0[2] - r0[0], d.smart.others, 0, zone, smart) : null;
        const my = sideY && !sy ? sizeMatch(r0[3] - r0[1], d.smart.others, 1, zone, smart) : null;
        if (mx) { wx = mx.delta / (sideX * k); dx += wx; }
        if (my) { wy = my.delta / (sideY * k); dy += wy; }
      } else {
        // a turned object: its own width (along its x axis) and height are what is compared; the handle moves along that axis
        const r0 = rectAt(dx, dy), kx = Math.hypot(d.m[0], d.m[1]), ky = Math.hypot(d.m[2], d.m[3]);
        const mx = sideX ? sizeMatch((r0[2] - r0[0]) * kx, d.smart.others, 0, zone, smart) : null;
        const my = sideY ? sizeMatch((r0[3] - r0[1]) * ky, d.smart.others, 1, zone, smart) : null;
        if (mx) { const q = mx.delta / (kx * kx * sideX * k); dx += d.m[0] * q; dy += d.m[1] * q; wx = wy = 1; }
        if (my) { const q = my.delta / (ky * ky * sideY * k); dx += d.m[2] * q; dy += d.m[3] * q; wx = wy = 1; }
      }
    }
    // no guide caught the handle: it lands on InDesign's drag grid for this zoom (rules/tabledrag-c1.ko.md), for an item that is not turned
    [dx, dy] = onGrid(d, dx, dy, !sx && !wx, !sy && !wy);
    d.by = [dx, dy];
    // the preview; the engine (resizeByHandle) works out the same box when the pointer is let go
    const own: [number, number] = [d.inv[0] * dx + d.inv[2] * dy, d.inv[1] * dx + d.inv[3] * dy];
    d.rect = handleRect(d.inner, [d.fx, d.fy], own, d.keep, d.center);
    if (d.smart && !d.keep) {
      const a = apply(d.m, d.rect[0], d.rect[1]), b = apply(d.m, d.rect[2], d.rect[3]);
      const pageBox: Box = [Math.min(a[0], b[0]), Math.min(a[1], b[1]), Math.max(a[0], b[0]), Math.max(a[1], b[1])];
      setSmartDraw({ page: d.page, draw: resizeDraw(pageBox, d.smart.others, { size: d.smart.size, margins: d.smart.margins }, smart, { x: movingEdges(d.fx, d.center), y: movingEdges(d.fy, d.center) }, flatItem ? undefined : localFrame(d.m, d.rect) ?? uprightFrame(pageBox)) });
    } else setSmartDraw(null);
    setGhost({ pages: [d.page], g: { kind: 'box', id: d.id, transform: d.m, rect: d.rect } });
  }

  function rotate(e: Pointer, d: Drag<'rotate'>) {
    const pt = pointOn(d.page, e.clientX, e.clientY);
    if (!pt) return;
    const a = turnAngle(d.center, d.from, pt, e.shiftKey);
    d.by = a ?? 0;
    const rad = d.by * Math.PI / 180, c = Math.cos(rad), s = Math.sin(rad), [cx, cy] = d.center;
    setGhost({ pages: [d.page], g: { kind: 'matrix', ids: d.ids, matrix: [c, -s, s, c, cx * (1 - c) - cy * s, cy * (1 - c) + cx * s] } });
  }

  function marquee(e: Pointer, d: Drag<'marquee'>, view: HTMLDivElement) {
    const el = view.querySelector<HTMLElement>(`[data-page="${d.page}"]`);
    if (!el) return;
    const r = el.getBoundingClientRect();
    const k = pxPerPt(el, r);
    const x = (e.clientX - r.left) / k, y = (e.clientY - r.top) / k;
    if (Math.hypot((x - d.x) * scale, (y - d.y) * scale) > 3) d.moved = true;
    // drawn in the page, so in its own space (a turned spread turns it with the page)
    const pg = doc.pages.find((q) => q.index === d.page);
    if (d.moved) setMarquee({ page: d.page, rect: pg && turnOf(d.page) ? ownRect(turnOf(d.page), pg.width, pg.height, [Math.min(d.x, x), Math.min(d.y, y), Math.max(d.x, x), Math.max(d.y, y)]) : [d.x, d.y, x, y] });
  }

  function dropItems(e: Pointer, d: Drag<'move'>) {
    setGhost(null); setSmartDraw(null);
    if (d.moved && (d.by[0] || d.by[1])) {
      const dest = spreadDrop(d, e.clientX, e.clientY);
      if (dest === 'mixed') { p.onNote(labels.mixedSpreads()); return; }
      if (dest) {
        // let go over another spread: its pages or its pasteboard. Alt drags a copy there (one step: the copy, then its move); the original stays
        const commands: EditCommand[] = e.altKey
          ? [{ op: 'duplicate', ids: d.ids, by: [0, 0] }, { op: 'moveToSpread', ids: ['@copies'], spread: dest.spread, by: dest.by }]
          : [{ op: 'moveToSpread', ids: d.ids, spread: dest.spread, by: dest.by }];
        void p.onEdit(e.altKey ? labels.duplicate() : labels.moveToSpread(), commands).then(result => {
          if (!result) return;
          const ids = e.altKey ? result.created ?? [] : d.ids;
          p.setSelection(ids.map(id => ({ page: dest.index, id, content: false })));
        });
        return;
      }
      d.alt = e.altKey;
      const copies = e.altKey ? moveCopies(d) : [];
      if (copies.length) {
        // the grid of copies: one edit (one Undo), the copies selected
        const page = d.pages[0];
        void p.onEdit(labels.duplicate(), copies.map((by): EditCommand => ({ op: 'duplicate', ids: d.ids, by }))).then((result) => {
          if (result?.created?.length) p.setSelection(result.created.map((id) => ({ page, id, content: false })));
        });
      } else if (e.altKey) p.onEdit(labels.duplicate(), [{ op: 'duplicate', ids: d.ids, by: d.by }]);
      else p.onEdit(labels.move(), [{ op: 'move', ids: d.ids, by: d.by }]);
    }
  }

  function dropContent(d: Drag<'content'>) {
    if (d.moved && (d.by[0] || d.by[1])) p.onEdit(labels.moveContent(), [{ op: 'moveContent', id: d.id, by: d.by }]);
  }

  function dropResize(d: Drag<'resize'>) {
    setGhost(null); setSmartDraw(null);
    // the drag and the keys go to the engine as they are; it works out the box (Shift: proportions, Alt: about the middle)
    if (d.rect && (d.by[0] || d.by[1])) {
      p.onEdit(labels.resize(), [{ op: 'resizeByHandle', ids: d.ids, handle: [d.fx, d.fy], by: d.by, keep_proportions: d.keep, from_center: d.center }]);
    }
  }

  function dropRotation(e: Pointer, d: Drag<'rotate'>) {
    setGhost(null); setSmartDraw(null);
    const origin = pageOf(p.info, d.page)?.spreadOrigin;
    const pt = pointOn(d.page, e.clientX, e.clientY);
    if (!origin || !pt || !d.by) return;
    const at = (x: number, y: number): [number, number] => [x + origin[0], y + origin[1]];
    p.onEdit(tr('회전'), [{ op: 'rotateByDrag', ids: d.ids, from: at(d.from[0], d.from[1]), to: at(pt[0], pt[1]), constrain: e.shiftKey, anchor: [0.5, 0.5] }]);
  }

  function dropMarquee(e: Pointer, d: Drag<'marquee'>, view: HTMLDivElement) {
    setMarquee(null);
    if (!d.moved) {
      if (p.tool === 'direct' && !d.add) p.setSelection([]);
      return;
    }
    // the rectangle in client coordinates, then each page of the spread
    const el0 = view.querySelector<HTMLElement>(`[data-page="${d.page}"]`);
    if (!el0) return;
    const r0 = el0.getBoundingClientRect();
    const k0 = pxPerPt(el0, r0);
    const cx0 = r0.left + d.x * k0, cy0 = r0.top + d.y * k0;
    const row = el0.closest('.spread');
    const own: [number, number, number, number] = [
      (Math.min(cx0, e.clientX) - r0.left) / k0, (Math.min(cy0, e.clientY) - r0.top) / k0,
      (Math.max(cx0, e.clientX) - r0.left) / k0, (Math.max(cy0, e.clientY) - r0.top) / k0,
    ];
    // the points inside: read in the page's own space (a turned spread is shown turned)
    const pg0 = doc.pages.find((q) => q.index === d.page);
    if (p.tool === 'direct' && paths.selectInRect(d.page, pg0 ? ownRect(doc.pageSettings?.rotation?.[pg0.spread], pg0.width, pg0.height, own) : own, d.add)) return;
    if (p.tool === 'direct' && !d.add) p.setSelection([]);
    const found: SelEntry[] = [];
    for (const el of row ? [...row.querySelectorAll<HTMLElement>('.page')] : [el0]) {
      const idx = Number(el.dataset.page);
      const items = p.store.peek(idx);
      if (!items) continue;
      const r = el.getBoundingClientRect();
      const rect: [number, number, number, number] = [
        (Math.min(cx0, e.clientX) - r.left) / pxPerPt(el, r), (Math.min(cy0, e.clientY) - r.top) / pxPerPt(el, r),
        (Math.max(cx0, e.clientX) - r.left) / pxPerPt(el, r), (Math.max(cy0, e.clientY) - r.top) / pxPerPt(el, r),
      ];
      const pg = doc.pages.find((q) => q.index === idx), tn = turnOf(idx);
      for (const it of itemsInRect(items, pg && tn ? ownRect(tn, pg.width, pg.height, rect) : rect, p.layers)) found.push({ page: idx, id: it.id, content: false });
    }
    // a marquee that holds no object selects the ruler guides it crosses (Folio's choice, not measured in InDesign)
    if (!found.length && p.tool === 'selection' && guides.selectIn(d.page, own, d.add)) return;
    p.setSelection(d.add ? [...p.selection, ...found.filter((f) => !p.selection.some((s) => s.id === f.id))] : found);
  }

  return { down, moveContent, moveItems, resize, rotate, marquee, dropItems, dropContent, dropResize, dropRotation, dropMarquee };
}
