// The anchored object control of the one selected object (Selection or Direct Selection tool): a press on the blue square on its top edge and a drag
// into text anchors the object at the insertion point nearest the pointer (View ▸ Extras ▸ Show Anchored Object Control). Rules measured in InDesign
// 2025 with a real mouse (next/oracle/rules/liveinput-c2.ko.md): any drag that ends inside a text frame anchors (Shift and Ctrl change nothing), Alt opens
// the Anchored Object Options dialog on the object just anchored (Cancel keeps it anchored), a drop outside any text frame anchors nothing, and an
// object already set in text is anchored again at the new place (the same press on its anchor symbol). What the anchor means (the offsets that keep the
// object where it is, the story character) is worked out by the engine (`anchorObject`); the screen sends only the story and the character.
import { useRef } from 'react';
import type { EditCommand } from '../../engine';
import { textHit } from '../../engine';
import { corners, type ItemInfo, type PageItems } from '../../inspect';
import { t as tr } from '../../i18n';
import { findItem, type SelEntry } from '../../ui/overlay';
import type { ToolId } from '../../ui/tools';
import { ANCHOR_BOX, ANCHOR_SQUARE, markPlacement, type ViewMarks } from '../view/marks';

/** Pointer travel (screen px) below which a press on the control is only a click: nothing is anchored. */
export const ANCHOR_DRAG_PX = 4;

type Drag = { id: string; page: number; el: HTMLElement; from: [number, number]; moved: boolean };
type Event = React.PointerEvent<HTMLDivElement>;
type Props = {
  tool: ToolId; editable: boolean; selection: SelEntry[]; marks: ViewMarks | null;
  peek: (page: number) => PageItems | null | undefined;
  pxPerPt: (el: HTMLElement, rect: DOMRect) => number;
  /** A page of a spread turned by View ▸ Rotate Spread (its pointer tools are not built). */
  turned: (page: number) => boolean;
  edit: (label: string, commands: EditCommand[]) => Promise<unknown> | unknown;
  /** Opens the Anchored Object Options dialog on an object (the Alt drag). */
  options: (id: string) => void;
};

/** Is the page point inside the frame? A turned, flipped or scaled frame is the parallelogram its transform makes, not its bounding box. */
export function insideItem(it: ItemInfo, [x, y]: [number, number]): boolean {
  const q = corners(it.transform, it.inner);
  // the same side of all four edges (either way round: a flipped frame lists its corners the other way)
  const side = q.map((p, k) => { const r = q[(k + 1) % 4]; return (r[0] - p[0]) * (y - p[1]) - (r[1] - p[1]) * (x - p[0]); });
  return side.every((v) => v >= 0) || side.every((v) => v <= 0);
}

export function useAnchorControl(p: Props) {
  const drag = useRef<Drag | null>(null);
  const only = p.selection.length === 1 && !p.selection[0].content ? p.selection[0] : null;

  const down = (e: Event, at: { index: number; el: HTMLElement } | null) => {
    if (!at || !only || only.page !== at.index || !p.editable || !p.marks?.anchorControl || (p.tool !== 'selection' && p.tool !== 'direct') || p.turned(at.index)) return false;
    const it = findItem(p.peek(at.index), only.id)?.item;
    if (!it || it.locked) return false;
    const r = at.el.getBoundingClientRect(), s = p.pxPerPt(at.el, r), px: [number, number] = [e.clientX - r.left, e.clientY - r.top];
    const m = markPlacement(it, s, { liveCorners: false, anchorControl: true }).anchor;
    const half = (it.anchored ? ANCHOR_BOX : ANCHOR_SQUARE) / 2 + 2;
    if (!m || Math.abs(px[0] - m.x) > half || Math.abs(px[1] - m.y) > half) return false;
    e.preventDefault();
    drag.current = { id: it.id, page: at.index, el: at.el, from: [e.clientX, e.clientY], moved: false };
    return true;
  };
  const move = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    if (Math.hypot(e.clientX - d.from[0], e.clientY - d.from[1]) > ANCHOR_DRAG_PX) d.moved = true;
    return true;
  };
  /** `pageAt`: the page point under the pointer (the page it is over, which need not be the page the object is on). */
  const up = (e: Event, pageAt: (x: number, y: number) => { index: number; x: number; y: number } | null) => {
    const d = drag.current;
    drag.current = null;
    if (!d) return false;
    move(e);
    if (!d.moved) return true;
    const here = pageAt(e.clientX, e.clientY);
    if (!here) return true;
    const items = p.peek(here.index);
    const alt = e.altKey;
    void (async () => {
      const { hit } = await textHit(here.index, here.x, here.y);
      if (!hit) return;
      // only a drop inside a text frame anchors (InDesign: a drop beside the text anchors nothing)
      const frame = findItem(items, hit.frame)?.item;
      if (!frame || !insideItem(frame, [here.x, here.y]) || hit.page !== here.index) return;
      const result = await p.edit(tr('개체를 글에 고정'), [{ op: 'anchorObject', id: d.id, story: hit.story, at: hit.at }]);
      if (result && alt) p.options(d.id);
    })().catch(() => {});
    return true;
  };
  const cancel = () => { drag.current = null; };
  return { down, move, up, cancel, active: () => drag.current !== null };
}
