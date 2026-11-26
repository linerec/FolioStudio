// The bracket handles of the one selected text path (Selection, Direct Selection and Type on a Path tool): drawn from the engine's `textPathBrackets` query,
// the start and end bracket (a line across the path) and the centre tick grabbed and dragged with the Selection or Direct Selection tool (rules measured in
// InDesign 2025, next/oracle/rules/typepath-c14.ko.md): the press has to be on the line (3 px each side of it, along its whole length) or on the tick; the
// small squares at the lines are the text ports, not handles; the Type on a Path tool shows the handles but a drag there does not move them. What a drag
// means (the pointer's projection onto the path, the stop at the other bracket, the centre carrying both) is the engine's (`setTextPathBracket`); the screen
// only hands it the pointer, rounded to InDesign's drag grid, and shows what the engine says the brackets would be while the pointer is down.
import { useEffect, useRef, useState } from 'react';
import type { EditCommand } from '../../engine';
import { apply, type ItemInfo, type Matrix, type PageItems } from '../../inspect';
import { t as tr } from '../../i18n';
import { findItem, type SelEntry } from '../../ui/overlay';
import { inDesignZoom, snapPointer } from '../../ui/dragunit';
import type { ToolId } from '../../ui/tools';

export type Handle = { at: number; point: [number, number]; tangent: [number, number] };
export type BracketHandles = { start: Handle; end: Handle; center: Handle; above: number; below: number; closed: boolean; flipped: boolean; length: number };
export type Which = 'start' | 'end' | 'center';
/** What a page draws: the handles of the selected text path (`dragging`: the one in the pointer's hand). */
export type BracketView = { id: string; handles: BracketHandles; dragging: Which | null };

/** Measured sizes (screen px; InDesign's physical pixel is Folio's CSS pixel throughout): the port square, the centre tick, the grab zones. */
export const PORT = 11;
export const TICK = 12;
const LINE_ZONE = 3.5;
const TICK_ZONE = { lateral: 3.5, up: 14, down: 3 };
const BASE_ZONE = { lateral: 7, up: 3 };
/** Pointer travel (screen px) below which a press on a handle is only a click. */
const CLICK_PX = 2;

type Event = React.PointerEvent<HTMLDivElement>;
type Props = {
  tool: ToolId; editable: boolean; selection: SelEntry[];
  peek: (page: number) => PageItems | null | undefined;
  pxPerPt: (el: HTMLElement, rect: DOMRect) => number;
  /** The spread coordinates of a page's top left corner (the engine's `point` is in spread coordinates). */
  origin: (page: number) => [number, number];
  turned: (page: number) => boolean;
  edit: (label: string, commands: EditCommand[]) => Promise<unknown> | unknown;
};
type Drag = { id: string; page: number; el: HTMLElement; which: Which; from: [number, number]; press: [number, number]; moved: boolean; last: [number, number] };

const SHAPES = new Set(['line', 'rectangle', 'oval', 'polygon']);
export const bracketTools = (tool: ToolId) => tool === 'selection' || tool === 'direct' || tool === 'typePath';

/** A direction of the item's own coordinates in page pixels (unit length). */
export function mapDirection(m: Matrix, d: [number, number]): [number, number] {
  const x = m[0] * d[0] + m[2] * d[1], y = m[1] * d[0] + m[3] * d[1], n = Math.hypot(x, y) || 1;
  return [x / n, y / n];
}

/** Where a bracket stands on the page (points) and the unit direction of the path and of "above" (the left of travel on screen: `(t.y, -t.x)`). */
export function placeBracket(it: ItemInfo, h: Handle): { p: [number, number]; t: [number, number]; n: [number, number] } {
  const t = mapDirection(it.transform, h.tangent);
  return { p: apply(it.transform, h.point[0], h.point[1]), t, n: [t[1], -t[0]] };
}

/** Whether the screen point `q` (px, page-relative) is on the bracket `which` of `view` at scale `s` (px per point). */
export function hitBracket(it: ItemInfo, view: BracketHandles, q: [number, number], s: number): Which | null {
  const test = (which: Which) => {
    const h = view[which];
    const { p, t, n } = placeBracket(it, h);
    const dx = q[0] - p[0] * s, dy = q[1] - p[1] * s;
    const along = dx * t[0] + dy * t[1], up = dx * n[0] + dy * n[1];
    if (which === 'center') {
      return (Math.abs(along) <= TICK_ZONE.lateral && up <= TICK_ZONE.up && up >= -TICK_ZONE.down) || (Math.abs(along) <= BASE_ZONE.lateral && Math.abs(up) <= BASE_ZONE.up);
    }
    return Math.abs(along) <= LINE_ZONE && up <= view.above * s + 1 && up >= -view.below * s - 1;
  };
  return (['start', 'end', 'center'] as const).find(test) ?? null;
}

export function useTextPathBrackets(p: Props) {
  const only = p.selection.length === 1 && !p.selection[0].content ? p.selection[0] : null;
  const items = only ? p.peek(only.page) : null;
  const item = only && items ? findItem(items, only.id)?.item ?? null : null;
  const eligible = !!item && SHAPES.has(item.kind) && bracketTools(p.tool);
  const [data, setData] = useState<{ id: string; handles: BracketHandles; draggable: boolean } | null>(null);
  const [dragView, setDragView] = useState<{ which: Which; handles: BracketHandles } | null>(null);
  const drag = useRef<Drag | null>(null);
  const busy = useRef(false);
  const queued = useRef<[number, number] | null>(null);

  // The handles of the selected item, again when the page's items change (an edit, an undo).
  useEffect(() => {
    if (!eligible || !only) { setData(null); return; }
    let live = true;
    void window.folio.request({ cmd: 'textPathBrackets', target: only.id }).then(({ header }) => {
      if (live) setData(header.ok && header.supported ? { id: only.id, handles: header.handles as BracketHandles, draggable: !!header.draggable } : null);
    }).catch(() => { if (live) setData(null); });
    return () => { live = false; };
  }, [eligible, only?.id, items]);

  const mine = eligible && item && data?.id === item.id ? data : null;

  /** The pointer as the engine reads it: page point, rounded to InDesign's drag grid, in spread coordinates. */
  const pointer = (d: Drag, e: Event): [number, number] => {
    const r = d.el.getBoundingClientRect(), s = p.pxPerPt(d.el, r), z = inDesignZoom(s), o = p.origin(d.page);
    return [snapPointer((e.clientX - r.left) / s, z) + o[0], snapPointer((e.clientY - r.top) / s, z) + o[1]];
  };
  const preview = (d: Drag, point: [number, number]) => {
    if (busy.current) { queued.current = point; return; }
    busy.current = true;
    void window.folio.request({ cmd: 'textPathBrackets', target: d.id, drag: { which: d.which, point, from: d.from } }).then(({ header }) => {
      if (drag.current === d && header.ok && header.supported) setDragView({ which: d.which, handles: header.handles as BracketHandles });
    }).catch(() => {}).finally(() => {
      busy.current = false;
      const next = queued.current;
      queued.current = null;
      if (next && drag.current === d) preview(d, next);
    });
  };

  const down = (e: Event, at: { index: number; el: HTMLElement } | null) => {
    if (!at || !mine || !item || !only || only.page !== at.index || !mine.draggable || !p.editable || item.locked || (p.tool !== 'selection' && p.tool !== 'direct') || p.turned(at.index)) return false;
    const r = at.el.getBoundingClientRect(), s = p.pxPerPt(at.el, r);
    const which = hitBracket(item, mine.handles, [e.clientX - r.left, e.clientY - r.top], s);
    if (!which) return false;
    e.preventDefault();
    const d: Drag = { id: item.id, page: at.index, el: at.el, which, from: [0, 0], press: [e.clientX, e.clientY], moved: false, last: [0, 0] };
    d.from = pointer(d, e);
    d.last = d.from;
    drag.current = d;
    return true;
  };
  const move = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    if (Math.hypot(e.clientX - d.press[0], e.clientY - d.press[1]) > CLICK_PX) d.moved = true;
    if (d.moved) { d.last = pointer(d, e); preview(d, d.last); }
    return true;
  };
  const up = (e: Event) => {
    const d = drag.current;
    drag.current = null;
    setDragView(null);
    if (!d) return false;
    move(e);
    if (!d.moved) return true;
    const point = pointer(d, e);
    void Promise.resolve(p.edit(tr('패스에 입력 괄호 끌기'), [{ op: 'setTextPathBracket', id: d.id, which: d.which, point, from: d.from }])).catch(() => {});
    return true;
  };
  const cancel = () => { drag.current = null; setDragView(null); };

  /** What a page draws: the handles (the dragged state while a bracket is in the pointer's hand) of the selected text path on it. */
  const view = (page: number): BracketView | null => {
    if (!mine || !only || only.page !== page) return null;
    return { id: mine.id, handles: dragView?.handles ?? mine.handles, dragging: dragView?.which ?? null };
  };
  return { down, move, up, cancel, view, active: () => drag.current !== null };
}
