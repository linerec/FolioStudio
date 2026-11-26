// Ruler guides in the document window, as in InDesign: drag out of the top ruler for a
// horizontal guide or the left ruler for a vertical one; let go over a page for a guide on that
// page, over the pasteboard or with Ctrl/⌘ held for a guide across the spread. The selection
// tool selects a guide (where no object is) and drags it; Delete removes it. Locked guides and
// guides on hidden layers are left alone. The engine turns page positions into ruler locations.
// Shift+click adds a guide to the selection (or takes it out), a marquee over guides selects them, and dragging one of several
// selected guides moves them all. Dragging a guide, or one out of a ruler, snaps to guides, page edges and gridlines.
import { t as tr } from '../i18n';
import { useCallback, useEffect, useRef, useState } from 'react';
import type { EditCommand, EditResult } from '../engine';
import type { DocumentInfo, Guide, ItemStore } from '../inspect';
import { typing } from './commands';
import { commandKey } from './keys';
import { inDesignZoom, roundTo, rulerTickStep, snapPointer } from './dragunit';
import { rulerOffset, zeroOf } from './zeropoint';
import type { LayerLook } from './overlay';

type Orientation = Guide['orientation'];
type Here = { index: number; el: HTMLElement; x: number; y: number };
export type GuideHit = { guide: Guide; page: number };
export type GuideRef = { id: string; page: number };
/** The guide the menus act on: the one chosen last. */
export type GuideSelection = GuideRef | null;
type Drag = { orientation: Orientation; id?: string; page?: number; client: number; location?: number; group?: GuideRef[] };

type Options = {
  editable: boolean;
  shown: boolean;
  document: string;
  info: DocumentInfo | null;
  store: ItemStore;
  layers: LayerLook;
  scale: number;
  viewRef: React.RefObject<HTMLDivElement | null>;
  pageAt: (clientX: number, clientY: number) => Here | null;
  /** `value` of a guide along `axis` of `page`, pulled onto the snap targets (without the guides in `skip`), when Snap to Guides / Document Grid is on. */
  snapGuide?: (page: number, axis: 'x' | 'y', value: number, skip: string[]) => number;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
  clearSelection: () => void;
};

const visible = (g: Guide, layers: LayerLook) => {
  const l = g.layer ? layers.get(g.layer) : undefined;
  return !l || (l.visible && l.guides);
};

export function useGuides(o: Options) {
  const [chosen, setChosen] = useState<GuideRef[]>([]);
  const selected: GuideSelection = chosen[chosen.length - 1] ?? null;
  const chosenRef = useRef(chosen);
  chosenRef.current = chosen;
  const [drag, setDrag] = useState<Drag | null>(null);
  const live = useRef(o);
  live.current = o;
  useEffect(() => setChosen([]), [o.document, o.editable, o.shown]);

  const pageId = (index: number) => live.current.info?.pages.find((pg) => pg.index === index)?.id;

  /** The guide under a client point, among the pages of the spread there. */
  const hit = useCallback((clientX: number, clientY: number, includeLocked = false): GuideHit | null => {
    const { shown, pageAt, store, layers, scale, info } = live.current;
    const here = shown && (includeLocked || !info?.view?.guidesLocked) ? pageAt(clientX, clientY) : null;
    const row = here?.el.closest('.spread');
    if (!here || !row) return null;
    const tol = 3 / scale;
    for (const el of row.querySelectorAll<HTMLElement>('.page')) {
      const index = Number(el.dataset.page);
      const items = store.peek(index);
      if (!items) continue;
      const r = el.getBoundingClientRect();
      const k = r.width / items.size[0];
      const x = (clientX - r.left) / k, y = (clientY - r.top) / k;
      const [w, h] = items.size;
      for (const g of items.guides) {
        if (g.master || (!includeLocked && g.locked) || !visible(g, layers) || scale * 100 < g.viewThreshold) continue;
        const along = g.orientation === 'horizontal' ? x : y;
        const extent = g.orientation === 'horizontal' ? w : h;
        const across = g.orientation === 'horizontal' ? y : x;
        if (Math.abs(across - g.location) <= tol && (g.spread || (along >= 0 && along <= extent))) return { guide: g, page: index };
      }
    }
    return null;
  }, []);

  /** Follows the pointer until it is let go, then makes or moves the guide. */
  const follow = useCallback((e: React.PointerEvent, start: Drag, ruler: DOMRect | null) => {
    const target = e.currentTarget as Element;
    try { target.setPointerCapture(e.pointerId); } catch { /* synthetic */ }
    const at = (ev: PointerEvent | React.PointerEvent) => (start.orientation === 'horizontal' ? ev.clientY : ev.clientX);
    const down = at(e);
    const skip = (start.group ?? (start.id ? [{ id: start.id, page: start.page ?? 0 }] : [])).map((g) => g.id);
    /** Where the pointer puts the guide: on the page it is picked up from, or the page under the pointer; snapped when snapping is on. */
    const resolve = (ev: PointerEvent | React.PointerEvent) => {
      const { pageAt, store, viewRef, snapGuide } = live.current;
      const horizontal = start.orientation === 'horizontal';
      const here = start.page === undefined ? pageAt(ev.clientX, ev.clientY) : null;
      const index = start.page ?? here?.index;
      const el = start.page !== undefined ? viewRef.current?.querySelector<HTMLElement>(`[data-page="${start.page}"]`) : here?.el;
      const items = index === undefined ? null : store.peek(index);
      if (!el || !items || index === undefined) return { client: at(ev), value: null, index: undefined, raw: null as [number, number] | null };
      const r = el.getBoundingClientRect(), k = r.width / items.size[0];
      const raw: [number, number] = [(ev.clientX - r.left) / k, (ev.clientY - r.top) / k];
      let value = horizontal ? raw[1] : raw[0];
      // InDesign 2025: with Shift the guide is pulled onto the ruler's ticks (3 pt at 100 % and less, counted from the ruler's zero); the
      // zoom's drag unit rounds a plain drop (0.75 pt at 100 %; ui/dragunit.ts, rules/docinput-c3.ko.md)
      if (ev.shiftKey) {
        const { info, scale } = live.current;
        const [rx, ry] = rulerOffset(info?.rulerOrigin ?? 'SpreadOrigin', info?.pages.find((pg) => pg.index === index)?.spreadOffset, zeroOf(info));
        value = roundTo(value, rulerTickStep(inDesignZoom(scale)), -(horizontal ? ry : rx));
      } else {
        // a plain drop lands on InDesign's drag grid for this zoom (rules/tabledrag-c1.ko.md), then a guide or grid line near it catches it
        value = snapPointer(value, inDesignZoom(live.current.scale));
        if (snapGuide) value = snapGuide(index, horizontal ? 'y' : 'x', value, skip);
      }
      return { client: (horizontal ? r.top : r.left) + value * k, value, index, raw };
    };
    const pressed = resolve(e);
    setDrag({ ...start, client: down });
    const move = (ev: PointerEvent) => setDrag((d) => (d ? { ...d, client: resolve(ev).client } : d));
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', cancel);
      setDrag(null);
      const { store, edit } = live.current;
      const horizontal = start.orientation === 'horizontal';
      if (start.id !== undefined && start.page !== undefined) {
        if (Math.abs(at(ev) - down) < 1) return; // a click selects; it does not move
        // moved on the page it was picked up from, in that page's coordinates
        const page = pageId(start.page);
        const { value } = resolve(ev);
        if (!page || value === null) return;
        const group = start.group ?? [];
        if (group.length > 1 && start.location !== undefined) {
          // several guides go along: a vertical one by the pointer's way across, a horizontal one by its way down (the one picked up
          // is the snapped one); each moves along its own axis
          const raw = resolve(ev).raw, from = pressed.raw;
          const by = value - start.location;
          const delta: [number, number] = horizontal ? [raw && from ? raw[0] - from[0] : 0, by] : [by, raw && from ? raw[1] - from[1] : 0];
          if (delta[0] || delta[1]) void edit(tr('안내선 이동'), [{ op: 'moveGuidesBy', ids: group.map((g) => g.id), delta }]);
          return;
        }
        void edit(tr('안내선 이동'), [{ op: 'moveGuide', id: start.id, page, at: value }]);
        return;
      }
      // back on the ruler: nothing
      if (ruler && ev.clientX >= ruler.left && ev.clientX <= ruler.right && ev.clientY >= ruler.top && ev.clientY <= ruler.bottom) return;
      const where = resolve(ev);
      const items = where.index !== undefined && store.peek(where.index), page = where.index !== undefined && pageId(where.index);
      if (!where.raw || where.value === null || !items || !page) return;
      const [w, h] = items.size;
      const onPage = where.raw[0] >= 0 && where.raw[0] <= w && where.raw[1] >= 0 && where.raw[1] <= h;
      void edit(tr('안내선 만들기'), [{ op: 'newGuide', page, orientation: start.orientation, at: where.value, spread: !onPage || commandKey(ev) }]);
    };
    const cancel = () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', cancel);
      setDrag(null);
    };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', cancel);
  }, []);

  /** Pointer down on a ruler: the top one makes horizontal guides, the left one vertical. */
  const fromRuler = useCallback((e: React.PointerEvent<HTMLCanvasElement>, orientation: Orientation) => {
    if (e.button !== 0 || !live.current.editable) return;
    e.preventDefault();
    setChosen([]);
    follow(e, { orientation, client: 0 }, e.currentTarget.getBoundingClientRect());
  }, [follow]);

  /** A double click on a ruler makes a guide across the spread at that point of the ruler, perpendicular to it (the top ruler makes a vertical
   *  guide), not rounded (InDesign 2025). */
  const fromRulerDouble = useCallback((e: React.MouseEvent<HTMLCanvasElement>, ruler: Orientation) => {
    const { editable, pageAt, store, viewRef } = live.current;
    if (!editable) return;
    const view = viewRef.current?.getBoundingClientRect();
    if (!view) return;
    const vertical = ruler === 'horizontal';
    const here = pageAt(vertical ? e.clientX : view.left + view.width / 2, vertical ? view.top + view.height / 2 : e.clientY);
    const items = here && store.peek(here.index), page = here && pageId(here.index);
    if (!here || !items || !page) return;
    const at = vertical ? here.x : here.y;
    void live.current.edit(tr('안내선 만들기'), [{ op: 'newGuide', page, orientation: vertical ? 'vertical' : 'horizontal', at, spread: true }]);
  }, []);

  /** Selection tool on a guide: select it and drag it across. Shift adds it to the selection (or takes it out) and does not drag; a
   *  press on one of several selected guides drags them all. */
  const pick = useCallback((e: React.PointerEvent, h: GuideHit) => {
    const ref: GuideRef = { id: h.guide.id, page: h.page };
    const cur = chosenRef.current, has = cur.some((g) => g.id === ref.id);
    live.current.clearSelection();
    if (e.shiftKey) {
      setChosen(has ? cur.filter((g) => g.id !== ref.id) : [...cur, ref]);
      return;
    }
    const group = has ? [...cur.filter((g) => g.id !== ref.id), ref] : [ref];
    setChosen(group);
    if (live.current.editable) follow(e, { orientation: h.guide.orientation, id: h.guide.id, page: h.page, client: 0, location: h.guide.location, group }, null);
  }, [follow]);

  const deselect = useCallback(() => setChosen([]), []);
  const select = useCallback((h: GuideHit) => {
    setChosen([{ id: h.guide.id, page: h.page }]);
    live.current.clearSelection();
  }, []);
  /** A marquee over guides (page coordinates `rect` of `page`): the unlocked, visible ones it crosses are selected (added with `add`). */
  const selectIn = useCallback((page: number, rect: [number, number, number, number], add: boolean): boolean => {
    const { shown, store, layers, scale, info } = live.current;
    const items = shown && !info?.view?.guidesLocked ? store.peek(page) : null;
    if (!items) return false;
    const found: GuideRef[] = [];
    for (const g of items.guides) {
      if (g.master || g.locked || !visible(g, layers) || scale * 100 < g.viewThreshold) continue;
      const [a, b] = g.orientation === 'horizontal' ? [rect[1], rect[3]] : [rect[0], rect[2]];
      if (g.location >= a && g.location <= b) found.push({ id: g.id, page });
    }
    if (!found.length) return false;
    const cur = add ? chosenRef.current : [];
    setChosen([...cur, ...found.filter((f) => !cur.some((c) => c.id === f.id))]);
    live.current.clearSelection();
    return true;
  }, []);

  useEffect(() => {
    if (!chosen.length) return;
    const key = (e: KeyboardEvent) => {
      if (typing(e)) return;
      if (e.key === 'Escape') { setChosen([]); return; }
      if ((e.key === 'Delete' || e.key === 'Backspace') && live.current.editable) {
        e.preventDefault();
        e.stopImmediatePropagation();
        setChosen([]);
        void live.current.edit(tr('안내선 삭제'), chosen.map((g) => ({ op: 'deleteGuide' as const, id: g.id })));
      }
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [chosen]);

  /** The guide being dragged: a line across the document window. */
  const ghost = drag && drag.client ? (() => {
    const view = live.current.viewRef.current?.getBoundingClientRect();
    if (!view) return null;
    const style: React.CSSProperties = drag.orientation === 'horizontal'
      ? { position: 'fixed', left: view.left, width: view.width, top: drag.client, height: 0, borderTop: '1px solid #00d2ff', pointerEvents: 'none', zIndex: 9 }
      : { position: 'fixed', top: view.top, height: view.height, left: drag.client, width: 0, borderLeft: '1px solid #00d2ff', pointerEvents: 'none', zIndex: 9 };
    return <div className="guide-ghost" style={style} />;
  })() : null;

  /** Select All Guides (Ctrl+Alt+G): every guide of these pages that could be picked by hand (not a parent's, not locked, shown); false: none. */
  const selectAll = useCallback((pages: number[]): boolean => {
    const { shown, store, layers, scale, info } = live.current;
    if (!shown || info?.view?.guidesLocked) return false;
    const found: GuideRef[] = [];
    for (const page of pages) {
      for (const g of store.peek(page)?.guides ?? []) {
        if (g.master || g.locked || !visible(g, layers) || scale * 100 < g.viewThreshold || found.some((f) => f.id === g.id)) continue;
        found.push({ id: g.id, page });
      }
    }
    if (!found.length) return false;
    setChosen(found);
    live.current.clearSelection();
    return true;
  }, []);
  return { selected, selection: chosen, hit, pick, select, selectIn, selectAll, fromRuler, fromRulerDouble, deselect, ghost, dragging: !!drag };
}

export type Guides = ReturnType<typeof useGuides>;

/** View ▸ Grids & Guides ▸ Delete All Guides on Spread: the unlocked ruler guides of the spread
 *  `page` is on (pages not yet loaded are fetched). */
export async function spreadGuides(store: ItemStore, info: DocumentInfo | null, page: number): Promise<string[]> {
  const spread = info?.pages.find((pg) => pg.index === page)?.spreadIndex;
  const ids = new Set<string>();
  for (const pg of info?.pages ?? []) {
    if (pg.spreadIndex !== spread) continue;
    for (const g of (await store.load(pg.index))?.guides ?? []) if (!g.locked) ids.add(g.id);
  }
  return [...ids];
}
