import { useEffect, useSyncExternalStore } from 'react';
import type { DrawKind, EditCommand } from '../engine';
import type { Box } from '../inspect';

export type PolygonSettings = { sides: number; inset: number };
type State = { polygon: PolygonSettings; points: [number, number][]; aspect: number };
let state: State | null = null;
const listeners = new Set<() => void>();
const subscribe = (fn: () => void) => { listeners.add(fn); return () => { listeners.delete(fn); }; };
export async function drawingSettings(polygon?: PolygonSettings): Promise<State> {
  const { header } = await window.folio.request({ cmd: 'drawingSettings', ...(polygon ? { polygon } : {}) });
  if (!header.ok) throw Error(String(header.error));
  state = header as unknown as State;
  listeners.forEach(fn => fn());
  return state;
}
export function useDrawingSettings() {
  const value = useSyncExternalStore(subscribe, () => state);
  useEffect(() => { void drawingSettings().catch(() => {}); }, []);
  return value;
}
/** A drawn shape; the rect is read as the page is shown (a turned spread shows its pages turned), the engine stores it in the spread's own space as the mouse makes it. */
export async function drawingCommand(page: string, kind: DrawKind, rect: Box): Promise<EditCommand> {
  const star = kind === 'polygon' || kind === 'polygonFrame' ? (await drawingSettings()).polygon : { sides: 6, inset: 0 };
  return { op: 'createByDrag', page, kind, rect, layer: '', ...star };
}
