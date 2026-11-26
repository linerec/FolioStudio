// Threading text frames by hand, as in InDesign: click a frame's out port (or in port) to load
// its text, then click another text frame to thread to it, or click/drag in empty space to make
// a new frame there. Double-clicking a port breaks the thread at that port. Esc unloads.
import { t as tr } from '../i18n';
import { useCallback, useEffect, useState } from 'react';
import type { EditCommand, EditResult } from '../engine';
import type { ItemInfo, PageItems } from '../inspect';

export type Port = 'in' | 'out';
export type Loaded = { from: string; port: Port };
export type Threading = ReturnType<typeof useThreading>;

export function useThreading(document: string | undefined, editable: boolean,
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>) {
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const cancel = useCallback(() => setLoaded(null), []);
  useEffect(cancel, [document, editable, cancel]);
  useEffect(() => {
    if (!loaded) return;
    const key = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { cancel(); e.preventDefault(); e.stopImmediatePropagation(); }
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, [loaded, cancel]);

  const load = useCallback((from: string, port: Port) => { if (editable) setLoaded({ from, port }); }, [editable]);

  /** Threads the loaded frame to `target` (an existing text frame). */
  const toFrame = useCallback(async (target: string) => {
    if (!loaded || target === loaded.from) return null;
    const [id, next] = loaded.port === 'out' ? [loaded.from, target] : [target, loaded.from];
    setLoaded(null);
    return edit(tr('텍스트 프레임 잇기'), [{ op: 'threadText', id, next }]);
  }, [loaded, edit]);

  /** Makes a new text frame at `rect` (page points) and threads the loaded text into it. */
  const toNewFrame = useCallback(async (page: string, rect: [number, number, number, number]) => {
    if (!loaded) return null;
    if (loaded.port === 'in') return null; // a new frame before the thread: not built yet
    const from = loaded.from;
    setLoaded(null);
    return edit(tr('텍스트 프레임 잇기'), [{ op: 'createThreadFrame', id: from, page, rect }]);
  }, [loaded, edit]);

  /** Breaks the thread at a port of `item`. */
  const unthread = useCallback(async (item: ItemInfo, port: Port) => {
    const id = port === 'out' ? item.id : item.text?.previous;
    if (!editable || !id || (port === 'out' && !item.text?.next)) return null;
    return edit(tr('텍스트 프레임 잇기 끊기'), [{ op: 'threadText', id, next: null }]);
  }, [editable, edit]);

  return { loaded, load, cancel, toFrame, toNewFrame, unthread };
}

/**
 * The frame InDesign makes for a click with loaded text: the width of the column under the
 * click, from the click down to the bottom margin.
 */
export function columnFrame(page: PageItems, x: number, y: number): [number, number, number, number] {
  const [, left, bottom] = page.margins;
  const pos = page.columns.positions;
  let x0 = left, x1 = page.size[0] - page.margins[3];
  if (pos && pos.length >= 2) {
    let best = 0, dist = Infinity;
    for (let i = 0; i + 1 < pos.length; i += 2) {
      const d = x < pos[i] ? pos[i] - x : x > pos[i + 1] ? x - pos[i + 1] : 0;
      if (d < dist) { dist = d; best = i; }
    }
    [x0, x1] = [pos[best], pos[best + 1]];
  }
  const y1 = Math.max(page.size[1] - bottom, y + 12);
  return [x0, y, x1, y1];
}
