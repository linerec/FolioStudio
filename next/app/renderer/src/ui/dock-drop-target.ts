//! Finds the dock drop target and preview rectangle under a pointer.

import { AREA_SIZE, type Area, type DropTarget, type Edge } from './docklayout';

export type Rect = { x: number; y: number; w: number; h: number };

/** Where the dragged panel's ghost sits: just below and right of the pointer (set by the render and, between renders, by the drag). */
export function placeGhost<T extends { left?: string | number; top?: string | number }>(style: T, x: number, y: number): T {
  style.left = `${x + 12}px`;
  style.top = `${y + 10}px`;
  return style;
}

export const dropKey = (t: DropTarget) => JSON.stringify(t);
export const rectOf = (el: Element): Rect => { const r = el.getBoundingClientRect(); return { x: r.left, y: r.top, w: r.width, h: r.height }; };
export const view = () => ({ w: window.innerWidth, h: window.innerHeight });
export const isAreaZone = (z: unknown): z is Area => z === 'left' || z === 'column' || z === 'bottom';

/** The part of a group a split on one side would take (half of it), or of an area a new column / row at its edge would take. */
function band(r: Rect, edge: Edge, part = 0.5): Rect {
  if (edge === 'left') return { ...r, w: r.w * part };
  if (edge === 'right') return { ...r, x: r.x + r.w * (1 - part), w: r.w * part };
  if (edge === 'top') return { ...r, h: r.h * part };
  return { ...r, y: r.y + r.h * (1 - part), h: r.h * part };
}

/**
 * Where a drop at (x, y) would land. Places that say so themselves (`data-drop`: tabs, tab strips, dock icons, the places between
 * groups, an area's outer edges) come first; over a group's contents the place depends on where in the group: near an edge (the outer
 * third) splits the group on that side, the middle joins its tabs (VS Code's editor groups and Visual Studio's guides do the same).
 * `skipFloating`: the floating panel being moved (it is under the pointer itself). Null: nowhere in the dock (the panel floats).
 * `doc`: the page of the window the pointer is over (a panel window's own, uix-d3; x and y are in that page).
 */
export function targetAt(x: number, y: number, skipFloating: number | null = null, doc: Document = document): { to: DropTarget; key: string; preview: Rect | null } | null {
  for (const el of doc.elementsFromPoint(x, y)) {
    const h = el as HTMLElement;
    if (skipFloating !== null && h.closest?.(`.floating[data-float="${skipFloating}"]`)) continue;
    const d = h.dataset?.drop;
    if (d) {
      try {
        const to = JSON.parse(d) as DropTarget;
        if (skipFloating !== null && to.zone === 'floating' && 'group' in to && to.group === skipFloating) continue;
        let preview: Rect | null = null;
        if (h.classList.contains('dock-band') && isAreaZone(to.zone)) {
          // an empty area (or one of panels this workspace leaves out): the area as it would open, its default size along the document's
          // edge (the right one opens leftwards from the right edge)
          const r = rectOf(h), def = AREA_SIZE[to.zone].def;
          preview = to.zone === 'bottom' ? { x: r.x, y: r.y + r.h - def, w: r.w, h: def } : to.zone === 'column' ? { x: r.x + r.w - def, y: r.y, w: def, h: r.h } : { x: r.x, y: r.y, w: def, h: r.h };
        } else if ('edge' in to && !('group' in to)) {
          // an area's outer edge: the new column or row would take a band of the whole area
          const area = doc.querySelector(`[data-area="${to.zone}"]`);
          preview = area ? band(rectOf(area), to.edge, 0.34) : rectOf(h);
        }
        return { to, key: d, preview };
      } catch { /* not a target */ }
    }
    if (h.classList?.contains('dpanel-body')) {
      const frame = h.closest('.dpanel') as HTMLElement | null;
      const zone = frame?.dataset.zone;
      const group = Number(frame?.dataset.group);
      if (!frame || !Number.isInteger(group)) return null;
      const r = rectOf(frame);
      if (isAreaZone(zone)) {
        const fx = (x - r.x) / r.w, fy = (y - r.y) / r.h;
        const near = [['left', fx], ['right', 1 - fx], ['top', fy], ['bottom', 1 - fy]] as [Edge, number][];
        near.sort((a, b) => a[1] - b[1]);
        if (near[0][1] < 1 / 3) {
          const to: DropTarget = { zone, group, edge: near[0][0] };
          return { to, key: dropKey(to), preview: band(r, near[0][0]) };
        }
        const to: DropTarget = { zone, group };
        return { to, key: dropKey(to), preview: r };
      }
      if (zone === 'floating' || zone === 'strip') {
        const to: DropTarget = { zone, group };
        return { to, key: dropKey(to), preview: r };
      }
    }
  }
  return null;
}
