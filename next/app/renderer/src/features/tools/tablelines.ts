import { useEffect, useRef } from 'react';
import { t as tr } from '../../i18n';
import { type EditCommand, tableLines } from '../../engine';
import { fieldKey } from '../../ui/keys';
import type { Ghost } from '../../ui/overlay';
import type { ToolId } from '../../ui/tools';
import { inDesignZoom, snapEdge } from '../../ui/dragunit';
import { type LineHit, type Pt, type TableLineInfo, axisParallel, dragAlong, lineAt, movedSeg } from '../../ui/tablelinehit';

type Event = React.PointerEvent<HTMLDivElement>;
type At = { index: number; x: number; y: number } | null;
type Drag = { hit: LineHit; from: Pt; by: number };
type Props = {
  tool: ToolId; editable: boolean;
  /** Changes whenever the pages are laid out again (the cached lines are then stale). */
  revision: unknown;
  view: React.RefObject<HTMLDivElement | null>; scale: number;
  ghost: (v: { pages: number[]; g: Ghost } | null) => void;
  edit: (label: string, commands: EditCommand[]) => unknown;
};
/** How close (screen pixels) the pointer has to be to a table line to take it (InDesign 2025: within about 4 screen pixels). */
const GRAB_PX = 4;

/** Type tool: the pointer over a table's row or column line shows a resize cursor and a drag moves the line (engine `dragTableLine`;
 * Shift and Alt are read when the button is let go). The lines come from the engine query `tableLines` (already laid out). */
export function useTableLines(p: Props) {
  const drag = useRef<Drag | null>(null);
  const cache = useRef<{ info: TableLineInfo; rev: unknown } | null>(null);
  const asked = useRef<{ rev: unknown; page: number; x: number; y: number; busy: boolean }>({ rev: undefined, page: -1, x: 0, y: 0, busy: false });
  const live = useRef(p);
  live.current = p;
  const on = () => live.current.tool === 'type' && live.current.editable;
  const cursor = (c: string) => { const v = live.current.view.current; if (v && v.style.cursor !== c) v.style.cursor = c; };
  const cancel = () => { if (drag.current) { drag.current = null; p.ghost(null); } cursor(''); };
  useEffect(() => { cancel(); cache.current = null; }, [p.tool, p.editable]);
  useEffect(() => { cache.current = null; }, [p.revision]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => { if (drag.current && fieldKey(e) === 'cancel') { cancel(); e.preventDefault(); e.stopImmediatePropagation(); } };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, []);
  const tol = () => GRAB_PX / Math.max(live.current.scale, 1e-6);
  /** The line under a page point from the cached table; asks the engine (once at a time) when the pointer is somewhere else. */
  const hitAt = (at: At): LineHit | null => {
    if (!at) return null;
    const c = cache.current;
    const hit = c && c.rev === live.current.revision ? lineAt(c.info, at.index, [at.x, at.y], tol()) : null;
    if (hit) return hit;
    const a = asked.current;
    if (a.busy || (a.rev === live.current.revision && a.page === at.index && Math.hypot(a.x - at.x, a.y - at.y) * live.current.scale < 6)) return null;
    Object.assign(a, { rev: live.current.revision, page: at.index, x: at.x, y: at.y, busy: true });
    const rev = live.current.revision;
    void tableLines({ page: at.index, x: at.x, y: at.y, tol: tol() * 2 }).then((info) => {
      asked.current.busy = false;
      if (rev !== live.current.revision) return;
      cache.current = info.table ? { info, rev } : null;
    }).catch(() => { asked.current.busy = false; });
    return null;
  };
  const down = (e: Event, at: At) => {
    if (!on() || !at) return false;
    const hit = hitAt(at);
    if (!hit) return false;
    e.preventDefault();
    drag.current = { hit, from: [at.x, at.y], by: 0 };
    return true;
  };
  const amount = (d: Drag, at: Pt) => {
    const raw = dragAlong(d.hit, d.from, at);
    if (!axisParallel(d.hit)) return raw;
    // the line is the far edge of the column (row) before it and lands on InDesign's drag grid (page position), which depends on the zoom
    const axis = d.hit.columns ? 0 : 1, pos = d.hit.seg.from[axis];
    return snapEdge(pos + raw, inDesignZoom(live.current.scale), true) - pos;
  };
  const move = (e: Event, at: At, point?: Pt) => {
    const d = drag.current;
    if (d) {
      if (!point) return true;
      d.by = amount(d, point);
      const s = movedSeg(d.hit, d.by);
      p.ghost({ pages: [d.hit.page], g: { kind: 'line', from: s.from, to: s.to } });
      return true;
    }
    if (!on() || e.buttons) { cursor(''); return false; }
    const hit = hitAt(at);
    cursor(hit ? (hit.columns ? 'col-resize' : 'row-resize') : '');
    return false;
  };
  const up = (e: Event, point?: Pt) => {
    const d = drag.current;
    if (!d) return false;
    if (point) d.by = amount(d, point);
    drag.current = null;
    p.ghost(null);
    cursor('');
    const { hit } = d;
    if (Math.abs(d.by) * live.current.scale >= 1) {
      void p.edit(tr('표 선 끌기'), [{ op: 'dragTableLine', table: hit.table, columns: hit.columns, line: hit.line, by: d.by, shift: e.shiftKey, insert: e.altKey, size: hit.seg.size, min_before: hit.columns ? undefined : hit.seg.min, min_after: hit.columns ? undefined : hit.minAfter }]);
    }
    return true;
  };
  return { down, move, up, cancel, active: () => !!drag.current };
}
