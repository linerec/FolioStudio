// The stroke picture of Cell Options ▸ Strokes and Fills: the selection drawn as a small grid whose lines are picked by clicking
// (blue = picked, grey = left alone); the inner lines are there when the selection spans more than one row or column. Right-click
// (or Shift+F10) opens Select All Outer Edges / Select All Inner Edges / Select All / Clear All. The picture only chooses which
// edges the dialog's stroke goes to: the values are the same per-side cell strokes the engine's `setCells` writes.
import { t as tr } from '../i18n';
import { useState } from 'react';

export type EdgeSet = { Top: boolean; Left: boolean; Bottom: boolean; Right: boolean; InnerH: boolean; InnerV: boolean };
export type EdgeName = keyof EdgeSet;
export const ALL_EDGES: EdgeSet = { Top: true, Left: true, Bottom: true, Right: true, InnerH: true, InnerV: true };

const OUTER: EdgeName[] = ['Top', 'Left', 'Bottom', 'Right'];

export function EdgeProxy({ edges, onChange, rows, columns }: { edges: EdgeSet; onChange: (next: EdgeSet) => void; rows: number; columns: number }) {
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const innerH = rows > 1, innerV = columns > 1;
  const names: Record<EdgeName, string> = { Top: tr('위쪽'), Left: tr('왼쪽'), Bottom: tr('아래쪽'), Right: tr('오른쪽'), InnerH: tr('안쪽 가로'), InnerV: tr('안쪽 세로') };
  const shown = (e: EdgeName) => (e === 'InnerH' ? innerH : e === 'InnerV' ? innerV : true);
  const set = (pick: (e: EdgeName) => boolean) => {
    const next = { ...edges };
    (Object.keys(next) as EdgeName[]).forEach((e) => { next[e] = shown(e) && pick(e); });
    onChange(next);
    setMenu(null);
  };
  const toggle = (e: EdgeName) => onChange({ ...edges, [e]: !edges[e] });
  // 100 x 100 drawing: the outer square from 10 to 90, the inner lines through the middle
  const geometry: Record<EdgeName, [number, number, number, number]> = {
    Top: [10, 10, 90, 10], Bottom: [10, 90, 90, 90], Left: [10, 10, 10, 90], Right: [90, 10, 90, 90], InnerH: [10, 50, 90, 50], InnerV: [50, 10, 50, 90],
  };
  const line = (e: EdgeName) => {
    const [x1, y1, x2, y2] = geometry[e];
    return (
      <g key={e} role="checkbox" aria-checked={edges[e]} aria-label={tr('가장자리 {0}', { 0: names[e] })} tabIndex={0} data-edge={e} className="edge-proxy-line"
        onClick={() => toggle(e)} onKeyDown={(ev) => { if (ev.key === ' ' || ev.key === 'Enter') { ev.preventDefault(); toggle(e); } }}>
        {/* a wide transparent stroke to click, the visible line over it */}
        <line x1={x1} y1={y1} x2={x2} y2={y2} stroke="transparent" strokeWidth={10} />
        <line x1={x1} y1={y1} x2={x2} y2={y2} stroke={edges[e] ? 'var(--accent, #2f7bf6)' : 'var(--muted, #8a8f98)'} strokeWidth={edges[e] ? 3.5 : 1.5} strokeLinecap="square" />
      </g>
    );
  };
  const open = (x: number, y: number) => setMenu({ x, y });
  return (
    <div className="edge-proxy" role="group" aria-label={tr('바꿀 가장자리')}
      onContextMenu={(ev) => { ev.preventDefault(); open(ev.nativeEvent.offsetX, ev.nativeEvent.offsetY); }}
      onKeyDown={(ev) => { if ((ev.key === 'F10' && ev.shiftKey) || ev.key === 'ContextMenu') { ev.preventDefault(); open(8, 8); } if (ev.key === 'Escape' && menu) { ev.stopPropagation(); setMenu(null); } }}
      style={{ position: 'relative', width: 112, height: 112 }}>
      <svg viewBox="0 0 100 100" width={112} height={112} aria-hidden={false}>
        <rect x={10} y={10} width={80} height={80} fill="var(--field-bg, transparent)" />
        {(Object.keys(geometry) as EdgeName[]).filter(shown).map(line)}
      </svg>
      {menu && (
        <div className="st-menu edge-proxy-menu" role="menu" aria-label={tr('가장자리 메뉴')} style={{ position: 'absolute', left: menu.x, top: menu.y, zIndex: 10 }}>
          <button type="button" role="menuitem" onClick={() => set((e) => OUTER.includes(e))}>{tr('모든 바깥 테두리 선택')}</button>
          <button type="button" role="menuitem" disabled={!innerH && !innerV} onClick={() => set((e) => !OUTER.includes(e))}>{tr('모든 안쪽 테두리 선택')}</button>
          <button type="button" role="menuitem" onClick={() => set(() => true)}>{tr('모두 선택')}</button>
          <button type="button" role="menuitem" onClick={() => set(() => false)}>{tr('모두 지우기')}</button>
        </div>
      )}
    </div>
  );
}
