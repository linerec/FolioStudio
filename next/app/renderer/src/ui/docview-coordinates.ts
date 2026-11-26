//! Maps pointer positions and movement to pages, spreads and items.

import { useCallback } from 'react';
import { pageOf, unionBox } from '../inspect';
import { type Hit, findItem, hitTest } from './overlay';
import { ownDelta, ownPoint, shownPoint } from './turned';
import type { DocWindowProps } from './docview';

export function usePageCoordinates(p: DocWindowProps, pxPerPt: (el: HTMLElement, r: DOMRect) => number) {
  const { vp, doc } = p;
  const scale = vp.scale;
  /** The page under (or nearest to) a client point, with the point in its coordinates. */
  const pageAt = useCallback(
    (clientX: number, clientY: number) => {
      const view = vp.viewRef.current;
      if (!view) return null;
      let best: { index: number; el: HTMLElement; r: DOMRect } | null = null, bestD = Infinity;
      view.querySelectorAll<HTMLElement>('.page').forEach((el) => {
        const r = el.getBoundingClientRect();
        if (r.bottom < -2000 || r.top > window.innerHeight + 2000) return;
        const dx = clientX < r.left ? r.left - clientX : clientX > r.right ? clientX - r.right : 0;
        const dy = clientY < r.top ? r.top - clientY : clientY > r.bottom ? clientY - r.bottom : 0;
        const d = dy * 4 + dx;
        if (d < bestD) { bestD = d; best = { index: Number(el.dataset.page), el, r }; }
      });
      if (!best) return null;
      const b = best as { index: number; el: HTMLElement; r: DOMRect };
      const k = pxPerPt(b.el, b.r);
      return { index: b.index, el: b.el, x: (clientX - b.r.left) / k, y: (clientY - b.r.top) / k };
    },
    [vp.viewRef, pxPerPt],
  );

  /**
   * Where a dragged selection goes when it is let go over another spread (its pages or the pasteboard around them): the spread whose page the middle of the
   * dragged objects is nearest to owns them, so the pasteboard between two spreads belongs to the nearer one (a selection on several pages of a spread is
   * judged by the pointer instead). `by` is the move in the destination spread's own coordinates, so the objects stay where they were shown. null: they stay
   * on their spread; 'mixed': the selection is on more than one spread. InDesign's own boundary rule on the pasteboard could not be measured by script
   * (oracle README, spread-move); this is the nearest-spread rule (screens-doc STATUS). Spreads turned by View ▸ Rotate Spread, as the source or the
   * destination: the middle of the objects is put where the screen shows it (own space → shown → screen on the source, screen → shown → own space on the
   * destination); the objects keep their shape in the spread's own space (moveToSpread keeps the spread-local transform), so between spreads of
   * different turns they turn with the destination as it is shown.
   */
  const spreadDrop = (d: { start: { page: number; x: number; y: number }; ids: string[]; pages: number[]; by: [number, number] }, clientX: number, clientY: number):
    { spread: string; index: number; by: [number, number] } | 'mixed' | null => {
    const source = doc.pages.find(pg => pg.index === d.start.page);
    if (!source) return null;
    if (d.pages.some(page => doc.pages.find(pg => pg.index === page)?.spread !== source.spread)) return 'mixed';
    const view = vp.viewRef.current;
    const el = view?.querySelector<HTMLElement>(`[data-page="${d.start.page}"]`);
    if (!view || !el) return null;
    let at: { x: number; y: number } = { x: clientX, y: clientY };
    let from: [number, number] = [d.start.x, d.start.y];
    if (d.pages.length === 1) {
      const items = p.store.peek(d.start.page);
      const found = items && d.ids.map(id => findItem(items, id));
      const b = found && found.every(f => f) ? unionBox(found.map(f => f!.item.bounds)) : null;
      if (b) {
        const r = el.getBoundingClientRect(), k = pxPerPt(el, r);
        const centre: [number, number] = [(b[0] + b[2]) / 2, (b[1] + b[3]) / 2];
        from = centre;
        // where the middle is shown now (the move is in the page's own space; a turned source shows that space turned)
        const [sx, sy] = shownPoint(turnOf(d.start.page), source.width, source.height, [centre[0] + d.by[0], centre[1] + d.by[1]]);
        at = { x: r.left + sx * k, y: r.top + sy * k };
      }
    }
    const drop = pageAt(at.x, at.y);
    const target = drop && doc.pages.find(pg => pg.index === drop.index);
    if (!drop || !target || target.spread === source.spread) return null;
    const fromOrigin = pageOf(p.info, d.start.page)?.spreadOrigin, toOrigin = pageOf(p.info, drop.index)?.spreadOrigin;
    if (!fromOrigin || !toOrigin) return null;
    // a turned destination is read in its own space
    const [dropX, dropY] = ownOf(drop.index, [drop.x, drop.y]);
    return { spread: target.spread, index: drop.index, by: [dropX + toOrigin[0] - from[0] - fromOrigin[0], dropY + toOrigin[1] - from[1] - fromOrigin[1]] };
  };

  /** A point of a page as shown (from the top-left of the page as it is drawn) in the page's own space: the same on an unturned page; on a spread
   *  turned by View ▸ Rotate Spread the quarter turn is taken back (ui/turned.ts). */
  const ownOf = useCallback((index: number, pt: [number, number]): [number, number] => {
    const pg = doc.pages.find((q) => q.index === index);
    const turn = pg ? doc.pageSettings?.rotation?.[pg.spread] : undefined;
    return pg && turn ? ownPoint(turn, pg.width, pg.height, pt) : pt;
  }, [doc]);

  /** Top-most item under the point among the pages of its spread (`own`: the pages of a turned spread are read in their own space, as the
   *  Pen, Pencil and point tools read them). */
  const hitAt = useCallback(
    (clientX: number, clientY: number, strokeOnly = false, tolPx = 4, own = false): Hit | null => {
      const here = pageAt(clientX, clientY);
      if (!here) return null;
      const row = here.el.closest('.spread');
      const pages = row ? [...row.querySelectorAll<HTMLElement>('.page')] : [here.el];
      let best: Hit | null = null;
      const tol = tolPx / scale;
      for (const el of pages) {
        const idx = Number(el.dataset.page);
        const items = p.store.peek(idx);
        if (!items) continue;
        const r = el.getBoundingClientRect();
        const k = pxPerPt(el, r);
        const shown: [number, number] = [(clientX - r.left) / k, (clientY - r.top) / k];
        const h = hitTest(items, idx, own ? ownOf(idx, shown) : shown, tol, p.layers, false, strokeOnly);
        if (h && (!best || h.top.z > best.top.z)) best = h;
      }
      return best;
    },
    [pageAt, p.store, p.layers, scale, pxPerPt, ownOf],
  );

  /** The page place under a client point for the Pen, Pencil and point tools: in the page's own space, also on a turned spread (their drawing
   *  goes into the page as it lies, and their overlay is drawn in the page, turned with it). */
  const ownAt = useCallback((clientX: number, clientY: number) => {
    const h = pageAt(clientX, clientY);
    if (!h) return null;
    const [x, y] = ownOf(h.index, [h.x, h.y]);
    return { ...h, x, y };
  }, [pageAt, ownOf]);

  /** The quarter turn (View ▸ Rotate Spread) of the spread a page is on. */
  const turnOf = useCallback((index: number) => {
    const pg = doc.pages.find((q) => q.index === index);
    return pg ? doc.pageSettings?.rotation?.[pg.spread] : undefined;
  }, [doc]);
  /** A client point read on the page `index` (also off its edge), in the page's own space: what every drag that started on a page reads its pointer with. */
  const pointOn = useCallback((index: number, clientX: number, clientY: number): [number, number] | null => {
    const el = vp.viewRef.current?.querySelector<HTMLElement>(`[data-page="${index}"]`);
    if (!el) return null;
    const r = el.getBoundingClientRect(), k = pxPerPt(el, r);
    return ownOf(index, [(clientX - r.left) / k, (clientY - r.top) / k]);
  }, [vp.viewRef, pxPerPt, ownOf]);
  /** How far the pointer went (client pixels) as a move of the objects of the page `index` (page points in its own space: on a turned spread they
   *  go the way the pointer went on the screen). */
  const moveOn = (index: number, cdx: number, cdy: number): [number, number] => ownDelta(turnOf(index), [cdx / scale, cdy / scale]);
  return { pageAt, spreadDrop, ownOf, hitAt, ownAt, turnOf, pointOn, moveOn };
}

export type PageCoordinates = ReturnType<typeof usePageCoordinates>;
