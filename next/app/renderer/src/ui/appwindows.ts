// Window > Arrange across documents: the app windows (one per document) that sit together as tabs of one window, as tiles of one frame,
// or float. The state lives in the shell (electron/appwin.cjs, a pure model) because it is about windows of the computer, not about a
// document; this file is the window's side: it hears the state, tells the shell this window's tab label and unsaved mark, and makes the
// Window > Arrange menu items act on documents when more than one is open. A command is the same value a tool would send: every
// item here is `window.folio.appWindows({ cmd, ...args })` with a structured answer.
import { useCallback, useEffect, useMemo, useState } from 'react';
import type { AppWinReply, AppWinView } from '../engine';
import type { Command } from './commands';
import { ARRANGEMENTS, fits } from './winlayout';

export type AppWinCommand = { cmd: string; [arg: string]: unknown };

/** The tab strip shows when this window is one of several tabs; in a tiling or floating a window shows only its own title. */
export const showsTabs = (v: AppWinView | null): v is AppWinView => !!v && v.mode === 'tabs' && v.tabs.length > 1;

/**
 * The place a tab dragged to horizontal position `x` takes among the other tabs (their centres, left to right, without the dragged one).
 */
export function dropIndex(centres: number[], x: number): number {
  let i = 0;
  while (i < centres.length && x > centres[i]) i++;
  return i;
}

/** Whether a drag that ended at (`x`, `y`) left the strip `strip` far enough to float the window (a tab pulled off the strip). */
export function pulledOff(strip: { left: number; right: number; top: number; bottom: number }, x: number, y: number, slack = 24): boolean {
  return y < strip.top - slack || y > strip.bottom + slack || x < strip.left - slack || x > strip.right + slack;
}

type Handlers = Record<string, Partial<Command>>;

/**
 * The Window > Arrange items. With one document they act on that document's windows (ui/windows.ts, `inDoc`). With more than one they
 * act on the documents' windows: Tile and the N-up items put every document side by side, Cascade and Float All float them, Float in
 * Window pulls this one out of its tabs, Consolidate All Windows gathers them as tabs (and docks this document's floating windows too).
 * New Window, Split Window and Minimize stay as they are.
 */
export function composeArrange(inDoc: Handlers, v: AppWinView | null, apply: (c: AppWinCommand) => Promise<AppWinReply>): Handlers {
  const docs = v?.docs ?? 1;
  if (!v || docs < 2) return inDoc;
  const go = (c: AppWinCommand) => () => { void apply(c); };
  const out: Handlers = { ...inDoc };
  // Next / Previous Window with one window of this document: the next app window (by id, round) is shown and active (tools-c1)
  const turn = (step: 1 | -1) => async () => {
    const r = await apply({ cmd: 'state' }) as AppWinReply & { state?: { docs?: Record<string, unknown> } };
    const ids = Object.keys(r.state?.docs ?? {}).map(Number).sort((a, b) => a - b), i = ids.indexOf(v.doc);
    if (ids.length > 1 && i >= 0) await apply({ cmd: 'activate', doc: ids[(i + step + ids.length) % ids.length] });
  };
  if (!inDoc['window.key.next']?.run) out['window.key.next'] = { run: () => void turn(1)() };
  if (!inDoc['window.key.previous']?.run) out['window.key.previous'] = { run: () => void turn(-1)() };
  out['id.window.arrange.cascade'] = { run: go({ cmd: 'cascade' }) };
  for (const id of Object.keys(ARRANGEMENTS)) {
    out[`id.window.arrange.${id}`] = { run: fits(id, docs) ? go({ cmd: 'arrange', how: id }) : undefined, checked: v.mode === id };
  }
  out['id.window.arrange.floatInWindow'] = { run: v.tabs.length > 1 || v.mode !== 'tabs' ? go({ cmd: 'floatDoc' }) : undefined };
  out['id.window.arrange.floatAllInWindows'] = { run: v.groups < docs ? go({ cmd: 'floatAll' }) : undefined };
  const together = inDoc['id.window.arrange.consolidateAllWindows']?.run;
  out['id.window.arrange.consolidateAllWindows'] = {
    run: v.groups > 1 || v.mode !== 'tabs' || together
      ? () => { together?.(); void apply({ cmd: 'consolidate' }); }
      : undefined,
  };
  return out;
}

/** This window's side: the state it was told, the commands, and the Window > Arrange items for them. */
export function useAppWindows(title: string | null, dirty: boolean, inDoc: Handlers) {
  const [view, setView] = useState<AppWinView | null>(null);
  useEffect(() => {
    let alive = true;
    window.folio.appWindows({ cmd: 'view' }).then((r) => { if (alive && r.ok && r.view) setView(r.view); }, () => {});
    const off = window.folio.onEvent((e) => { if (e.event === 'appwin' && e.view) setView(e.view); });
    return () => { alive = false; off(); };
  }, []);
  // the other windows' tab strips show this window's file name and unsaved mark
  useEffect(() => { void window.folio.appWindowMeta({ title, dirty }); }, [title, dirty]);
  const apply = useCallback((c: AppWinCommand) => window.folio.appWindows(c), []);
  const handlers = useMemo(() => composeArrange(inDoc, view, apply), [inDoc, view, apply]);
  return { view, apply, handlers } as const;
}

export type AppWindows = ReturnType<typeof useAppWindows>;
