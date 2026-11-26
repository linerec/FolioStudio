// The live corner of the one selected rectangle or text frame (Selection tool): see livecorners.ts for the rules and the numbers.
import { useEffect, useRef, useState } from 'react';
import type { EditCommand } from '../../engine';
import type { Corner, PageItems } from '../../inspect';
import { t as tr } from '../../i18n';
import { findItem, type SelEntry } from '../../ui/overlay';
import type { ToolId } from '../../ui/tools';
import { DRAG_THRESHOLD_PX, cornersOfItem, diamondPlacement, dragBy, draggedCorners, shorterSide, type Point } from './livecorners';
import { LIVE_SIZE, hasLiveCorners, markPlacement, type ViewMarks } from './marks';

const HIT = 7;

type Drag = { id: string; page: number; corner: number; from: Point; start: Corner[]; shorter: number; el: HTMLElement; s: number; moved: boolean; by: number };
type Event = React.PointerEvent<HTMLDivElement>;
type At = { index: number; el: HTMLElement } | null;
type Props = {
  tool: ToolId; editable: boolean; selection: SelEntry[]; marks: ViewMarks | null;
  peek: (page: number) => PageItems | null | undefined;
  pxPerPt: (el: HTMLElement, rect: DOMRect) => number;
  edit: (label: string, commands: EditCommand[]) => unknown;
};

/**
 * The live corner of the one selected rectangle or text frame (Selection tool): a press on the yellow square turns the diamonds on (they stay until
 * the selection changes), a drag on a diamond changes the corners (one edit when the button goes up: InDesign draws no rounded preview while dragging),
 * an Alt-press without a drag cycles the shape (Shift: this corner only).
 */
export function useLiveCorners(p: Props) {
  const [editing, setEditing] = useState<string | null>(null);
  const [shown, setShown] = useState<{ id: string; corners: Corner[] } | null>(null);
  const drag = useRef<Drag | null>(null);
  const cancel = () => { drag.current = null; setShown(null); };
  const only = p.selection.length === 1 && !p.selection[0].content ? p.selection[0].id : null;
  useEffect(() => { if (editing && editing !== only) { setEditing(null); cancel(); } }, [only, editing]);
  useEffect(() => { if (editing && (p.tool !== 'selection' || !p.marks?.liveCorners)) { setEditing(null); cancel(); } }, [p.tool, p.marks?.liveCorners, editing]);

  const target = (at: NonNullable<At>) => {
    if (!p.editable || p.tool !== 'selection' || !p.marks?.liveCorners || !only || p.selection[0].page !== at.index) return null;
    const it = findItem(p.peek(at.index), only)?.item;
    return it && hasLiveCorners(it) ? it : null;
  };
  const down = (e: Event, at: At) => {
    if (!at) return false;
    const it = target(at);
    if (!it) return false;
    const r = at.el.getBoundingClientRect(), s = p.pxPerPt(at.el, r), px: Point = [e.clientX - r.left, e.clientY - r.top];
    if (editing !== it.id) {
      const m = markPlacement(it, s, { liveCorners: true, anchorControl: false }).live;
      if (!m || Math.hypot(px[0] - m.x, px[1] - m.y) > LIVE_SIZE / 2 + 2) return false;
      e.preventDefault();
      setEditing(it.id);
      return true;
    }
    const spots = diamondPlacement(it, s);
    const k = spots.findIndex(([x, y]) => Math.hypot(px[0] - x, px[1] - y) <= HIT);
    if (k < 0) return false;
    e.preventDefault();
    drag.current = { id: it.id, page: at.index, corner: k, from: px, start: cornersOfItem(it).map((c) => ({ ...c })), shorter: shorterSide(it), el: at.el, s, moved: false, by: 0 };
    return true;
  };
  const move = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    const it = findItem(p.peek(d.page), d.id)?.item;
    if (!it) { cancel(); return true; }
    const r = d.el.getBoundingClientRect();
    d.by = dragBy(it, d.corner, d.from, [e.clientX - r.left, e.clientY - r.top], d.s);
    d.moved = Math.hypot(e.clientX - r.left - d.from[0], e.clientY - r.top - d.from[1]) > DRAG_THRESHOLD_PX;
    setShown({ id: d.id, corners: draggedCorners(d.start, d.corner, d.by, e.shiftKey, d.shorter) });
    return true;
  };
  const up = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    move(e);
    cancel();
    if (d.moved) p.edit(tr('모퉁이 크기 바꾸기'), [{ op: 'liveCornerDrag', ids: [d.id], corner: d.corner, by: d.by, only_corner: e.shiftKey }]);
    else if (e.altKey) p.edit(tr('모퉁이 모양 바꾸기'), [{ op: 'cycleCornerShape', ids: [d.id], corner: d.corner, only_corner: e.shiftKey }]);
    return true;
  };
  /** What the page's overlay draws: the diamonds while one is dragged, else the diamonds of the frame in edit mode. */
  const liveFor = (page: number): { id: string; corners: Corner[] } | null => {
    if (shown) return shown;
    if (!editing) return null;
    const it = findItem(p.peek(page), editing)?.item;
    return it ? { id: editing, corners: cornersOfItem(it) } : null;
  };
  return { down, move, up, cancel, editing, liveFor };
}
