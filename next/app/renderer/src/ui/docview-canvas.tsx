//! Renders and overlays the raster tiles of an individual page.

import { memo, useEffect, useRef, useState } from 'react';
import { type Bitmap, type DocumentSummary, type InkView, type PageInfo, RenderQueue } from '../engine';
import { type ItemStore, type PageItems } from '../inspect';
import type { Target } from '../print';
import { type Ghost, type LayerLook, PageOverlay, type SelEntry } from './overlay';
import { type SmartDraw } from './smartguides';
import { HiddenMarks } from './hiddenchars';
import { NoteMarks } from './notemarks';
import { PdfCommentMarks } from '../features/pdfcomments/marks';

export const queue = new RenderQueue(4);

type PageCanvasProps = {
  region: number[] | null;
  drawingOverlay?: React.ReactNode;
  /** The path the Pen tool is making, or the points of the selected shapes. */
  pathOverlay?: React.ReactNode;
  textOverlay?: React.ReactNode;
  page: PageInfo;
  scale: number;
  view: HTMLElement | null;
  ink: InkView | null;
  onPainted: (b: Bitmap) => void;
  highlight: Target | null;
  store: ItemStore;
  layers: LayerLook;
  side: 'left' | 'right' | 'single' | undefined;
  outerLeft: boolean;
  outerRight: boolean;
  bleed: DocumentSummary['bleed'] | null;
  /** The baseline and document grids to draw (View ▸ Grids & Guides). */
  grids: import('../inspect').ViewInfo | null;
  guides: boolean;
  edges: boolean;
  hiddenChars: boolean;
  showNotes: boolean;
  pdfComments: boolean;
  pdfCommentSelected: string | null;
  contentGrabber: boolean;
  selection: SelEntry[];
  hover: string | null;
  showContentGrabber: boolean;
  marks: import('../features/view/marks').ViewMarks | null;
  liveEdit: { id: string; corners: import('../inspect').Corner[] } | null;
  brackets: import('../features/tools/useTextPathBrackets').BracketView | null;
  linkBadges: import('./overlay').BadgeLinks | null;
  marquee: [number, number, number, number] | null;
  measure: number[] | null;
  pageSelected: boolean;
  ghost: Ghost | null;
  smartDraw: SmartDraw | null;
  smartColor: string;
  pivot: { x: number; y: number } | null;
  /** The selected ruler guide. */
  guide: string | null;
  /** Bumped when an edit changed the page: drawn again. */
  rev: number;
};

/** Two layers: the whole page at a capped resolution (instant while scrolling), and, when
 * zoomed past that cap, only the visible region at full resolution. Off-screen pages drop
 * their pixels and render again when they come back. */
export const PageCanvas = memo(function PageCanvas(p: PageCanvasProps) {
  const { page, scale, view, ink, onPainted } = p;
  const box = useRef<HTMLDivElement>(null);
  const base = useRef<HTMLCanvasElement>(null);
  const detail = useRef<HTMLCanvasElement>(null);
  const [visible, setVisible] = useState(false);
  const [items, setItems] = useState<PageItems | null | undefined>(() => p.store.peek(page.index));
  const baseShown = useRef('');
  const inkKey = ink ? JSON.stringify(ink) : '';
  const detailKey = useRef('');
  const dpr = window.devicePixelRatio || 1;
  const cssW = Math.round(page.width * scale);
  const cssH = Math.round(page.height * scale);
  const region = p.region;
  const regionKey = region?.join(',') ?? '';
  const [left, top, right, bottom] = region ?? [0, 0, page.width, page.height];
  const renderW = right - left, renderH = bottom - top;
  const full = scale * dpr;
  const cap = Math.min(full, Math.sqrt(6e6 / (renderW * renderH)));

  useEffect(() => {
    const el = box.current;
    if (!el || !view) return;
    const io = new IntersectionObserver(([e]) => setVisible(e.isIntersecting), { root: view, rootMargin: '800px' });
    io.observe(el);
    return () => io.disconnect();
  }, [view]);

  // page items for the overlay, once visible
  useEffect(() => {
    if (!visible) return;
    const store = p.store;
    const sync = () => setItems(store.peek(page.index));
    const off = store.subscribe(sync);
    if (store.supported && store.peek(page.index) === undefined) store.load(page.index);
    sync();
    return off;
  }, [visible, p.store, page.index]);

  useEffect(() => {
    const c = base.current;
    if (!c) return;
    if (!visible) { c.width = 0; c.height = 0; baseShown.current = ''; return; }
    const target = Math.max(0.125, Math.round(cap * 8) / 8);
    const key = `${target}|${inkKey}|${p.rev}|${regionKey}`;
    if (baseShown.current === key) return;
    let live = true;
    (region ? queue.renderRegion(page.index, target, region, 0, ink) : queue.render(page.index, target, 0, ink))
      .then((b) => { if (!live) return; draw(c, b); baseShown.current = key; onPainted(b); })
      .catch((e) => { if (String(e) !== 'Error: cancelled') console.error('render failed', page.index, String(e)); });
    return () => { live = false; };
    // `ink` changes exactly when `inkKey` does
  }, [visible, cap, page.index, onPainted, inkKey, p.rev, regionKey]);

  useEffect(() => {
    const c = detail.current;
    if (!c) return;
    const clear = () => { c.width = 0; c.height = 0; c.style.display = 'none'; detailKey.current = ''; };
    if (!visible || !view || full <= cap * 1.02) { clear(); return; }
    let timer = 0;
    const update = () => {
      const el = box.current;
      if (!el) return;
      const pr = el.getBoundingClientRect();
      const vr = view.getBoundingClientRect();
      const padX = vr.width * 0.5, padY = vr.height * 0.5;
      const l = Math.max(pr.left + left * scale, vr.left - padX), r = Math.min(pr.left + right * scale, vr.right + padX);
      const t = Math.max(pr.top + top * scale, vr.top - padY), btm = Math.min(pr.top + bottom * scale, vr.bottom + padY);
      if (r <= l || btm <= t) return;
      const snap = 16;
      const region = [
        Math.max(left, Math.floor((l - pr.left) / scale / snap) * snap),
        Math.max(top, Math.floor((t - pr.top) / scale / snap) * snap),
        Math.min(right, Math.ceil((r - pr.left) / scale / snap) * snap),
        Math.min(bottom, Math.ceil((btm - pr.top) / scale / snap) * snap),
      ];
      const key = `${full.toFixed(3)}:${region.join(',')}|${inkKey}|${p.rev}`;
      if (key === detailKey.current) return;
      detailKey.current = key;
      queue
        .renderRegion(page.index, full, region, 0, ink)
        .then((b) => {
          if (detailKey.current !== key) return;
          draw(c, b);
          c.style.display = 'block';
          c.style.left = `${region[0] * scale}px`;
          c.style.top = `${region[1] * scale}px`;
          c.style.width = `${(region[2] - region[0]) * scale}px`;
          c.style.height = `${(region[3] - region[1]) * scale}px`;
          onPainted(b);
        })
        .catch((e) => { if (String(e) !== 'Error: cancelled') console.error('render failed', page.index, String(e)); });
    };
    const onScroll = () => { window.clearTimeout(timer); timer = window.setTimeout(update, 90); };
    update();
    view.addEventListener('scroll', onScroll, { passive: true });
    return () => { window.clearTimeout(timer); view.removeEventListener('scroll', onScroll); };
  }, [visible, view, full, cap, scale, page.index, page.width, page.height, onPainted, inkKey, p.rev, regionKey]);

  return (
    <div className="page" ref={box} data-page={page.index} data-region={regionKey} style={{ width: cssW, height: cssH, margin: `${-top * scale}px ${(right - page.width) * scale}px ${(bottom - page.height) * scale}px ${-left * scale}px` }}>
      <canvas ref={base} style={{ position: 'absolute', left: left * scale, top: top * scale, width: renderW * scale, height: renderH * scale }} />
      <canvas ref={detail} className="detail" />
      {visible && p.showNotes && <NoteMarks page={page.index} width={page.width} height={page.height} rev={p.rev} />}
      {visible && p.pdfComments && <PdfCommentMarks page={page.index} width={page.width} height={page.height} scale={scale} rev={p.rev} selected={p.pdfCommentSelected} />}
      {visible && p.hiddenChars && <HiddenMarks page={page.index} width={page.width} height={page.height} rev={p.rev} items={items} layers={p.layers} />}
      {p.textOverlay}
      {p.drawingOverlay}
      {p.pathOverlay}
      {visible && (
        <PageOverlay
          contentGrabber={p.contentGrabber}
          items={items}
          scale={scale}
          layers={p.layers}
          side={p.side}
          bleed={p.bleed ?? null}
          grids={p.grids}
          guides={p.guides}
          edges={p.edges}
          selection={p.selection}
          page={page.index}
          hover={p.hover}
          showContentGrabber={p.showContentGrabber}
          marks={p.marks}
          liveEdit={p.liveEdit}
          brackets={p.brackets}
          linkBadges={p.linkBadges}
          marquee={p.marquee}
          measure={p.measure}
          pageSelected={p.pageSelected}
          ghost={p.ghost}
          smartDraw={p.smartDraw}
          smartColor={p.smartColor}
          pivot={p.pivot}
          guide={p.guide}
          outer={{ left: p.outerLeft, right: p.outerRight }}
          size={[page.width, page.height]}
        />
      )}
      {p.highlight && (
        <div key={JSON.stringify(p.highlight.bounds)} className={p.highlight.bounds ? 'mark' : 'mark page-mark'} style={p.highlight.bounds ? markStyle(p.highlight.bounds, scale) : undefined} />
      )}
    </div>
  );
});

function draw(c: HTMLCanvasElement, b: Bitmap) {
  c.width = b.width;
  c.height = b.height;
  c.getContext('2d')!.putImageData(new ImageData(b.pixels, b.width, b.height), 0, 0);
}

function markStyle(b: number[], scale: number): React.CSSProperties {
  const pad = 3;
  return { left: b[0] * scale - pad, top: b[1] * scale - pad, width: Math.max(0, (b[2] - b[0]) * scale) + 2 * pad, height: Math.max(0, (b[3] - b[1]) * scale) + 2 * pad };
}
