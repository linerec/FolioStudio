import { useEffect, useRef, useState } from 'react';
import type { AppContext } from './context';
import type { SelEntry } from '../ui/overlay';
import type { ToolId } from '../ui/tools';
import { userSettings } from '../user-settings';

type ViewState = {
  page: number; zoom: number; scrollCenter: [number, number]; selection: SelEntry[];
  textSelection: { story: string; start: number; end: number } | null; tool: ToolId; showHiddenSpreads: boolean; viewRev: number;
};
type Patch = Partial<Omit<ViewState, 'viewRev'>>;
const canonical = (value: unknown): unknown => Array.isArray(value) ? value.map(canonical)
  : value && typeof value === 'object' ? Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => [k, canonical(v)])) : value;
const same = (a: unknown, b: unknown) => JSON.stringify(canonical(a)) === JSON.stringify(canonical(b));
const pathKey = (s: string) => s.replaceAll('\\', '/');

/** The canvas keeps its React state; session commands and events synchronize it at this boundary. */
export function useViewStateSync(context: AppContext) {
  const current = useRef(context);
  current.current = context;
  const origin = useRef(`canvas-${crypto.randomUUID()}`);
  const bridge = useRef<{
    baseline: Patch; pending: Patch; revision: number; ready: boolean; busy: boolean; disposed: boolean;
    apply: (view: ViewState, patch: Patch) => void;
  } | null>(null);
  const [tick, setTick] = useState(0);
  const wake = () => setTick(n => n + 1);
  const { realDoc, openedAt, vp, current: page, selection, textSelection, tool, showHidden } = context;

  useEffect(() => {
    if (!realDoc) return;
    const b = { baseline: {} as Patch, pending: {} as Patch, revision: 0, ready: false, busy: false, disposed: false,
      apply: (_view: ViewState, _patch: Patch) => {} };
    bridge.current = b;
    b.apply = (view, patch) => {
      if (b.disposed || view.viewRev < b.revision) return;
      b.revision = view.viewRev;
      const c = current.current;
      for (const key of Object.keys(patch) as (keyof Patch)[]) {
        Object.assign(b.baseline, { [key]: view[key] });
        Object.assign(b.pending, { [key]: view[key] });
      }
      if ('tool' in patch) c.setTool(view.tool);
      if ('showHiddenSpreads' in patch) {
        c.setShowHidden(view.showHiddenSpreads);
        userSettings.setItem('folio.showHiddenSpreads', view.showHiddenSpreads ? '1' : '0');
      }
      if ('textSelection' in patch) {
        if (view.textSelection) c.setTextReveal({ ...view.textSelection, allowOverset: true });
        else if (c.textActions.current) {
          void c.textActions.current.exit().then(() => {
            if (b.disposed || b.revision !== view.viewRev) return;
            if ('selection' in patch) current.current.setSelection(view.selection);
            if ('tool' in patch) current.current.setTool(view.tool);
          });
        }
      }
      if ('selection' in patch) c.setSelection(view.selection);
      if ('zoom' in patch) c.vp.zoomTo(view.zoom);
      if ('page' in patch) {
        const master = c.realInfo?.masterPages?.find(p => p.index === view.page);
        c.setMasterView(master?.masterSpread ?? null);
        c.setCurrent(view.page);
      }
      requestAnimationFrame(() => requestAnimationFrame(() => {
        if (b.disposed) return;
        const c = current.current;
        if ('page' in patch) c.vp.scrollToPage(view.page, false);
        if ('scrollCenter' in patch) {
          const el = c.vp.viewRef.current;
          if (el) el.scrollTo({ left: view.scrollCenter[0] * c.vp.scale - el.clientWidth / 2,
            top: view.scrollCenter[1] * c.vp.scale - el.clientHeight / 2 });
          // Browser scroll limits can clamp a requested centre; report the resulting centre.
          delete b.pending.scrollCenter;
        }
        wake();
      }));
      wake();
    };
    const off = window.folio.onEvent(event => {
      if (event.event !== 'view-state' || !event.path || pathKey(event.path) !== pathKey(realDoc.path)) return;
      const detail = event.detail as { viewState: ViewState; patch: Patch; origin?: string };
      if (detail.origin === origin.current) { b.revision = Math.max(b.revision, detail.viewState.viewRev); return; }
      b.apply(detail.viewState, detail.patch);
    });
    void window.folio.request({ cmd: 'viewState' }).then(({ header }) => {
      if (b.disposed || !header.ok) return;
      const view = header.viewState as ViewState;
      b.revision = Math.max(b.revision, view.viewRev);
      if (view.viewRev > 0) {
        const { viewRev: _, ...patch } = view;
        b.apply(view, patch);
      }
      b.ready = true;
      wake();
    }).catch(() => {});
    return () => { b.disposed = true; off(); if (bridge.current === b) bridge.current = null; };
  }, [realDoc?.path, openedAt]);

  useEffect(() => {
    const el = vp.viewEl;
    if (!el) return;
    let frame = 0;
    const changed = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(wake); };
    el.addEventListener('scroll', changed, { passive: true });
    const resize = new ResizeObserver(changed);
    resize.observe(el);
    return () => { el.removeEventListener('scroll', changed); resize.disconnect(); cancelAnimationFrame(frame); };
  }, [vp.viewEl]);

  useEffect(() => {
    const b = bridge.current, el = vp.viewRef.current;
    if (!b?.ready || b.busy || !realDoc || !el) return;
    const snapshot: Patch = {
      page, zoom: vp.scale, tool, selection, showHiddenSpreads: showHidden,
      textSelection: textSelection ? { story: textSelection.story, start: textSelection.start, end: textSelection.end } : null,
      scrollCenter: [(el.scrollLeft + el.clientWidth / 2) / vp.scale, (el.scrollTop + el.clientHeight / 2) / vp.scale],
    };
    const patch: Patch = {};
    for (const key of Object.keys(snapshot) as (keyof Patch)[]) {
      if (key in b.pending) {
        if (same(snapshot[key], b.pending[key])) delete b.pending[key];
        else continue;
      }
      if (!same(snapshot[key], b.baseline[key])) Object.assign(patch, { [key]: snapshot[key] });
    }
    if (!Object.keys(patch).length) return;
    b.busy = true;
    void window.folio.request({ cmd: 'setViewState', patch, origin: origin.current, ifViewRev: b.revision }).then(({ header }) => {
      if (b.disposed) return;
      if (header.ok) {
        if (header.viewRev >= b.revision) Object.assign(b.baseline, patch);
        b.revision = Math.max(b.revision, header.viewRev);
      } else if (header.viewState) {
        const { viewRev: _, ...remote } = header.viewState as ViewState;
        b.apply(header.viewState, remote);
      } else {
        // A deleted item or transient text range must not flood the engine with retries.
        Object.assign(b.baseline, patch);
      }
    }).catch(() => { Object.assign(b.baseline, patch); }).finally(() => { b.busy = false; if (!b.disposed) wake(); });
  }, [realDoc, vp.scale, vp.viewEl, page, selection, textSelection, tool, showHidden, tick]);
}
