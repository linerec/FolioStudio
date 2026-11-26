import { useEffect, useRef } from 'react';
import { t as tr } from '../../i18n';
import type { EditCommand } from '../../engine';
import type { DocumentInfo } from '../../inspect';
import { fieldKey } from '../../ui/keys';
import type { Ghost } from '../../ui/overlay';
import { inDesignZoom, snapPointer } from '../../ui/dragunit';
import type { ToolId } from '../../ui/tools';

type Point = [number, number];
type Event = React.PointerEvent<HTMLDivElement>;
type Drag = { page: number; id: string; from: Point; to: Point; moved: boolean };
type Tool = { label: () => string; command: (d: Drag, e: Event) => EditCommand };
const TOOLS: Partial<Record<ToolId, Tool>> = {
  gap: {
    label: () => tr('간격 조절'),
    // Shift: only the two items nearest the pointer; Ctrl: the gap gets wider or narrower (the items beside it change size); Alt: the gap and the
    // items move together; Ctrl+Alt: the items move apart or together (tools-c2: the engine's adjustGapKeys, as InDesign's help describes the keys)
    command: (d, e) => ({ op: 'adjustGapKeys', page: d.id, from: d.from, to: d.to, nearest: e.shiftKey, resize: e.ctrlKey || e.metaKey, move_items: e.altKey }),
  },
};
type Props = {
  tool: ToolId; editable: boolean; info: DocumentInfo | null;
  view: React.RefObject<HTMLDivElement | null>; pxPerPt: (el: HTMLElement, rect: DOMRect) => number;
  ghost: (v: { pages: number[]; g: Ghost } | null) => void;
  edit: (label: string, commands: EditCommand[]) => unknown;
};
export function useCoordinateTools(p: Props) {
  const drag = useRef<Drag | null>(null);
  const cancel = () => { if (drag.current) { drag.current = null; p.ghost(null); } };
  useEffect(() => { cancel(); }, [p.tool, p.editable]);
  useEffect(() => {
    // An edit's documentInfo reply can arrive after the next drag has started.
    const d = drag.current;
    if (d && !p.info?.pages.some(page => page.index === d.page && page.id === d.id)) cancel();
  }, [p.info]);
  useEffect(() => {
    const key = (e: KeyboardEvent) => { if (drag.current && fieldKey(e) === 'cancel') { cancel(); e.preventDefault(); e.stopImmediatePropagation(); } };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, []);
  const down = (e: Event, at: { index: number; x: number; y: number } | null) => {
    if (!TOOLS[p.tool]) return false;
    const page = p.info?.pages.find(page => page.index === at?.index);
    if (!at || !page || !p.editable) return true;
    e.preventDefault();
    drag.current = { page: at.index, id: page.id, from: [at.x, at.y], to: [at.x, at.y], moved: false };
    return true;
  };
  const move = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    const el = p.view.current?.querySelector<HTMLElement>(`.page[data-page="${d.page}"]`);
    if (!el) return true;
    const r = el.getBoundingClientRect(), k = p.pxPerPt(el, r);
    const raw: Point = [(e.clientX - r.left) / k, (e.clientY - r.top) / k];
    d.moved = Math.hypot(raw[0] - d.from[0], raw[1] - d.from[1]) * k >= 3;
    // InDesign works with the pointer on its drag grid: the distance a gap is moved is the difference of the two grid positions (docdrag-c1: Gap tool, 40 drags,
    // delta = P(release) - P(press) with P(x) = unit * floor(x / unit + 0.2..0.3); the facing edges of the neighbours move by it)
    const z = inDesignZoom(k);
    d.to = [d.from[0] + snapPointer(raw[0], z) - snapPointer(d.from[0], z), d.from[1] + snapPointer(raw[1], z) - snapPointer(d.from[1], z)];
    p.ghost({ pages: [d.page], g: { kind: 'gradient', from: d.from, to: d.to } });
    return true;
  };
  const up = (e: Event) => {
    const d = drag.current, tool = TOOLS[p.tool];
    if (!d) return false;
    move(e); cancel();
    if (d.moved && tool) p.edit(tool.label(), [tool.command(d, e)]);
    return true;
  };
  return { down, move, up, cancel };
}
