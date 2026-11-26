// View ▸ Extras ▸ Show Text Threads: for each selected text frame, InDesign draws a line from
// every frame's out port to the next frame's in port along its story, also between pages and
// spreads. Drawn over the whole document view (it scrolls with the pages), in the frames'
// layer colour; the ports sit where the selection overlay draws them (ui/overlay.tsx Ports).
import { useLayoutEffect, useState } from 'react';
import { corners, type ItemInfo, type ItemStore } from '../inspect';
import { findItem, type LayerLook } from './overlay';

type Sel = { page: number; id: string; content: boolean };
type Found = { item: ItemInfo; page: number };
type Line = { from: [number, number]; to: [number, number]; colour: string };

type Props = {
  view: HTMLElement | null;
  selection: Sel[];
  store: ItemStore;
  pages: number;
  scale: number;
  layers: LayerLook;
  /** Changes when pages are drawn again (edits), so the lines follow. */
  revision: unknown;
};

/** The frames of `id`'s story in thread order, as far as the loaded pages show them. */
function chain(id: string, find: (id: string) => Found | null): Found[] {
  let cur = find(id);
  const back = new Set<string>();
  while (cur?.item.text?.previous && !back.has(cur.item.id)) {
    back.add(cur.item.id);
    const p = find(cur.item.text.previous);
    if (!p) break;
    cur = p;
  }
  const out: Found[] = [];
  const seen = new Set<string>();
  while (cur && !seen.has(cur.item.id)) {
    seen.add(cur.item.id);
    out.push(cur);
    cur = cur.item.text?.next ? find(cur.item.text.next) : null;
  }
  return out;
}

export function ThreadLines({ view, selection, store, pages, scale, layers, revision }: Props) {
  const [lines, setLines] = useState<Line[]>([]);
  const [size, setSize] = useState<[number, number]>([0, 0]);
  useLayoutEffect(() => {
    if (!view) return;
    const update = () => {
      const find = (id: string): Found | null => {
        for (let p = 0; p < pages; p++) {
          const f = findItem(store.peek(p), id);
          if (f) return { item: f.item, page: p };
        }
        return null;
      };
      const v = view.getBoundingClientRect();
      // a page point in the view's scrolled content, in CSS pixels
      const at = (page: number, x: number, y: number): [number, number] | null => {
        const el = view.querySelector<HTMLElement>(`.page[data-page="${page}"]`);
        if (!el) return null;
        const r = el.getBoundingClientRect();
        return [r.left - v.left + view.scrollLeft + x, r.top - v.top + view.scrollTop + y];
      };
      // the ports as Ports draws them: in at the top left, out at the bottom right
      const port = (f: Found, out: boolean) => {
        const [tl, , br] = corners(f.item.transform, f.item.inner).map(([x, y]) => [x * scale, y * scale]);
        const dy = Math.min(18, (br[1] - tl[1]) / 3);
        return out ? at(f.page, br[0], br[1] - dy) : at(f.page, tl[0], tl[1] + dy);
      };
      const out: Line[] = [];
      const done = new Set<string>();
      for (const s of selection) {
        if (s.content) continue;
        const frames = chain(s.id, find);
        if (frames.length < 2 || frames.some((f) => done.has(f.item.id))) continue;
        frames.forEach((f) => done.add(f.item.id));
        for (let k = 0; k + 1 < frames.length; k++) {
          const a = port(frames[k], true), b = port(frames[k + 1], false);
          const c = frames[k].item.layer ? layers.get(frames[k].item.layer!)?.color : undefined;
          if (a && b) out.push({ from: a, to: b, colour: c ? `rgb(${c[0]},${c[1]},${c[2]})` : 'rgb(79,153,255)' });
        }
      }
      setLines(out);
      setSize([view.scrollWidth, view.scrollHeight]);
    };
    update();
    // pages load as they come into view: their frames may finish a thread
    const off = store.subscribe(update);
    const ro = new ResizeObserver(update);
    ro.observe(view);
    return () => { off(); ro.disconnect(); };
  }, [view, selection, store, pages, scale, layers, revision]);
  if (!lines.length) return null;
  return (
    <svg className="thread-lines" aria-hidden="true" width={size[0]} height={size[1]}
      style={{ position: 'absolute', left: 0, top: 0, pointerEvents: 'none', overflow: 'visible', zIndex: 3 }}>
      {lines.map((l, i) => <line key={i} x1={l.from[0]} y1={l.from[1]} x2={l.to[0]} y2={l.to[1]} stroke={l.colour} strokeWidth={1} />)}
    </svg>
  );
}
