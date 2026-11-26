import { useEffect, useRef, useState } from 'react';
import { t as tr } from '../i18n';
import type { EditCommand } from '../engine';
import { type DocumentInfo, type ItemStore, type Matrix, pageOf, unionBox } from '../inspect';
import { type Ghost, type SelEntry, findItem, handleAt } from './overlay';
import type { ToolId } from './tools';
import { scaleAdjust } from './transformmenu';
import { commandKey } from './keys';

export const TRANSFORM_TOOLS = new Set<ToolId>(['rotate', 'scale', 'shear', 'freeTransform']);
const DIALOGS: Partial<Record<ToolId, 'rotate' | 'scale' | 'shear'>> = { rotate: 'rotate', scale: 'scale', shear: 'shear', freeTransform: 'scale' };
export const transformDialogFor = (tool: ToolId) => DIALOGS[tool];
type Point = { page: number; x: number; y: number };
type Event = React.PointerEvent<HTMLDivElement>;
/** `about`: a Free Transform handle's two points, the opposite handle and the middle of the box (Alt scales about the middle, InDesign). */
type Drag = { page: number; x: number; y: number; cx: number; cy: number; ids: string[]; mode: 'rotate' | 'scale' | 'shear' | 'move' | 'pivot'; copy: boolean; moved: boolean; matrix: Matrix; by: number; sx: number; sy: number;
  about?: { opposite: [number, number]; middle: [number, number] } };
type Props = {
  tool: ToolId; editable: boolean; selection: SelEntry[]; store: ItemStore; info: DocumentInfo | null; scale: number;
  view: React.RefObject<HTMLDivElement | null>;
  pxPerPt: (el: HTMLElement, rect: DOMRect) => number;
  /** A client point on a page in its own space (docview: a spread turned by View ▸ Rotate Spread is read as it is shown); absent: the page as drawn. */
  pointOn?: (page: number, clientX: number, clientY: number) => [number, number] | null;
  ghost: (ghost: { pages: number[]; g: Ghost } | null) => void;
  edit: (label: string, commands: EditCommand[]) => unknown;
};

/** A gesture owns its preview; the document receives one atomic edit on release. */
export function useTransformTools(p: Props) {
  const drag = useRef<Drag | null>(null);
  const [customPivot, setPivot] = useState<Point | null>(null);
  const cancel = () => { drag.current = null; p.ghost(null); };
  useEffect(() => { cancel(); setPivot(null); }, [p.tool, p.selection, p.editable]);
  useEffect(() => {
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape' && drag.current) { cancel(); e.preventDefault(); e.stopImmediatePropagation(); } };
    window.addEventListener('keydown', escape, true);
    return () => window.removeEventListener('keydown', escape, true);
  }, []);
  const page = p.selection[0]?.page;
  const items = page === undefined ? null : p.store.peek(page);
  const selected = items ? p.selection.map(s => findItem(items, s.id)?.item) : [];
  const box = selected.length && selected.every(Boolean) ? unionBox(selected.map(i => i!.bounds)) : null;
  const pivot = TRANSFORM_TOOLS.has(p.tool) && box && page !== undefined
    ? customPivot ?? { page, x: (box[0] + box[2]) / 2, y: (box[1] + box[3]) / 2 } : null;
  const down = (e: Event, here: { index: number; x: number; y: number } | null) => {
    if (!TRANSFORM_TOOLS.has(p.tool)) return false;
    if (!p.editable || !here || !pivot || !box || p.selection.some(s => s.content || s.page !== here.index) || selected.some(i => !i || i.locked)) return true;
    let mode: Drag['mode'] = p.tool === 'freeTransform' ? 'move' : p.tool as 'rotate' | 'scale' | 'shear';
    let cx = pivot.x, cy = pivot.y, about: Drag['about'];
    if (Math.hypot(here.x - cx, here.y - cy) * p.scale < 9) mode = 'pivot';
    else if (p.tool === 'freeTransform') {
      const h = handleAt(items, p.selection, here.index, [here.x, here.y], 7 / p.scale);
      if (h) {
        mode = commandKey(e) ? 'shear' : 'scale';
        if (!customPivot) {
          about = { opposite: [box[0] + (1 - h.fx) * (box[2] - box[0]), box[1] + (1 - h.fy) * (box[3] - box[1])], middle: [(box[0] + box[2]) / 2, (box[1] + box[3]) / 2] };
          [cx, cy] = e.altKey && mode === 'scale' ? about.middle : about.opposite;
        }
      } else if (here.x < box[0] || here.x > box[2] || here.y < box[1] || here.y > box[3]) mode = 'rotate';
    }
    e.preventDefault();
    // Alt makes a copy (a Free Transform handle: Alt scales about the middle instead); pressed any time during the drag, as InDesign
    const copy = e.altKey && !(about && mode === 'scale');
    drag.current = { page: here.index, x: here.x, y: here.y, cx, cy, ids: p.selection.map(s => s.id), mode, copy, moved: false, matrix: [1, 0, 0, 1, 0, 0], by: 0, sx: 1, sy: 1, about };
    return true;
  };
  const move = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    const el = p.view.current?.querySelector<HTMLElement>(`.page[data-page="${d.page}"]`);
    if (!el) return true;
    const r = el.getBoundingClientRect(), k = p.pxPerPt(el, r);
    const [x, y] = p.pointOn?.(d.page, e.clientX, e.clientY) ?? [(e.clientX - r.left) / k, (e.clientY - r.top) / k];
    if (!d.moved && Math.hypot(x - d.x, y - d.y) * k < 3) return true;
    d.moved = true;
    if (d.mode === 'pivot') { setPivot({ page: d.page, x, y }); return true; }
    if (d.about && d.mode === 'scale') [d.cx, d.cy] = e.altKey ? d.about.middle : d.about.opposite;
    else if (e.altKey) d.copy = true;
    let m: Matrix = [1, 0, 0, 1, 0, 0];
    if (d.mode === 'rotate') {
      let a = (Math.atan2(d.cy - y, x - d.cx) - Math.atan2(d.cy - d.y, d.x - d.cx)) * 180 / Math.PI;
      a = ((a + 540) % 360) - 180;
      d.by = e.shiftKey ? Math.round(a / 45) * 45 : a;
      const rad = d.by * Math.PI / 180, c = Math.cos(rad), s = Math.sin(rad);
      m = [c, -s, s, c, d.cx * (1 - c) - d.cy * s, d.cy * (1 - c) + d.cx * s];
    } else if (d.mode === 'scale') {
      const dx = d.x - d.cx, dy = d.y - d.cy;
      let sx = Math.abs(dx) < 1e-6 ? 1 : (x - d.cx) / dx;
      let sy = Math.abs(dy) < 1e-6 ? 1 : (y - d.cy) / dy;
      if (e.shiftKey) sx = sy = Math.abs(dx) >= Math.abs(dy) ? sx : sy;
      d.sx = Math.max(.001, sx); d.sy = Math.max(.001, sy);
      m = [d.sx, 0, 0, d.sy, d.cx * (1 - d.sx), d.cy * (1 - d.sy)];
    } else if (d.mode === 'shear') {
      const dy = d.y - d.cy;
      let a = Math.atan2(-(x - d.x), Math.abs(dy) < 1e-6 ? 1 : dy) * 180 / Math.PI;
      if (a > 90) a -= 180; if (a < -90) a += 180;
      d.by = Math.max(-89, Math.min(89, e.shiftKey ? Math.round(a / 45) * 45 : a));
      const s = -Math.tan(d.by * Math.PI / 180);
      m = [1, 0, s, 1, -s * d.cy, 0];
    } else {
      let dx = x - d.x, dy = y - d.y;
      if (e.shiftKey) { if (Math.abs(dx) > Math.abs(dy)) dy = 0; else dx = 0; }
      m = [1, 0, 0, 1, dx, dy];
    }
    d.matrix = m;
    p.ghost({ pages: [d.page], g: { kind: 'matrix', ids: d.ids, matrix: m } });
    return true;
  };
  const up = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    move(e); cancel();
    if (!d.moved) {
      setPivot({ page: d.page, x: d.x, y: d.y });
      // Alt+click with the Rotate, Scale or Shear tool: that point is the reference and the tool's window opens about it (InDesign); the engine
      // holds the window (next/shared/dialogs/rotateAbout.json, scaleAbout.json, shearAbout.json: the point in spread coordinates)
      const kind = p.tool === 'freeTransform' ? undefined : DIALOGS[p.tool], o = pageOf(p.info, d.page)?.spreadOrigin;
      if (e.altKey && kind && o) void window.folio.request({ cmd: 'openDialog', dialog: `${kind}About`, ids: d.ids, values: { point: [d.x + o[0], d.y + o[1]] } });
      return true;
    }
    if (d.mode === 'pivot' || d.matrix.every((v, i) => Math.abs(v - [1, 0, 0, 1, 0, 0][i]) < 1e-9)) return true;
    const o = pageOf(p.info, d.page)?.spreadOrigin;
    if (!o) return true;
    const point: [number, number] = [d.cx + o[0], d.cy + o[1]], ids = d.copy ? ['@copies'] : d.ids;
    const commands: EditCommand[] = d.copy ? [{ op: 'duplicate', ids: d.ids, by: [0, 0] }] : [];
    const labels = { rotate: tr('회전'), scale: tr('크기 조정'), shear: tr('기울이기'), move: tr('이동') };
    if (d.mode === 'rotate') commands.push({ op: 'rotateAbout', ids, by: d.by, point });
    if (d.mode === 'scale') commands.push({ op: 'scaleAbout', ids, sx: d.sx, sy: d.sy, point, ...scaleAdjust() });
    if (d.mode === 'shear') commands.push({ op: 'shearAbout', ids, by: d.by, point });
    if (d.mode === 'move') commands.push({ op: 'move', ids, by: [d.matrix[4], d.matrix[5]] });
    p.edit(tr(labels[d.mode]), commands);
    return true;
  };
  return { down, move, up, cancel, pivot };
}
