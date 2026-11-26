// A tiny subscribable value for things that change on every mouse move (cursor position), so
// only the parts that show them re-render.
import { useSyncExternalStore } from 'react';

export class Live<T> {
  private listeners = new Set<() => void>();
  constructor(private value: T) {}
  get = () => this.value;
  set(v: T) {
    if (v === this.value) return;
    this.value = v;
    for (const fn of this.listeners) fn();
  }
  subscribe = (fn: () => void) => {
    this.listeners.add(fn);
    return () => { this.listeners.delete(fn); };
  };
}

export function useLive<T>(live: Live<T>): T {
  return useSyncExternalStore(live.subscribe, live.get, live.get);
}

/** Where the pointer is over the document: page index and points from its top-left. */
export type Cursor = { page: number; x: number; y: number; clientX: number; clientY: number } | null;
export const cursor = new Live<Cursor>(null);

/** Whether anchor points are selected (Direct Selection): the arrow keys and Delete then act on them, not on the shapes. */
export const pointSelection = new Live<boolean>(false);

/** Which anchor points are selected (Direct Selection): Object ▸ Convert Point works on them. */
export type SelectedPoints = { id: string; path: number; indices: number[] } | null;
export const selectedPoints = new Live<SelectedPoints>(null);

/** Select All Guides (Ctrl+Alt+G): bumped by the command, the document window then selects the guides of the spread with the current page. */
export const selectAllGuidesRequest = new Live<number>(0);

/** Measure tool line on a page (points). With the protractor (Alt-drag from an end of the line, tools-c1) `x2`,`y2` end a second line that starts
 *  at `x0`,`y0` too: the Info panel shows both lengths (D1, D2) and the angle between them. */
export type Measure = { page: number; x0: number; y0: number; x1: number; y1: number; x2?: number; y2?: number } | null;
export const measure = new Live<Measure>(null);
