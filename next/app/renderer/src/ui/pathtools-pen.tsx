//! Pen gestures, open path continuation, cursor feedback and preview.

//   Pen: click for a corner point, drag for a smooth one (the handles follow the pointer); click the first point to close the
//   path (drag there: the closing point's handles); Enter, Esc, a double click or another tool ends an open one (the hand and zoom tools for the
//   moment, Space / Ctrl+Space, do not); Backspace takes the last point back. While dragging: Alt breaks the handles (only the outgoing one
//   follows), Space moves the point being placed, Shift keeps points and handles on 45° steps. A press on the last point takes its outgoing
//   handle back (a drag from it pulls a new one out alone). Points snap to guides and the grid. Ctrl: the Direct Selection tool for the moment.
//     On a selected path: over a segment a click adds a point, over a point it deletes it (Shift: neither), Alt over a point converts it.
//     On a selected open path, a click on one of its end points picks the path up: the clicks after it add points to that end, a click on an
//     end point of another open path joins the two there, a click on the path's other end closes it (penContinue, edit/src/ops/penextend.rs).
//     A new path ended on an end point of another open path is joined to it (penContinue on that path with the new points turned round).
//     The cursor says what a click does (pen-cur-* classes, ui/pentools.css).

import { type Dispatch, type RefObject, type SetStateAction, useCallback, useEffect } from 'react';
import type { EditCommand } from '../engine';
import { tr } from '../i18n';
import type { ItemInfo } from '../inspect';
import { typing } from './commands';
import { commandKey } from './keys';
import { type Pt, constrain45, svgPath } from './pathgeom';
import { type PointSel, type createPointTools, at, dist } from './pathtools-points';
import type { Here, Hover, Options, PageId, ShowSnap, SmartAt } from './pathtools-types';

type PenPoint = { anchor: [number, number]; left?: [number, number]; right?: [number, number] };
/** What a pen drag does: `new` pulls the handles of the point just put down, `retract` pulls the last point's outgoing handle alone (a
 *  press on it without a drag takes that handle back), `close` sets the first point's handles and closes the path when let go. */
type PenMode = 'new' | 'retract' | 'close';
/** `cont`: the open path `id` is being continued from its start or end; `points[0]` is that end point (not new), the others are the new points.
 *  `split`: Alt was pressed during the drag, the incoming handle stays; `space`: Space is held, the point follows the pointer from `from`. */
export type Pen = { page: number; points: PenPoint[]; hover: [number, number] | null; dragging: boolean; cont?: { id: string; atStart: boolean };
  mode?: PenMode; split?: boolean; space?: { from: [number, number]; at: PenPoint } };
/** What the cursor shows (and a click would do) with the Pen and point tools: the `pen-cur-*` class of the view. */
export type PenCursor = 'start' | 'draw' | 'close' | 'continue' | 'join' | 'add' | 'delete' | 'convert' | 'direct' | 'none';

const plus = (a: [number, number], d: [number, number]): [number, number] => [a[0] + d[0], a[1] + d[1]];
const mirror = (a: [number, number], h: [number, number]): [number, number] => [2 * a[0] - h[0], 2 * a[1] - h[1]];

export function useFinishPen(
  live: RefObject<Options>,
  penRef: RefObject<Pen | null>,
  setPen: Dispatch<SetStateAction<Pen | null>>,
  pageId: PageId,
) {
  /** Takes the path as it is now (or ends it open); the created item is selected. A path that was picked up by its end point goes to the engine
   *  as the new points after that end (`join`: the path ends on an end point of another open path, or on its own other end to close). */
  const finish = useCallback((closed: boolean, join?: { id: string; atStart: boolean }) => {
    const p = penRef.current;
    setPen(null);
    penRef.current = null;
    if (!p) return;
    const pts = [...p.points];
    // a double click leaves its second click on the first: take it off (the end point the path was picked up by is not a new point)
    const last = pts[pts.length - 1], before = pts[pts.length - 2];
    if (before && pts.length > (p.cont ? 2 : 1) && Math.hypot(last.anchor[0] - before.anchor[0], last.anchor[1] - before.anchor[1]) < 1) pts.pop();
    const page = pageId(p.page);
    if (p.cont) {
      const added = pts.slice(1);
      if (!page || (!added.length && !join)) return;
      const label = join ? (join.id === p.cont.id ? tr('패스 닫기') : tr('패스 합치기')) : tr('패스 이어 그리기');
      void live.current.edit(label, [{ op: 'penContinue', id: p.cont.id, page, at_start: p.cont.atStart,
        anchors: added.map((q) => q.anchor), lefts: added.map((q) => q.left ?? q.anchor), rights: added.map((q) => q.right ?? q.anchor),
        join: join?.id ?? '', join_at_start: join?.atStart ?? false }]).then((r) => {
        const id = r?.created?.[0];
        if (id) live.current.setSelection([{ page: p.page, id, content: false }]);
      });
      return;
    }
    if (join && page && pts.length) {
      // a new path ended on an end point of another open path: that path goes on through the new points, from the last one back to the first
      const back = [...pts].reverse();
      void live.current.edit(tr('패스 합치기'), [{ op: 'penContinue', id: join.id, page, at_start: join.atStart,
        anchors: back.map((q) => q.anchor), lefts: back.map((q) => q.right ?? q.anchor), rights: back.map((q) => q.left ?? q.anchor),
        join: '', join_at_start: false }]).then((r) => {
        const id = r?.created?.[0];
        if (id) live.current.setSelection([{ page: p.page, id, content: false }]);
      });
      return;
    }
    if (!page || pts.length < (closed ? 3 : 2)) return;
    // a new path takes the Tools panel's stroke: 1 pt black, no fill
    const black = live.current.info?.swatches.some((s) => s.id === 'Color/Black');
    const paint: EditCommand[] = black ? [{ op: 'setPaint', ids: ['@copies'], stroke: 'Color/Black', strokeWeight: 1 }] : [];
    void live.current.edit(tr('패스 만들기'), [{ op: 'createPath', page, closed, points: pts }, ...paint]).then((r) => {
      const id = r?.created?.[0];
      if (id) live.current.setSelection([{ page: p.page, id, content: false }]);
    });
  }, []);
  return finish;
}

type Finish = ReturnType<typeof useFinishPen>;

export function usePenKeys(
  pen: Pen | null,
  penRef: RefObject<Pen | null>,
  setPen: Dispatch<SetStateAction<Pen | null>>,
  live: RefObject<Options>,
  hoverRef: RefObject<Hover | null>,
  spaceRef: RefObject<boolean>,
  finish: Finish,
) {
  useEffect(() => {
    if (!pen) return;
    const key = (e: KeyboardEvent) => {
      if (typing(e)) return;
      // Space while a point is being dragged out moves that point (it is not the hand tool then)
      if (e.code === 'Space' && penRef.current?.dragging) {
        e.preventDefault();
        e.stopImmediatePropagation();
        const p = penRef.current, h = hoverRef.current;
        // the point goes with the pointer from where the pointer is now
        if (!spaceRef.current && p.mode === 'new' && !p.space && h && h.page === p.page) {
          const next: Pen = { ...p, space: { from: [h.x, h.y], at: p.points[p.points.length - 1] } };
          penRef.current = next;
          setPen(next);
        }
        spaceRef.current = true;
        return;
      }
      if (live.current.tool !== 'pen') return;
      // Edit ▸ Undo while a path is being drawn takes its last point back (the path is not in the document yet: an undo there would undo an
      // edit made before it); with no point left the path is given up. Redo has nothing to bring back then.
      if (commandKey(e) && !e.altKey && e.code === 'KeyZ') {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (!e.shiftKey) {
          const p = penRef.current;
          const keep = p?.cont ? 2 : 1;
          const next: Pen | null = p && p.points.length > keep ? { ...p, points: p.points.slice(0, -1), dragging: false, mode: undefined, space: undefined } : null;
          penRef.current = next;
          setPen(next);
        }
        return;
      }
      if (live.current.onKey?.(e)) { e.preventDefault(); e.stopImmediatePropagation(); }
      else if (e.key === 'Escape') { e.preventDefault(); e.stopImmediatePropagation(); finish(false); }
      else if (e.key === 'Backspace' || e.key === 'Delete') {
        e.preventDefault();
        e.stopImmediatePropagation();
        setPen((p) => (p && p.points.length > 1 ? { ...p, points: p.points.slice(0, -1) } : null));
      }
    };
    const up = (e: KeyboardEvent) => {
      if (e.code !== 'Space') return;
      spaceRef.current = false;
      // the handles follow the pointer again from where the point is now
      const p = penRef.current;
      if (p?.space) { const next: Pen = { ...p, space: undefined }; penRef.current = next; setPen(next); }
    };
    window.addEventListener('keydown', key, true);
    window.addEventListener('keyup', up, true);
    return () => { window.removeEventListener('keydown', key, true); window.removeEventListener('keyup', up, true); spaceRef.current = false; };
  }, [pen !== null, finish]);
}

type PenToolState = {
  live: RefObject<Options>;
  penRef: RefObject<Pen | null>;
  setPen: Dispatch<SetStateAction<Pen | null>>;
  setSel: Dispatch<SetStateAction<PointSel | null>>;
  spaceRef: RefObject<boolean>;
  hoverRef: RefObject<Hover | null>;
  setPenCursor: Dispatch<SetStateAction<PenCursor>>;
  finish: Finish;
  points: Pick<ReturnType<typeof createPointTools>, 'editable' | 'pointAt' | 'pointTool' | 'segmentAt'>;
  smartAt: SmartAt;
  showSnap: ShowSnap;
};

export function createPenTools({ live, penRef, setPen, setSel, spaceRef, hoverRef, setPenCursor, finish, points, smartAt, showSnap }: PenToolState) {
  const { editable, pointAt, pointTool, segmentAt } = points;

  /** The end points of an open path (a path of one open piece that is no frame) under a page point: the selected paths' ones, or `all` the page's. */
  const endAt = (page: number, p: [number, number], tol: number, all: boolean) => {
    const l = live.current;
    const list: ItemInfo[] = all
      ? (l.store.peek(page)?.items ?? []).filter((item) => {
        const layer = item.layer ? l.layers.get(item.layer) : undefined;
        return item.visible && !item.locked && !(layer && (layer.locked || !layer.visible));
      })
      : editable(page).map((e) => e.item);
    let best: { id: string; atStart: boolean; at: [number, number]; dist: number } | null = null;
    for (const item of list) {
      const sp = item.paths;
      if (!sp || sp.length !== 1 || !sp[0].open || sp[0].points.length < 2 || item.text) continue;
      for (const atStart of [true, false]) {
        const q = sp[0].points[atStart ? 0 : sp[0].points.length - 1];
        const a = at(item.transform, q[0], q[1]);
        const dist = Math.hypot(a[0] - p[0], a[1] - p[1]);
        if (dist <= tol && (!best || dist < best.dist)) best = { id: item.id, atStart, at: a, dist };
      }
    }
    return best;
  };

  /** What a Pen press at a page point does while no path is being drawn (Alt and Shift as now): the cursor shows it, the press does it. */
  const penIntent = (page: number, p: [number, number], tol: number, alt: boolean, shift: boolean):
    | { kind: 'continue'; end: { id: string; atStart: boolean; at: [number, number] } }
    | { kind: 'convert' } | { kind: 'delete'; id: string; path: number; index: number; count: number }
    | { kind: 'add'; id: string; path: number; segment: number; t: number } | { kind: 'start' } => {
    const hit = pointAt(page, p, tol);
    if (alt && hit) return { kind: 'convert' };
    const end = endAt(page, p, tol, false);
    if (end) return { kind: 'continue', end };
    if (shift) return { kind: 'start' };
    if (hit?.kind === 'anchor') {
      const count = editable(page).find((c) => c.id === hit.id)?.item.paths?.[hit.path]?.points.length ?? 0;
      return { kind: 'delete', id: hit.id, path: hit.path, index: hit.index, count };
    }
    const seg = segmentAt(page, p, tol);
    if (seg) return { kind: 'add', id: seg.id, path: seg.path, segment: seg.segment, t: seg.t };
    return { kind: 'start' };
  };

  /** Where a new Pen point goes: Shift keeps it on 45° steps from the point before, else it is pulled onto an anchor or a Smart Guide, the
   *  guides and the grid. */
  const penPlace = (page: number, raw: [number, number], before: [number, number] | null, shift: boolean): [number, number] => {
    const l = live.current;
    if (shift && before) return constrain45(before, raw);
    if (shift) return raw;
    return smartAt(page, raw, null) ?? (l.snap ? l.snap(page, raw) : raw);
  };

  /** The cursor for the place the pointer is over (hoverRef) and the keys held: what a press would do there. */
  const showCursor = (alt: boolean, shift: boolean, ctrl: boolean) => {
    const l = live.current, h = hoverRef.current, p = penRef.current;
    let c: PenCursor = 'none';
    if (l.tool === 'penAdd') c = 'add';
    else if (l.tool === 'penDelete') c = 'delete';
    else if (l.tool === 'convert') c = 'convert';
    else if (l.tool === 'pen' && ctrl) c = 'direct';
    else if (l.tool === 'pen' && !l.editable) c = 'start';
    else if (l.tool === 'pen' && h) {
      const tol = 6 / l.scale, pt: [number, number] = [h.x, h.y];
      if (!p) {
        const i = penIntent(h.page, pt, tol, alt, shift);
        c = i.kind;
      } else if (p.page !== h.page) c = 'draw';
      else {
        const end = endAt(h.page, pt, tol, true);
        const own = p.cont && end && end.id === p.cont.id && end.atStart === p.cont.atStart;
        if (!p.cont && p.points.length >= 3 && dist(pt, p.points[0].anchor) <= tol) c = 'close';
        else if (end && !own) c = p.cont && end.id === p.cont.id ? 'close' : 'join';
        else if (!own && p.points.length > (p.cont ? 1 : 0) && dist(pt, p.points[p.points.length - 1].anchor) <= tol) c = 'convert';
        else c = 'draw';
      }
    }
    setPenCursor((old) => (old === c ? old : c));
  };

  const down = (l: Options, e: React.PointerEvent, here: Here, tol: number): boolean => {
    e.preventDefault();
    const p = penRef.current;
    const raw: [number, number] = [here.x, here.y];
    if (p && p.page !== here.index) { l.note(tr('같은 페이지 안에서 이어 그리세요.')); return true; }
    if (p && e.detail >= 2) { finish(false); return true; }
    if (p) {
      const last = p.points[p.points.length - 1];
      const end = endAt(here.index, raw, tol, true);
      if (p.cont) {
        // an end point of an open path: the path ends there (another path: the two are joined; its own other end: it is closed)
        if (end && end.id === p.cont.id && end.atStart === p.cont.atStart) return true;
        if (end && !(end.id === p.cont.id && end.atStart === p.cont.atStart)) { finish(false, { id: end.id, atStart: end.atStart }); return true; }
      } else {
        // the first point: the path closes when the pointer is let go (a drag there shapes the closing point first)
        if (p.points.length >= 3 && dist(raw, p.points[0].anchor) <= tol) {
          const next: Pen = { ...p, dragging: true, mode: 'close', split: false, space: undefined };
          penRef.current = next;
          setPen(next);
          return true;
        }
        // an end point of another open path: the new path is joined to it there
        if (end && p.points.length >= 1) { finish(false, { id: end.id, atStart: end.atStart }); return true; }
      }
      // the last point again: its outgoing handle is taken back; a drag from it pulls a new one out on its own
      if (p.points.length > (p.cont ? 1 : 0) && dist(raw, last.anchor) <= tol) {
        const pts = [...p.points];
        pts[pts.length - 1] = { anchor: last.anchor, left: last.left };
        const next: Pen = { ...p, points: pts, dragging: true, mode: 'retract', split: false, space: undefined };
        penRef.current = next;
        setPen(next);
        return true;
      }
    }
    const layer = l.info?.layers.find((x) => x.id === l.info?.activeLayer);
    if (layer && (layer.locked || !layer.visible)) { l.note(tr('활성 레이어가 잠겼거나 숨겨져 있습니다.')); return true; }
    if (!p) {
      // on a selected path: continue it from an end point, add or delete a point (Shift: neither), convert a point with Alt
      const intent = penIntent(here.index, raw, tol, e.altKey, e.shiftKey);
      if (intent.kind === 'convert') return pointTool(e, here, tol, 'convert');
      if (intent.kind === 'continue') {
        const next: Pen = { page: here.index, points: [{ anchor: intent.end.at }], hover: null, dragging: false, cont: { id: intent.end.id, atStart: intent.end.atStart } };
        penRef.current = next;
        setPen(next);
        return true;
      }
      if (intent.kind === 'delete') {
        if (intent.count <= 2) l.note(tr('패스에는 점이 두 개 이상 있어야 합니다.'));
        else { setSel(null); void l.edit(tr('고정점 삭제'), [{ op: 'deletePathPoint', id: intent.id, path: intent.path, index: intent.index }]); }
        return true;
      }
      if (intent.kind === 'add') {
        void l.edit(tr('고정점 추가'), [{ op: 'addPathPoint', id: intent.id, path: intent.path, segment: intent.segment, t: intent.t }]);
        return true;
      }
    }
    const before = p ? p.points[p.points.length - 1].anchor : null;
    const next: Pen = { ...(p ?? { hover: null }), page: here.index, points: [...(p?.points ?? []), { anchor: penPlace(here.index, raw, before, e.shiftKey) }],
      hover: null, dragging: true, mode: 'new', split: false, space: undefined };
    penRef.current = next;
    setPen(next);
    return true;
  };

  const move = (l: Options, e: React.PointerEvent, here: Here | null, p: Pen): boolean => {
    if (!here || here.index !== p.page) return true;
    const raw: [number, number] = [here.x, here.y];
    if (p.dragging) {
      const pts = [...p.points];
      const k = pts.length - 1;
      const far = 3 / l.scale;
      let next: Pen = p;
      if (p.mode === 'close') {
        // the first point's handles: the outgoing one follows the pointer, the incoming one is its mirror (Alt: only the incoming one changes)
        const f = pts[0], to = e.shiftKey ? constrain45(f.anchor, raw) : raw;
        if (dist(to, f.anchor) > far) pts[0] = { anchor: f.anchor, left: mirror(f.anchor, to), right: e.altKey ? f.right : to };
        next = { ...p, points: pts };
      } else if (p.mode === 'retract') {
        // the last point's outgoing handle alone follows the pointer; its incoming one stays
        const q = pts[k], to = e.shiftKey ? constrain45(q.anchor, raw) : raw;
        pts[k] = dist(to, q.anchor) > far ? { ...q, right: to } : { anchor: q.anchor, left: q.left };
        next = { ...p, points: pts };
      } else if (spaceRef.current) {
        // Space held: the point (with its handles) follows the pointer
        const space = p.space ?? { from: raw, at: pts[k] };
        const dd: [number, number] = [raw[0] - space.from[0], raw[1] - space.from[1]];
        const q = space.at;
        pts[k] = { anchor: plus(q.anchor, dd), ...(q.left ? { left: plus(q.left, dd) } : {}), ...(q.right ? { right: plus(q.right, dd) } : {}) };
        next = { ...p, points: pts, space };
      } else {
        // dragging out of a new point pulls its handles out, one each way; Alt breaks them (the incoming one stays where it is now)
        const q = pts[k], a = q.anchor, to = e.shiftKey ? constrain45(a, raw) : raw;
        const split = p.split || e.altKey;
        if (dist(to, a) > far) pts[k] = { anchor: a, right: to, left: split ? q.left : mirror(a, to) };
        next = { ...p, points: pts, split, space: undefined };
      }
      penRef.current = next;
      setPen(next);
      showSnap(p.page, null);
    } else {
      // the segment to the pointer goes where a click would put the next point (an anchor or a Smart Guide catches it)
      const last = p.points[p.points.length - 1].anchor;
      const end = endAt(here.index, raw, 6 / l.scale, true);
      const closing = !p.cont && p.points.length >= 3 && dist(raw, p.points[0].anchor) <= 6 / l.scale;
      let hover: [number, number] = raw;
      if (e.shiftKey) { hover = constrain45(last, raw); showSnap(p.page, null); }
      else if (end || closing) showSnap(p.page, null);
      else hover = smartAt(p.page, raw, null) ?? raw;
      setPen({ ...p, hover });
    }
    return true;
  };

  const up = (p: Pen): boolean => {
    if (p.dragging && p.mode === 'close') { finish(true); return true; }
    if (p.dragging) {
      const next: Pen = { ...p, dragging: false, mode: undefined, split: false, space: undefined };
      penRef.current = next;
      setPen(next);
    }
    return true;
  };

  const draw = (pen: Pen, page: number, k: number, items: React.ReactNode[]) => {
    const pts: Pt[] = pen.points.map((p) => [p.anchor[0], p.anchor[1], (p.left ?? p.anchor)[0], (p.left ?? p.anchor)[1], (p.right ?? p.anchor)[0], (p.right ?? p.anchor)[1]]);
    // the segment to the pointer follows the last point's handle
    const last = pts[pts.length - 1];
    const ghost = pen.hover && !pen.dragging ? [last, [pen.hover[0], pen.hover[1], pen.hover[0], pen.hover[1], pen.hover[0], pen.hover[1]] as Pt] : null;
    // closing on the first point (a drag there shapes it): the closing segment shows too
    items.push(<path key="pen" className="pen-path" d={svgPath(pts, !(pen.dragging && pen.mode === 'close'), k)} />);
    if (ghost) items.push(<path key="ghost" className="pen-ghost" d={svgPath(ghost, true, k)} />);
    pts.forEach((p, i) => {
      if (p[2] !== p[0] || p[3] !== p[1]) items.push(<line key={`hl${i}`} className="pen-handle" x1={p[0] * k} y1={p[1] * k} x2={p[2] * k} y2={p[3] * k} />);
      if (p[4] !== p[0] || p[5] !== p[1]) items.push(<line key={`hr${i}`} className="pen-handle" x1={p[0] * k} y1={p[1] * k} x2={p[4] * k} y2={p[5] * k} />);
      items.push(<rect key={`a${i}`} className={i === 0 ? 'pen-anchor first' : 'pen-anchor'} x={p[0] * k - 3} y={p[1] * k - 3} width={6} height={6} />);
    });
    // the end point of another open path (or, continuing one, its own other end) under the pointer is where a click ends the path, joined to it
    if (pen.hover && !pen.dragging) {
      const end = endAt(page, pen.hover, 6 / k, true);
      const cont = pen.cont;
      if (end && !(cont && end.id === cont.id && end.atStart === cont.atStart)) items.push(<circle key="joinhint" className="pen-join" data-join={cont && end.id === cont.id ? 'close' : 'join'} cx={end.at[0] * k} cy={end.at[1] * k} r={8} />);
    }
  };

  return { penIntent, showCursor, down, move, up, draw };
}
