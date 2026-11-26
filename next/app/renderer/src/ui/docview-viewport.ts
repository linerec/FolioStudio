//! Lays out pages, tracks scrolling and applies zoom around the pointer.

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { type PageInfo } from '../engine';
import type { Target } from '../print';
import { commandKey } from './keys';
import type { DocWindowProps } from './docview';

/** `fit`: the page (or spread) shown whole in the window, or `width`: the page's width fills the window (a page destination's Fit Width). */
export type Zoom = { kind: 'fit'; what: 'page' | 'spread' | 'width'; page: number } | { kind: 'value'; value: number };

export const PAD_X = 80, PAD_Y = 48, GAP = 44;
export const ZOOM_STOPS = [0.05, 0.0833, 0.125, 0.25, 0.33, 0.5, 0.66, 0.75, 1, 1.25, 1.5, 2, 3, 4, 6, 8, 12, 16, 24, 32, 40];

export function groupRows(allPages: PageInfo[], spreads: boolean, hiddenSpreads?: Record<string, boolean> | null): PageInfo[][] {
  // View ▸ Show Hidden Spreads off: a hidden spread is not in the layout (unless that would leave nothing to show)
  const shown = hiddenSpreads ? allPages.filter((p) => !hiddenSpreads[p.spread]) : allPages;
  const pages = shown.length ? shown : allPages;
  if (!spreads) return pages.map((p) => [p]);
  const rows: PageInfo[][] = [];
  for (const p of pages) {
    const last = rows[rows.length - 1];
    if (last && last[0].spread === p.spread) last.push(p);
    else rows.push([p]);
  }
  return rows;
}

export function stepZoom(z: number, dir: 1 | -1) {
  if (dir > 0) return ZOOM_STOPS.find((s) => s > z + 1e-3) ?? z;
  return [...ZOOM_STOPS].reverse().find((s) => s < z - 1e-3) ?? z;
}

/** Zoom, fit and scrolling of the document view. */
export function useViewport(rows: PageInfo[][]) {
  const viewRef = useRef<HTMLDivElement | null>(null);
  const [viewEl, setViewEl] = useState<HTMLDivElement | null>(null);
  const [zoom, setZoom] = useState<Zoom>({ kind: 'fit', what: 'spread', page: 0 });
  const [box, setBox] = useState({ w: 900, h: 700 });
  // zooming keeps the same point of a page under the cursor (or the view centre)
  const anchor = useRef<{ page: number; fx: number; fy: number; cx: number; cy: number } | null>(null);
  const pendingFit = useRef<number | null>(null);

  useEffect(() => {
    const el = viewEl;
    if (!el) return;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [viewEl]);

  const scale = useMemo(() => {
    if (zoom.kind === 'value') return zoom.value;
    const row = rows.find((r) => r.some((p) => p.index === zoom.page)) ?? rows[0];
    if (!row) return 1;
    const pages = zoom.what === 'spread' ? row : row.filter((p) => p.index === zoom.page);
    const w = pages.reduce((s, p) => s + p.width, 0);
    const h = Math.max(...pages.map((p) => p.height));
    if (zoom.what === 'width') return Math.max(0.03, (box.w - 2 * PAD_X) / w);
    return Math.max(0.03, Math.min((box.w - 2 * PAD_X) / w, (box.h - 2 * PAD_Y - 20) / h));
  }, [zoom, rows, box]);

  // the view before the last zoom change (its zoom and the page point at the middle of the window): View keys ▸ Toggle Between Current and Previous Views
  const zoomNow = useRef(zoom); zoomNow.current = zoom;
  const previous = useRef<{ zoom: Zoom; at: { page: number; fx: number; fy: number } | null } | null>(null);
  const middle = (): { page: number; fx: number; fy: number } | null => {
    const view = viewRef.current;
    if (!view) return null;
    const vr = view.getBoundingClientRect(), cx = vr.left + view.clientWidth / 2, cy = vr.top + view.clientHeight / 2;
    let best: { page: number; fx: number; fy: number } | null = null, bestD = Infinity;
    view.querySelectorAll<HTMLElement>('.page').forEach((el) => {
      const r = el.getBoundingClientRect();
      const d = Math.hypot(Math.max(r.left - cx, 0, cx - r.right), Math.max(r.top - cy, 0, cy - r.bottom));
      if (d < bestD && r.width && r.height) { bestD = d; best = { page: Number(el.dataset.page), fx: (cx - r.left) / r.width, fy: (cy - r.top) / r.height }; }
    });
    return best;
  };
  const remember = () => { previous.current = { zoom: zoomNow.current, at: middle() }; };
  /** Back to the view before the last zoom change, and again to this one (InDesign: Ctrl+Alt+2). false: there was no change yet. */
  const togglePrevious = useCallback((): boolean => {
    const back = previous.current, view = viewRef.current;
    if (!back || !view) return false;
    previous.current = { zoom: zoomNow.current, at: middle() };
    const vr = view.getBoundingClientRect();
    anchor.current = back.at ? { ...back.at, cx: vr.left + view.clientWidth / 2, cy: vr.top + view.clientHeight / 2 } : null;
    pendingFit.current = null;
    setZoom(back.zoom);
    return true;
  }, []);

  const zoomTo = useCallback(
    (to: number, at?: { clientX: number; clientY: number }) => {
      const view = viewRef.current;
      const v = Math.min(40, Math.max(0.03, to));
      remember();
      if (view) {
        const vr = view.getBoundingClientRect();
        const cx = at ? at.clientX : vr.left + view.clientWidth / 2;
        const cy = at ? at.clientY : vr.top + view.clientHeight / 2;
        let best: { el: HTMLElement; r: DOMRect } | null = null, bestD = Infinity;
        view.querySelectorAll<HTMLElement>('.page').forEach((el) => {
          const r = el.getBoundingClientRect();
          const dx = cx < r.left ? r.left - cx : cx > r.right ? cx - r.right : 0;
          const dy = cy < r.top ? r.top - cy : cy > r.bottom ? cy - r.bottom : 0;
          if (dx + dy * 4 < bestD) { bestD = dx + dy * 4; best = { el, r }; }
        });
        if (best) {
          const b = best as { el: HTMLElement; r: DOMRect };
          anchor.current = { page: Number(b.el.dataset.page), fx: (cx - b.r.left) / b.r.width, fy: (cy - b.r.top) / b.r.height, cx, cy };
        }
      }
      setZoom({ kind: 'value', value: v });
    },
    [],
  );

  const fit = useCallback((what: 'page' | 'spread' | 'width', page: number) => {
    remember();
    pendingFit.current = page;
    anchor.current = null;
    setZoom({ kind: 'fit', what, page });
  }, []);

  useLayoutEffect(() => {
    const a = anchor.current, view = viewRef.current;
    if (a && view) {
      anchor.current = null;
      const el = view.querySelector<HTMLElement>(`[data-page="${a.page}"]`);
      if (el) {
        const r = el.getBoundingClientRect();
        view.scrollLeft += r.left + a.fx * r.width - a.cx;
        view.scrollTop += r.top + a.fy * r.height - a.cy;
      }
    }
    if (pendingFit.current !== null && view) {
      const page = pendingFit.current;
      pendingFit.current = null;
      const el = view.querySelector<HTMLElement>(zoom.kind === 'fit' && zoom.what === 'spread' ? `[data-row-of="${page}"]` : `[data-page="${page}"]`);
      el?.scrollIntoView({ block: zoom.kind === 'fit' && zoom.what === 'width' ? 'start' : 'center', inline: 'center' });
    }
  }, [scale, zoom]);

  // going to a page jumps, as InDesign does: it is keyboard work (Page Down, Go To Page, first/last page) done many times a day, and a
  // smooth scroll across a 100-page issue reads as delay (uix-b4, remove-animations). Smooth stays only for a caller that asks for it.
  const scrollToPage = useCallback((index: number, smooth = false) => {
    viewRef.current?.querySelector(`[data-page="${index}"]`)?.scrollIntoView({ block: 'center', inline: 'center', behavior: smooth ? 'smooth' : 'auto' });
  }, []);

  const scrollToTarget = useCallback(
    (t: Target) => {
      const view = viewRef.current;
      const el = view?.querySelector<HTMLElement>(`[data-page="${t.page}"]`);
      if (!view || !el) return;
      // a jump, then the "go to" mark on the page settles once (styles.css .mark): the mark is the signal, not the scroll
      if (!t.bounds) { el.scrollIntoView({ block: 'start' }); return; }
      const vr = view.getBoundingClientRect();
      const pr = el.getBoundingClientRect();
      const cx = pr.left - vr.left + view.scrollLeft + ((t.bounds[0] + t.bounds[2]) / 2) * scale;
      const cy = pr.top - vr.top + view.scrollTop + ((t.bounds[1] + t.bounds[3]) / 2) * scale;
      view.scrollTo({ left: cx - view.clientWidth / 2, top: cy - view.clientHeight / 2 });
    },
    [scale],
  );

  const setView = useCallback((el: HTMLDivElement | null) => { viewRef.current = el; setViewEl(el); }, []);
  return { viewRef, viewEl, setView, zoom, scale, zoomTo, fit, scrollToPage, scrollToTarget, setZoom, togglePrevious };
}

export type Viewport = ReturnType<typeof useViewport>;

export function useViewportInput(p: Pick<DocWindowProps, 'vp' | 'doc' | 'current' | 'setCurrent'>) {
  const { vp, doc } = p;
  const scale = vp.scale;
  // current page from scrolling (the page nearest the view's centre)
  const currentRef = useRef(p.current);
  currentRef.current = p.current;
  useEffect(() => {
    const view = vp.viewEl;
    if (!view) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      // a spread just hidden is not laid out (View > Show Hidden Spreads off): its page stays the current one, so Show Spread acts on it again
      const cur = currentRef.current;
      if (doc.pageSettings?.hidden?.[doc.pages.find((q) => q.index === cur)?.spread ?? ''] && !view.querySelector(`.page[data-page="${cur}"]`)) return;
      const vr = view.getBoundingClientRect();
      const mid = vr.top + vr.height / 2, midX = vr.left + vr.width / 2;
      let best = 0, bestDist = Infinity;
      view.querySelectorAll<HTMLElement>('.page').forEach((el) => {
        const r = el.getBoundingClientRect();
        const dy = mid < r.top ? r.top - mid : mid > r.bottom ? mid - r.bottom : 0;
        const dx = midX < r.left ? r.left - midX : midX > r.right ? midX - r.right : 0;
        const d = dy * 4 + dx;
        if (d < bestDist) { bestDist = d; best = Number(el.dataset.page); }
      });
      p.setCurrent(best);
    };
    const onScroll = () => { if (!frame) frame = requestAnimationFrame(update); };
    view.addEventListener('scroll', onScroll, { passive: true });
    update();
    return () => { view.removeEventListener('scroll', onScroll); if (frame) cancelAnimationFrame(frame); };
  }, [vp.viewEl, doc, p.setCurrent, scale]);

  // Ctrl+wheel and Alt+wheel zoom around the cursor
  useEffect(() => {
    const view = vp.viewEl;
    if (!view) return;
    const onWheel = (e: WheelEvent) => {
      if (!commandKey(e) && !e.ctrlKey && !e.altKey) return;
      e.preventDefault();
      vp.zoomTo(scale * (e.deltaY < 0 ? 1.12 : 1 / 1.12), e);
    };
    view.addEventListener('wheel', onWheel, { passive: false });
    return () => view.removeEventListener('wheel', onWheel);
  }, [vp.viewEl, vp.zoomTo, scale]);
}
