//! Anchor, handle and segment editing for the point tools and Direct Selection.

//   Scissors: click a point or a segment of a shape to cut the path there (engine: cutPath, edit/src/ops/pathedit.rs).
//   Direct Selection: with shapes selected their anchor points show; click one to select it (Shift adds), drag to move the
//   selected anchors with their handles, drag a handle of a selected anchor (Alt: on its own), drag a segment to reshape it (a straight one
//   moves with its two points), Shift keeps the drag on 45° steps. A selected point shows the handles of its
//   neighbours that shape the segments next to it too (pathgeom.neighborHandles), and they can be dragged. The drag shows the new path;
//   the edit goes to the engine when the pointer is let go.
//   Convert Direction Point: a click on a point takes its handles in, a drag out of it pulls two in line, a drag of one handle breaks it.

import { type Dispatch, type RefObject, type SetStateAction, useEffect } from 'react';
import type { EditCommand } from '../engine';
import { tr } from '../i18n';
import { type ItemInfo, type Matrix, apply, invert } from '../inspect';
import { typing } from './commands';
import { findItem } from './overlay';
import { type Pt, type Side, constrain45, moveAnchors, moveHandle, nearestOnSegment, neighborHandles, reshapeSegment, svgPath } from './pathgeom';
import { pointSelection, selectedPoints } from './store';
import type { ToolId } from './tools';
import { ownDelta } from './turned';
import type { Here, Options, ShowSnap, SmartAt } from './pathtools-types';

export type PointSel = { page: number; id: string; path: number; indices: number[] };
/** A drag in progress: where it started (page), and the item's path as it would be. */
export type Drag =
  | { kind: 'anchors'; sel: PointSel; start: [number, number]; moved: boolean; draft: Pt[] }
  | { kind: 'handle'; sel: PointSel; index: number; side: Side; moved: boolean; draft: Pt[]; alt: boolean }
  /** Convert Direction Point: pressed on a point; dragged out it gets handles one each way. */
  | { kind: 'convert'; sel: PointSel; index: number; moved: boolean; draft: Pt[] }
  /** Direct Selection: a segment (from point `segment` to the next) grabbed at `t`, dragged from `start`. */
  | { kind: 'segment'; sel: PointSel; segment: number; t: number; start: [number, number]; moved: boolean; draft: Pt[] };

export const at = (m: Matrix, x: number, y: number): [number, number] => apply(m, x, y);
export const dist = (a: [number, number], b: [number, number]) => Math.hypot(a[0] - b[0], a[1] - b[1]);

export function usePointToolKeys(
  o: Options,
  live: RefObject<Options>,
  sel: PointSel | null,
  setSel: Dispatch<SetStateAction<PointSel | null>>,
) {
  // keys read this: the arrow keys and Delete are the points' while any are selected
  useEffect(() => { pointSelection.set(!!sel && sel.indices.length > 0 && o.tool === 'direct'); return () => pointSelection.set(false); }, [sel, o.tool]);
  // Object ▸ Convert Point works on them
  useEffect(() => {
    selectedPoints.set(sel && sel.indices.length > 0 && o.tool === 'direct' ? { id: sel.id, path: sel.path, indices: sel.indices } : null);
    return () => selectedPoints.set(null);
  }, [sel, o.tool]);

  // the selected points: Delete / Backspace take them away, the arrow keys move them by the cursor key increment (Shift: ten times)
  useEffect(() => {
    if (!sel || o.tool !== 'direct' || !o.editable) return;
    const key = (e: KeyboardEvent) => {
      if (typing(e) || e.ctrlKey || e.metaKey) return;
      const l = live.current;
      const item = findItem(l.store.peek(sel.page), sel.id)?.item;
      const points = item?.paths?.[sel.path]?.points;
      if (!item || !points || !sel.indices.length) return;
      if (e.key === 'Delete' || e.key === 'Backspace') {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (points.length - sel.indices.length < 2) { l.note(tr('패스에는 점이 두 개 이상 있어야 합니다.')); return; }
        // from the last to the first, so the numbers of the others hold
        const cmds: EditCommand[] = [...sel.indices].sort((a, b) => b - a).map((index) => ({ op: 'deletePathPoint', id: sel.id, path: sel.path, index }));
        setSel(null);
        void l.edit(tr('고정점 삭제'), cmds);
      } else if (e.key.startsWith('Arrow')) {
        e.preventDefault();
        e.stopImmediatePropagation();
        const step = (l.info?.view?.cursorKeyIncrement ?? 1) * (e.shiftKey ? 10 : 1);
        // on a turned spread the points go the way the arrow points on the screen
        const d = ownDelta(l.turn?.(sel.page), e.key === 'ArrowLeft' ? [-step, 0] : e.key === 'ArrowRight' ? [step, 0] : e.key === 'ArrowUp' ? [0, -step] : [0, step]);
        const inv = invert(item.transform);
        if (!inv) return;
        const [x0, y0] = at(inv, 0, 0), [x1, y1] = at(inv, d[0], d[1]);
        void l.edit(tr('점 이동'), [{ op: 'movePathPoints', id: sel.id, path: sel.path, indices: sel.indices, by: [x1 - x0, y1 - y0] }]);
      }
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [sel, o.tool, o.editable]);
}

type PointToolState = {
  live: RefObject<Options>;
  sel: PointSel | null;
  selRef: RefObject<PointSel | null>;
  setSel: Dispatch<SetStateAction<PointSel | null>>;
  drag: Drag | null;
  dragRef: RefObject<Drag | null>;
  setDrag: Dispatch<SetStateAction<Drag | null>>;
  smartAt: SmartAt;
  showSnap: ShowSnap;
};

export function createPointTools({ live, sel, selRef, setSel, drag, dragRef, setDrag, smartAt, showSnap }: PointToolState) {
  /** The shapes of the selection on a page whose points can be edited. */
  const editable = (page: number) => {
    const l = live.current;
    const items = l.store.peek(page);
    return l.selection.filter((s) => s.page === page && !s.content).flatMap((s) => {
      const item = findItem(items, s.id)?.item;
      const layer = item?.layer ? l.layers.get(item.layer) : undefined;
      return item?.paths?.length && !item.locked && !(layer && (layer.locked || !layer.visible)) ? [{ id: s.id, item }] : [];
    });
  };

  const pointAt = (page: number, p: [number, number], tol: number) => {
    // handles of the selected anchors first (and the neighbours' handles that shape the segments next to them), then any anchor
    for (const { id, item } of editable(page)) {
      const paths = item.paths ?? [];
      if (sel && sel.id === id) {
        const sp = paths[sel.path];
        const handles: { index: number; side: Side }[] = [];
        for (const k of sel.indices) for (const side of ['left', 'right'] as Side[]) handles.push({ index: k, side });
        if (sp) handles.push(...neighborHandles(sp.points, sp.open, sel.indices));
        for (const { index: k, side } of handles) {
          const pt = sp?.points[k];
          if (!pt) continue;
          const [hx, hy] = side === 'left' ? [pt[2], pt[3]] : [pt[4], pt[5]];
          if (hx === pt[0] && hy === pt[1]) continue;
          const q = at(item.transform, hx, hy);
          if (Math.hypot(q[0] - p[0], q[1] - p[1]) <= tol) return { kind: 'handle' as const, id, path: sel.path, index: k, side };
        }
      }
    }
    for (const { id, item } of editable(page)) {
      for (const [path, sp] of (item.paths ?? []).entries()) {
        for (const [index, pt] of sp.points.entries()) {
          const q = at(item.transform, pt[0], pt[1]);
          if (Math.hypot(q[0] - p[0], q[1] - p[1]) <= tol) return { kind: 'anchor' as const, id, path, index };
        }
      }
    }
    return null;
  };

  /** The shapes a point tool works on at a click: the selected ones, and the one under the pointer. */
  const candidates = (e: React.PointerEvent, page: number) => {
    const l = live.current;
    const out = editable(page).map(({ id, item }) => ({ page, id, item }));
    const under = l.hit(e.clientX, e.clientY);
    const item = under?.item;
    const layer = item?.layer ? l.layers.get(item.layer) : undefined;
    if (under && item?.paths?.length && !item.locked && !(layer && (layer.locked || !layer.visible)) && !out.some((c) => c.id === item.id)) out.push({ page: under.page, id: item.id, item });
    return out;
  };

  /** Add Anchor Point, Delete Anchor Point and Convert Direction Point: a click on a segment or a point of a shape. */
  const pointTool = (e: React.PointerEvent, here: Here, tol: number, tool: ToolId = live.current.tool): boolean => {
    const l = live.current;
    e.preventDefault();
    // Convert Direction Point on a handle of the selected point: that handle alone moves (the point is no longer smooth)
    if (tool === 'convert') {
      const h = pointAt(here.index, [here.x, here.y], tol);
      const s = selRef.current;
      if (h?.kind === 'handle' && s) {
        const points = findItem(l.store.peek(here.index), h.id)?.item?.paths?.[h.path]?.points;
        if (points) { setDrag({ kind: 'handle', sel: s, index: h.index, side: h.side, moved: false, draft: points, alt: true }); return true; }
      }
    }
    const cands = candidates(e, here.index).filter((c) => c.page === here.index);
    const p: [number, number] = [here.x, here.y];
    const choose = (id: string, page: number) => {
      if (!l.selection.some((s) => s.id === id && !s.content)) l.setSelection([{ page, id, content: false }]);
    };
    /** The segment nearest the click (not at an anchor itself: a point is already there). */
    const nearestSegment = (): { id: string; page: number; path: number; segment: number; t: number; dist: number } | null => {
      let best: { id: string; page: number; path: number; segment: number; t: number; dist: number } | null = null;
      for (const c of cands) {
        const m = c.item.transform;
        for (const [path, sp] of (c.item.paths ?? []).entries()) {
          const pts: Pt[] = sp.points.map((q) => { const a = at(m, q[0], q[1]), lf = at(m, q[2], q[3]), r = at(m, q[4], q[5]); return [a[0], a[1], lf[0], lf[1], r[0], r[1]]; });
          const count = sp.open ? pts.length - 1 : pts.length;
          for (let segment = 0; segment < count; segment++) {
            const r = nearestOnSegment(pts[segment], pts[(segment + 1) % pts.length], p);
            // not on an anchor itself: a point is already there
            if (r.t > 0.02 && r.t < 0.98 && r.dist <= tol && (!best || r.dist < best.dist)) best = { id: c.id, page: c.page, path, segment, t: r.t, dist: r.dist };
          }
        }
      }
      return best;
    };
    if (tool === 'penAdd') {
      const best = nearestSegment();
      if (best) {
        const b: { id: string; page: number; path: number; segment: number; t: number } = best;
        choose(b.id, b.page);
        void l.edit(tr('고정점 추가'), [{ op: 'addPathPoint', id: b.id, path: b.path, segment: b.segment, t: b.t }]);
      }
      return true;
    }
    // the nearest anchor
    let hit: { id: string; page: number; path: number; index: number; item: ItemInfo; dist: number } | null = null;
    for (const c of cands) {
      for (const [path, sp] of (c.item.paths ?? []).entries()) {
        for (const [index, q] of sp.points.entries()) {
          const a = at(c.item.transform, q[0], q[1]);
          const dist = Math.hypot(a[0] - p[0], a[1] - p[1]);
          if (dist <= tol && (!hit || dist < hit.dist)) hit = { id: c.id, page: c.page, path, index, item: c.item, dist };
        }
      }
    }
    if (!hit) {
      // Scissors on a segment: a point is added there and the path cut at it, in one step
      if (tool === 'scissors') {
        const s = nearestSegment();
        if (s) {
          choose(s.id, s.page);
          void l.edit(tr('패스 자르기'), [{ op: 'addPathPoint', id: s.id, path: s.path, segment: s.segment, t: s.t }, { op: 'cutPath', id: s.id, path: s.path, index: s.segment + 1 }]);
        }
      }
      return true;
    }
    const h: { id: string; page: number; path: number; index: number; item: ItemInfo } = hit;
    choose(h.id, h.page);
    if (tool === 'scissors') {
      void l.edit(tr('패스 자르기'), [{ op: 'cutPath', id: h.id, path: h.path, index: h.index }]);
      return true;
    }
    if (tool === 'penDelete') {
      if ((h.item.paths?.[h.path]?.points.length ?? 0) <= 2) l.note(tr('패스에는 점이 두 개 이상 있어야 합니다.'));
      else void l.edit(tr('고정점 삭제'), [{ op: 'deletePathPoint', id: h.id, path: h.path, index: h.index }]);
      return true;
    }
    const points = h.item.paths?.[h.path]?.points;
    if (points) setDrag({ kind: 'convert', sel: { page: h.page, id: h.id, path: h.path, indices: [h.index] }, index: h.index, moved: false, draft: points });
    return true;
  };

  /** The segment of a selected shape nearest a page point within `tol` (not at an anchor itself: a point is already there). */
  const segmentAt = (page: number, p: [number, number], tol: number) => {
    let best: { id: string; item: ItemInfo; path: number; segment: number; t: number; dist: number } | null = null;
    for (const { id, item } of editable(page)) {
      const m = item.transform;
      for (const [path, sp] of (item.paths ?? []).entries()) {
        const pts: Pt[] = sp.points.map((q) => { const a = at(m, q[0], q[1]), lf = at(m, q[2], q[3]), r = at(m, q[4], q[5]); return [a[0], a[1], lf[0], lf[1], r[0], r[1]]; });
        const count = sp.open ? pts.length - 1 : pts.length;
        for (let segment = 0; segment < count; segment++) {
          const r = nearestOnSegment(pts[segment], pts[(segment + 1) % pts.length], p);
          if (r.t > 0.02 && r.t < 0.98 && r.dist <= tol && (!best || r.dist < best.dist)) best = { id, item, path, segment, t: r.t, dist: r.dist };
        }
      }
    }
    return best;
  };

  /** Where the pointer counts as being during a drag: a dragged handle is pulled onto the snap targets itself, and of the moved anchors
   *  the one nearest where the drag started (the one grabbed) is, the others going along. */
  const pulled = (d: Drag, item: ItemInfo, points: Pt[], here: Here, shift: boolean): [number, number] => {
    const { snap, grid } = live.current;
    // Shift: the drag keeps to 45° steps (points and segments from where the drag started, a handle about its anchor), and snaps to nothing
    if (shift) {
      if (d.kind === 'anchors' || d.kind === 'segment') return constrain45(d.start, [here.x, here.y]);
      const q = points[d.index];
      return q ? constrain45(at(item.transform, q[0], q[1]), [here.x, here.y]) : [here.x, here.y];
    }
    if (d.kind === 'segment') return [here.x, here.y];
    // a dragged handle: onto an anchor of a path (not its own point; the object lines do not catch a handle), else the guides
    if (d.kind !== 'anchors') {
      const raw: [number, number] = [here.x, here.y];
      return smartAt(d.sel.page, raw, { id: d.sel.id, path: d.sel.path, indices: [d.index] }, true) ?? (snap ? snap(d.sel.page, raw) : raw);
    }
    let grab: [number, number] | null = null, best = Infinity;
    for (const k of d.sel.indices) {
      const q = points[k];
      if (!q) continue;
      const a = at(item.transform, q[0], q[1]), far = Math.hypot(a[0] - d.start[0], a[1] - d.start[1]);
      if (far < best) { best = far; grab = a; }
    }
    if (!grab) return [here.x, here.y];
    const moved: [number, number] = [grab[0] + here.x - d.start[0], grab[1] + here.y - d.start[1]];
    // the grabbed anchor onto another anchor (the moved ones are no targets) or a Smart Guide: exactly there, not on the drag grid
    const caught = smartAt(d.sel.page, moved, { id: d.sel.id, path: d.sel.path, indices: d.sel.indices });
    if (caught) return [here.x + caught[0] - moved[0], here.y + caught[1] - moved[1]];
    if (!snap && !grid) return [here.x, here.y];
    const to = snap ? snap(d.sel.page, moved) : moved;
    // an axis no guide or gridline pulled: the grabbed point goes to the grid point of the pointer itself, wherever on the point it was pressed (InDesign 2025,
    // draground-c1: 138 of 138 coordinates of a dragged line end on the grid, the press 0-2 pixels beside the point made no difference)
    const g = grid ? grid(d.sel.page, [here.x, here.y]) : null;
    const target: [number, number] = [g && to[0] === moved[0] ? g[0] : to[0], g && to[1] === moved[1] ? g[1] : to[1]];
    return [here.x + target[0] - moved[0], here.y + target[1] - moved[1]];
  };

  const down = (l: Options, e: React.PointerEvent, here: Here, tol: number): boolean => {
    const hit = pointAt(here.index, [here.x, here.y], tol);
    if (!hit) {
      // a segment of a selected shape: a drag reshapes it (a straight one moves with its two points), a click shows the path's points unselected
      const seg = segmentAt(here.index, [here.x, here.y], 4 / l.scale);
      const segPoints = seg?.item.paths?.[seg.path]?.points;
      if (!seg || !segPoints) return false;
      e.preventDefault();
      setDrag({ kind: 'segment', sel: { page: here.index, id: seg.id, path: seg.path, indices: [] }, segment: seg.segment, t: seg.t, start: [here.x, here.y], moved: false, draft: segPoints });
      return true;
    }
    e.preventDefault();
    const item = findItem(l.store.peek(here.index), hit.id)?.item;
    const points = item?.paths?.[hit.path]?.points;
    if (!item || !points) return false;
    if (hit.kind === 'handle') {
      setDrag({ kind: 'handle', sel: sel!, index: hit.index, side: hit.side, moved: false, draft: points, alt: e.altKey });
      return true;
    }
    const chosen: PointSel = sel && sel.id === hit.id && sel.path === hit.path && e.shiftKey
      ? { ...sel, indices: sel.indices.includes(hit.index) ? sel.indices.filter((k) => k !== hit.index) : [...sel.indices, hit.index] }
      : sel && sel.id === hit.id && sel.path === hit.path && sel.indices.includes(hit.index)
        ? sel
        : { page: here.index, id: hit.id, path: hit.path, indices: [hit.index] };
    setSel(chosen);
    if (chosen.indices.length) setDrag({ kind: 'anchors', sel: chosen, start: [here.x, here.y], moved: false, draft: points });
    return true;
  };

  const move = (l: Options, e: React.PointerEvent, here: Here | null): boolean => {
    const d = dragRef.current;
    if (!d || !here) return false;
    const item = findItem(l.store.peek(d.sel.page), d.sel.id)?.item;
    const points = item?.paths?.[d.sel.path]?.points;
    const inv = item && invert(item.transform);
    if (!item || !points || !inv) return true;
    if (d.kind === 'convert') {
      // dragged out of the point: the right handle follows the pointer, the left is its mirror
      const a = points[d.index], to = at(inv, ...pulled(d, item, points, here, e.shiftKey));
      const out = Math.hypot(to[0] - a[0], to[1] - a[1]);
      const moved = d.moved || out * l.scale > 3;
      setDrag({ ...d, moved, draft: moved ? points.map((q, i): Pt => (i === d.index ? [a[0], a[1], 2 * a[0] - to[0], 2 * a[1] - to[1], to[0], to[1]] : q)) : points });
    } else if (d.kind === 'anchors') {
      const [px, py] = pulled(d, item, points, here, e.shiftKey);
      const [x0, y0] = at(inv, d.start[0], d.start[1]), [x1, y1] = at(inv, px, py);
      const moved = Math.hypot(here.x - d.start[0], here.y - d.start[1]) * l.scale > 2 || d.moved;
      setDrag({ ...d, moved, draft: moved ? moveAnchors(points, d.sel.indices, [x1 - x0, y1 - y0]) : points });
    } else if (d.kind === 'segment') {
      const [px, py] = pulled(d, item, points, here, e.shiftKey);
      const [x0, y0] = at(inv, d.start[0], d.start[1]), [x1, y1] = at(inv, px, py);
      const moved = d.moved || dist([here.x, here.y], d.start) * l.scale > 2;
      const i = d.segment, j = (i + 1) % points.length;
      const r = moved && points[i] && points[j] ? reshapeSegment(points[i], points[j], d.t, [x1 - x0, y1 - y0]) : null;
      setDrag({ ...d, moved, draft: r ? points.map((q, k): Pt => (k === i ? r.a : k === j ? r.b : q)) : points });
    } else {
      const to = at(inv, ...pulled(d, item, points, here, e.shiftKey));
      // Alt (or the Convert Direction Point tool) breaks the handles: once broken in a drag, they stay apart for it
      const alt = d.alt || e.altKey;
      setDrag({ ...d, moved: true, alt, draft: moveHandle(points, d.index, d.side, to, alt) });
    }
    return true;
  };

  const up = (l: Options, e: React.PointerEvent, here: Here | null): boolean => {
    const d = dragRef.current;
    if (!d) return false;
    setDrag(null);
    showSnap(d.sel.page, null);
    // a converted point stays selected: its handles show, and one of them can be dragged on its own next
    if (d.kind === 'convert') setSel(d.sel);
    if (d.kind === 'convert' && !d.moved) {
      void l.edit(tr('방향점 전환'), [{ op: 'plainPathPoint', id: d.sel.id, path: d.sel.path, index: d.index }]);
      return true;
    }
    // a click on a segment: the path's points show, none of them selected
    if (d.kind === 'segment' && !d.moved) { setSel(null); return true; }
    if (!d.moved || !here) return true;
    const item = findItem(l.store.peek(d.sel.page), d.sel.id)?.item;
    const inv = item && invert(item.transform);
    const points = item?.paths?.[d.sel.path]?.points;
    if (!item || !inv || !points) return true;
    const [px, py] = pulled(d, item, points, here, e.shiftKey);
    showSnap(d.sel.page, null);
    if (d.kind === 'convert') {
      const q = d.draft[d.index];
      void l.edit(tr('방향점 전환'), [{ op: 'setPathHandles', id: d.sel.id, path: d.sel.path, index: d.index, left: [q[2], q[3]], right: [q[4], q[5]] }]);
    } else if (d.kind === 'anchors') {
      const [x0, y0] = at(inv, d.start[0], d.start[1]), [x1, y1] = at(inv, px, py);
      void l.edit(tr('점 이동'), [{ op: 'movePathPoints', id: d.sel.id, path: d.sel.path, indices: d.sel.indices, by: [x1 - x0, y1 - y0] }]);
    } else if (d.kind === 'segment') {
      const [x0, y0] = at(inv, d.start[0], d.start[1]), [x1, y1] = at(inv, px, py);
      const i = d.segment, j = (i + 1) % points.length, by: [number, number] = [x1 - x0, y1 - y0];
      if (!points[i] || !points[j]) return true;
      const r = reshapeSegment(points[i], points[j], d.t, by);
      const { id, path } = d.sel;
      if (r.move) void l.edit(tr('선분 이동'), [{ op: 'movePathPoints', id, path, indices: [i, j], by }]);
      else void l.edit(tr('선분 모양 바꾸기'), [
        { op: 'setPathHandles', id, path, index: i, left: [r.a[2], r.a[3]], right: [r.a[4], r.a[5]] },
        { op: 'setPathHandles', id, path, index: j, left: [r.b[2], r.b[3]], right: [r.b[4], r.b[5]] },
      ]);
    } else {
      const to = at(inv, px, py);
      void l.edit(tr('방향선 이동'), [{ op: 'movePathHandle', id: d.sel.id, path: d.sel.path, index: d.index, side: d.side, to, independent: e.altKey || d.alt }]);
    }
    return true;
  };

  const draw = (page: number, k: number, items: React.ReactNode[]) => {
    for (const { id, item } of editable(page)) {
      (item.paths ?? []).forEach((sp, pi) => {
        const mine = sel && sel.id === id && sel.path === pi ? sel : null;
        const draft = drag && drag.sel.id === id && drag.sel.path === pi ? drag.draft : null;
        const pts = draft ?? sp.points;
        const m = item.transform;
        // the path in page coordinates
        const pagePts: Pt[] = pts.map((p) => { const a = at(m, p[0], p[1]), l = at(m, p[2], p[3]), r = at(m, p[4], p[5]); return [a[0], a[1], l[0], l[1], r[0], r[1]]; });
        items.push(<path key={`p${id}.${pi}`} className="point-path" d={svgPath(pagePts, sp.open, k)} />);
        // the neighbours' handles that shape the segments next to the selected points are shown with the selected points' own
        if (mine) {
          for (const { index: i, side } of neighborHandles(pagePts, sp.open, mine.indices)) {
            const p = pagePts[i], [hx, hy] = side === 'left' ? [p[2], p[3]] : [p[4], p[5]];
            items.push(<line key={`nh${side}${id}${pi}.${i}`} className="point-handle" data-neighbor="1" x1={p[0] * k} y1={p[1] * k} x2={hx * k} y2={hy * k} />);
            items.push(<circle key={`nc${side}${id}${pi}.${i}`} className="point-handle-dot" data-neighbor="1" cx={hx * k} cy={hy * k} r={3.5} />);
          }
        }
        pagePts.forEach((p, i) => {
          const on = mine?.indices.includes(i);
          if (on) {
            for (const [hx, hy, key] of [[p[2], p[3], 'l'], [p[4], p[5], 'r']] as const) {
              if (hx === p[0] && hy === p[1]) continue;
              items.push(<line key={`h${key}${id}${pi}.${i}`} className="point-handle" x1={p[0] * k} y1={p[1] * k} x2={hx * k} y2={hy * k} />);
              items.push(<circle key={`c${key}${id}${pi}.${i}`} className="point-handle-dot" cx={hx * k} cy={hy * k} r={3.5} />);
            }
          }
          items.push(<rect key={`a${id}${pi}.${i}`} className={on ? 'point-anchor on' : 'point-anchor'} x={p[0] * k - 3} y={p[1] * k - 3} width={6} height={6} />);
        });
      });
    }
  };

  /** A marquee on a page with shapes selected takes the anchor points inside it (Shift: adds them); false when it holds none. */
  const selectInRect = (page: number, rect: [number, number, number, number], add: boolean): boolean => {
    if (live.current.tool !== 'direct' || !live.current.editable) return false;
    for (const { id, item } of editable(page)) {
      for (const [path, sp] of (item.paths ?? []).entries()) {
        const inside = sp.points.flatMap((q, i) => {
          const a = at(item.transform, q[0], q[1]);
          return a[0] >= rect[0] && a[0] <= rect[2] && a[1] >= rect[1] && a[1] <= rect[3] ? [i] : [];
        });
        if (!inside.length) continue;
        const same = sel && sel.id === id && sel.path === path;
        setSel({ page, id, path, indices: add && same ? [...new Set([...sel.indices, ...inside])] : inside });
        return true;
      }
    }
    return false;
  };

  return { editable, pointAt, pointTool, segmentAt, down, move, up, draw, selectInRect };
}
