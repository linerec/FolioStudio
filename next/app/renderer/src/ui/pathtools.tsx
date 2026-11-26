//! Shared state, event routing and overlays for the path tools.

import { useEffect, useRef, useState } from 'react';
import { commandKey } from './keys';
import { type Pen, type PenCursor, createPenTools, useFinishPen, usePenKeys } from './pathtools-pen';
import { type Drag, type PointSel, createPointTools, usePointToolKeys } from './pathtools-points';
import { type Pencil, useStrokeTools } from './pathtools-stroke';
import type { Here, Hover, Options } from './pathtools-types';
import type { PointSnap, SkipPoints } from './pointsnap';
import type { ToolId } from './tools';
import './pentools.css';

export type { PenCursor } from './pathtools-pen';

/** The tools that show a shape's points (the Pen too while no path is being drawn: it adds, deletes and continues on them). */
const POINT_TOOLS: ToolId[] = ['direct', 'penAdd', 'penDelete', 'convert', 'scissors', 'pen'];
/** The tools between which a point selection stays (the Pen with Alt converts, the Convert Direction Point tool breaks the selected point's handles). */
const KEEP_POINTS: ToolId[] = ['direct', 'convert', 'pen', 'hand', 'zoom'];

export function usePathTools(o: Options) {
  const [pen, setPen] = useState<Pen | null>(null);
  const [pencil, setPencil] = useState<Pencil | null>(null);
  const pencilRef = useRef(pencil);
  pencilRef.current = pencil;
  const [sel, setSel] = useState<PointSel | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const [penCursor, setPenCursor] = useState<PenCursor>('none');
  /** The anchor point a point being put or dragged is caught by (Smart Guides), marked on its page. */
  const [snapMark, setSnapMark] = useState<{ page: number; at: [number, number] } | null>(null);
  const live = useRef(o);
  live.current = o;
  const penRef = useRef(pen);
  penRef.current = pen;
  const dragRef = useRef(drag);
  dragRef.current = drag;
  const selRef = useRef(sel);
  selRef.current = sel;
  /** Space held (the Pen moves the point being placed with it). */
  const spaceRef = useRef(false);
  /** The last place the pointer was over a page with the Pen or a point tool, and its keys (the cursor is worked out again when a key changes). */
  const hoverRef = useRef<Hover | null>(null);

  // the point selection belongs to the shape selection; it stays while the tool goes between Direct Selection, Convert Direction Point and the Pen
  useEffect(() => { setSel(null); }, [o.selection.map((s) => s.id).join('|')]);
  useEffect(() => { if (!KEEP_POINTS.includes(o.tool)) setSel(null); }, [o.tool]);

  const pageId = (index: number) => live.current.info?.pages.find((pg) => pg.index === index)?.id;

  const finish = useFinishPen(live, penRef, setPen, pageId);
  const stroke = useStrokeTools(live, pencil, pencilRef, setPencil, pageId);

  // another tool ends the path being drawn; the hand and zoom tools held for the moment (Space, Ctrl+Space) do not: the path goes on after them
  useEffect(() => {
    if (o.tool !== 'pen' && o.tool !== 'hand' && o.tool !== 'zoom' && penRef.current) finish(false);
  }, [o.tool, finish]);
  useEffect(() => { if (o.tool !== 'pencil' && o.tool !== 'erase' && o.tool !== 'smooth') setPencil(null); }, [o.tool]);
  // another tool, or the path ended: no anchor stays marked
  useEffect(() => { setSnapMark(null); live.current.showSmart?.(0, null); }, [o.tool, pen === null]);

  usePenKeys(pen, penRef, setPen, live, hoverRef, spaceRef, finish);
  usePointToolKeys(o, live, sel, setSel);

  /** The Smart Guides result shown now: the anchor a point landed on is marked, the alignment lines are drawn by the view. */
  const showSnap = (page: number, s: PointSnap | null) => {
    const mark = s?.anchor ? { page, at: s.anchor } : null;
    setSnapMark((old) => (old?.page === mark?.page && old?.at[0] === mark?.at[0] && old?.at[1] === mark?.at[1] ? old : mark));
    live.current.showSmart?.(page, s && !s.anchor ? s.draw : null);
  };
  /** A point pulled by the Smart Guides (an anchor of a path, else the object and page lines along each axis; the guides and the grid
   *  pull an axis the Smart Guides left), shown as it lands; null when they caught nothing. `anchorsOnly`: a handle, caught by anchors alone. */
  const smartAt = (page: number, raw: [number, number], skip: SkipPoints, anchorsOnly = false): [number, number] | null => {
    const l = live.current;
    const s = l.smartPoint?.(page, raw, skip) ?? null;
    if (!s || (!s.pulled[0] && !s.pulled[1]) || (anchorsOnly && !s.anchor)) { showSnap(page, null); return null; }
    showSnap(page, s);
    if (s.anchor) return s.pt;
    const g = l.snap ? l.snap(page, raw) : raw;
    return [s.pulled[0] ? s.pt[0] : g[0], s.pulled[1] ? s.pt[1] : g[1]];
  };

  const points = createPointTools({ live, sel, selRef, setSel, drag, dragRef, setDrag, smartAt, showSnap });
  const penActions = createPenTools({ live, penRef, setPen, setSel, spaceRef, hoverRef, setPenCursor, finish, points, smartAt, showSnap });
  const { penIntent, showCursor } = penActions;

  const onDown = (e: React.PointerEvent, here: Here | null): boolean => {
    const l = live.current;
    if (!l.editable || !here || e.button !== 0) return false;
    const tol = 6 / l.scale;
    // Ctrl with the Pen: the Direct Selection tool for the moment (a path being drawn ends first)
    const asDirect = l.tool === 'pen' && commandKey(e);
    if (asDirect && penRef.current) { finish(false); return true; }
    if (l.tool === 'pen' && !asDirect) return penActions.down(l, e, here, tol);
    if (stroke.down(l, e, here)) return true;
    if (l.tool === 'penAdd' || l.tool === 'penDelete' || l.tool === 'convert' || l.tool === 'scissors') return points.pointTool(e, here, tol);
    if (l.tool !== 'direct' && !asDirect) return false;
    return points.down(l, e, here, tol);
  };

  useEffect(() => {
    if (o.tool !== 'pen' && o.tool !== 'penAdd' && o.tool !== 'penDelete' && o.tool !== 'convert') { setPenCursor('none'); return; }
    showCursor(false, false, false);
    // a key pressed or let go changes what a press would do where the pointer is
    const key = (e: KeyboardEvent) => { if (e.key === 'Alt' || e.key === 'Shift' || e.key === 'Control' || e.key === 'Meta') showCursor(e.altKey, e.shiftKey, commandKey(e)); };
    window.addEventListener('keydown', key);
    window.addEventListener('keyup', key);
    return () => { window.removeEventListener('keydown', key); window.removeEventListener('keyup', key); };
  }, [o.tool, pen !== null, o.selection, o.editable]);

  const onMove = (e: React.PointerEvent, here: Here | null): boolean => {
    const l = live.current;
    const p = penRef.current;
    if (stroke.move(l, e, here)) return true;
    if (here && (l.tool === 'pen' || POINT_TOOLS.includes(l.tool))) {
      hoverRef.current = { page: here.index, x: here.x, y: here.y, clientX: e.clientX, clientY: e.clientY };
      if (!(p?.dragging) && !dragRef.current) showCursor(e.altKey, e.shiftKey, commandKey(e));
      // the Pen before a path is begun: the anchor or Smart Guide a click would put the first point on is shown
      if (l.tool === 'pen' && !p && !dragRef.current) {
        if (!e.shiftKey && !commandKey(e) && l.editable && penIntent(here.index, [here.x, here.y], 6 / l.scale, e.altKey, false).kind === 'start') smartAt(here.index, [here.x, here.y], null);
        else showSnap(here.index, null);
      }
    }
    if (l.tool === 'pen' && p) return penActions.move(l, e, here, p);
    return points.move(l, e, here);
  };

  const onUp = (e: React.PointerEvent, here: Here | null): boolean => {
    const l = live.current;
    const p = penRef.current;
    if (stroke.up(l, e, here)) return true;
    if (l.tool === 'pen' && p) return penActions.up(p);
    return points.up(l, e, here);
  };

  /** What the tools draw over a page: the path being made, or the points of the selected shapes. */
  const overlay = (page: number, w: number, h: number): React.ReactNode => {
    const k = o.scale;
    const items: React.ReactNode[] = [];
    stroke.draw(page, k, items);
    if (pen && pen.page === page) penActions.draw(pen, page, k, items);
    else if (POINT_TOOLS.includes(o.tool)) points.draw(page, k, items);
    // the anchor point a Smart Guide caught the point on (the Pen, a dragged anchor or handle)
    if (snapMark && snapMark.page === page) items.push(<rect key="snap" className="pen-snap" data-snap="anchor" x={snapMark.at[0] * k - 5} y={snapMark.at[1] * k - 5} width={10} height={10} />);
    return items.length ? <svg className="path-overlay" width={Math.round(w * k)} height={Math.round(h * k)}>{items}</svg> : null;
  };

  return { pen, sel, onDown, onMove, onUp, overlay, finish, selectInRect: points.selectInRect, dragging: drag !== null, cursor: penCursor };
}
