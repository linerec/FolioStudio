import { useAppState } from './useAppState';
import { useFlattenerPreview } from '../features/flattener/state';
import { usePreflightMenu } from '../ui/preflightmenu';
import { useInfoMenu } from '../ui/infomenu';
import { usePanelOptions } from '../ui/paneloptions';
import { useCallback,useMemo,useRef } from 'react';
import {
type Bitmap,inkAt
} from '../engine';
import { unionBox } from '../inspect';
import { type Target } from '../print';
import { inkView } from '../separations';
import { findItem,layerLook } from '../ui/overlay';
import { useWorkspaces } from '../ui/workspaces';

export function useViewCommands(state: ReturnType<typeof useAppState>) {
  const {
    sepSupported, sep, inks,
    realDoc, info, settings,
    dock, workbench, setReading, setLastRender,
    setFirstPaint, openedAt, setTarget,
    vpRef, showTools, setShowTools,
    showControl, setShowControl, flash,
    preflight, doc, unitsPref,
    rows, current, store,
    setSelection, selection, setSep,
    vp,
  } = state;
  // --- separations, ink under the cursor ---------------------------------------------
  const sepView = useMemo(() => (sepSupported ? inkView(sep, inks) : null), [sep, inks, sepSupported]);
  // Window ▸ Output ▸ Flattener Preview draws the page its own way while a Highlight is chosen (features/flattener)
  const flattener = useFlattenerPreview(realDoc, info, settings);
  const view = flattener.view ?? sepView;
  const sepActive = sepSupported && inks !== null && (dock.isOpen('separations') || sepView !== null);
  const hoverAt = useRef<{ page: number; x: number; y: number } | null>(null);
  const hoverBusy = useRef(false);
  const readInk = useCallback(() => {
    if (hoverBusy.current || !hoverAt.current) return;
    const q = hoverAt.current;
    hoverAt.current = null;
    hoverBusy.current = true;
    inkAt(q.page, q.x, q.y).then(setReading, () => {}).finally(() => { hoverBusy.current = false; readInk(); });
  }, []);
  const onInk = useCallback((page: number, x: number, y: number) => { hoverAt.current = { page, x, y }; readInk(); }, [readInk]);
  const onInkLeave = useCallback(() => { hoverAt.current = null; setReading(null); }, []);

  const onPainted = useCallback((b: Bitmap) => { setLastRender(b.ms); setFirstPaint((f) => f ?? performance.now() - openedAt); }, [openedAt]);
  const go = useCallback((t: Target) => { setTarget({ ...t }); vpRef.current.scrollToTarget(t); }, []);
  const infoMenu = useInfoMenu();
  const panelOptions = usePanelOptions();
  // Window > Workspace: saved panel layouts (this computer's settings)
  const workspaces = useWorkspaces({ dock, tools: showTools, setTools: setShowTools, control: showControl, setControl: setShowControl, flash, workbench });
  const preflightMenu = usePreflightMenu({ report: preflight, document: doc?.path ?? null, note: flash });

  const layers = useMemo(() => layerLook(info), [info]);
  const units = useMemo(() => {
    if (unitsPref !== 'document') return { horizontal: unitsPref, vertical: unitsPref };
    return info?.units ?? { horizontal: 'Millimeters', vertical: 'Millimeters' };
  }, [unitsPref, info]);
  const rulerOrigin = info?.rulerOrigin ?? 'SpreadOrigin';

  // --- selection helpers --------------------------------------------------------------
  const spreadItems = useCallback(() => {
    const row = rows.find((r) => r.some((p) => p.index === current));
    if (!row) return [];
    return row.flatMap((p) => (store.peek(p.index)?.items ?? []).map((it) => ({ page: p.index, it })));
  }, [rows, current, store]);

  const selectAll = useCallback(() => {
    setSelection(
      spreadItems()
        .filter(({ it }) => !it.locked && it.visible && !(it.layer && (layers.get(it.layer)?.locked || !layers.get(it.layer)?.visible)))
        .map(({ page, it }) => ({ page, id: it.id, content: false })),
    );
  }, [spreadItems, layers]);

  const selectContainer = useCallback(() => {
    const s = selection[0];
    if (!s) return;
    if (s.content) { setSelection([{ ...s, content: false }]); return; }
    const f = findItem(store.peek(s.page), s.id);
    if (f && f.path.length > 1) setSelection([{ page: s.page, id: f.path[f.path.length - 2].id, content: false }]);
  }, [selection, store]);

  const selectContent = useCallback(() => {
    const s = selection[0];
    if (!s) return;
    const f = findItem(store.peek(s.page), s.id);
    if (!f) return;
    if (f.item.content && !s.content) setSelection([{ ...s, content: true }]);
    else if (f.item.kind === 'group' && f.item.children.length) setSelection([{ page: s.page, id: f.item.children[f.item.children.length - 1].id, content: false }]);
  }, [selection, store]);

  /** Object ▸ Select ▸ next object above/below (z order on the spread). */
  const selectStep = useCallback((dir: 1 | -1, end: boolean) => {
    const list = spreadItems().filter(({ it }) => !it.locked).sort((a, b) => a.it.z - b.it.z);
    if (!list.length) return;
    const cur = selection[0] ? list.findIndex((x) => x.it.id === selection[0].id) : -1;
    const i = end ? (dir > 0 ? list.length - 1 : 0) : cur < 0 ? (dir > 0 ? 0 : list.length - 1) : (cur + dir + list.length) % list.length;
    setSelection([{ page: list[i].page, id: list[i].it.id, content: false }]);
  }, [spreadItems, selection]);

  /** Object ▸ Select ▸ Previous / Next Object in Group: the member before or after the selected
   *  one in its group, front to back, round the ends (oracle basic-select-group: from the first
   *  member the next is the second, from the last the next is the first). */
  const groupStep = useCallback((dir: 1 | -1) => {
    const s = selection.length === 1 && !selection[0].content ? selection[0] : null;
    const f = s ? findItem(store.peek(s.page), s.id) : null;
    // (path ends with the item itself)
    const parent = f && f.path.length > 1 ? f.path[f.path.length - 2] : null;
    if (!s || !f || !parent || parent.kind !== 'group') return;
    const sib = parent.children;
    const k = sib.findIndex((c) => c.id === s.id);
    const to = sib[(k + dir + sib.length) % sib.length];
    if (to) setSelection([{ page: s.page, id: to.id, content: false }]);
  }, [selection, store]);

  /** Separations preview of one plate (View shortcuts Ctrl+Alt+Shift+1…4). */
  const soloPlate = useCallback((name: string) => {
    if (!inks) return;
    setSep((s) => ({ ...s, mode: 'separations', hidden: inks.inks.map((k) => k.name).filter((n) => n !== name) }));
  }, [inks]);

  /** View ▸ Fit Selection in Window. */
  const fitSelection = useCallback(() => {
    const s = selection[0];
    const view = vp.viewRef.current;
    if (!s || !view) return;
    const boxes = selection.filter((e) => e.page === s.page).map((e) => findItem(store.peek(e.page), e.id)?.item.bounds).filter((b): b is [number, number, number, number] => !!b);
    const u = unionBox(boxes);
    if (!u) return;
    const k = Math.min(view.clientWidth / Math.max(1, u[2] - u[0]), view.clientHeight / Math.max(1, u[3] - u[1])) * 0.85;
    vp.zoomTo(k);
    requestAnimationFrame(() => requestAnimationFrame(() => {
      const el = view.querySelector<HTMLElement>(`[data-page="${s.page}"]`);
      if (!el) return;
      const r = el.getBoundingClientRect(), vr = view.getBoundingClientRect();
      const cx = r.left - vr.left + view.scrollLeft + ((u[0] + u[2]) / 2) * k;
      const cy = r.top - vr.top + view.scrollTop + ((u[1] + u[3]) / 2) * k;
      view.scrollTo({ left: cx - view.clientWidth / 2, top: cy - view.clientHeight / 2 });
    }));
  }, [selection, store, vp]);

  return {
    go, units, spreadItems,
    rulerOrigin, fitSelection, preflightMenu,
    infoMenu, panelOptions, selectAll,
    soloPlate, selectContainer, selectContent,
    selectStep, groupStep, workspaces,
    flattener, view, onPainted,
    sepActive, onInk, onInkLeave,
    layers,
  };
}
