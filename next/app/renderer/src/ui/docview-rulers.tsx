//! Draws rulers and handles edits to the ruler origin.

import { t as tr } from '../i18n';
import { useCallback, useEffect, useRef } from 'react';
import { pageOf, rulerSteps, trim, unitOf } from '../inspect';
import { Icon } from './icons';
import { cursor, useLive } from './store';
import { zeroOf, zeroPointAt } from './zeropoint';
import type { DocWindowProps } from './docview';
import type { PageCoordinates } from './docview-coordinates';
import type { Viewport } from './docview-viewport';

export function Rulers({ cornerTitle, vp, units, origin, zero, current, onDown, onDouble, onCorner, onCornerDouble, onMenu }: { cornerTitle: (unit: string) => string; onMenu?: (e: React.MouseEvent, kind: 'rulerH' | 'rulerV' | 'zeroPoint') => void; vp: Viewport; units: { horizontal: string; vertical: string }; origin: string; zero: [number, number]; current: number; onDown?: (e: React.PointerEvent<HTMLCanvasElement>, orientation: 'horizontal' | 'vertical') => void; onDouble?: (e: React.MouseEvent<HTMLCanvasElement>, orientation: 'horizontal' | 'vertical') => void; onCorner?: (e: React.PointerEvent<HTMLDivElement>) => void; onCornerDouble?: () => void }) {
  const h = useRef<HTMLCanvasElement>(null);
  const v = useRef<HTMLCanvasElement>(null);
  const cur = useLive(cursor);
  const frame = useRef(0);

  const redraw = useCallback(() => {
    const view = vp.viewRef.current;
    if (!view || !h.current || !v.current) return;
    const vr = view.getBoundingClientRect();
    const page = view.querySelector<HTMLElement>(`[data-page="${current}"]`);
    const row = page?.closest<HTMLElement>('.spread');
    const pr = page?.getBoundingClientRect();
    const rr = row?.getBoundingClientRect();
    const ox = origin === 'PageOrigin' ? pr?.left ?? vr.left : rr?.left ?? vr.left;
    const oy = rr?.top ?? vr.top;
    // the zero point moves what the rulers call 0 (the page and its items stay)
    drawRuler(h.current, 'h', vr.left, vr.width, ox + zero[0] * vp.scale, vp.scale, units.horizontal, cur?.clientX ?? null);
    drawRuler(v.current, 'v', vr.top, vr.height, oy + zero[1] * vp.scale, vp.scale, units.vertical, cur?.clientY ?? null);
  }, [vp, units, origin, zero, current, cur]);

  useEffect(() => {
    const view = vp.viewEl;
    if (!view) return;
    const on = () => { if (!frame.current) frame.current = requestAnimationFrame(() => { frame.current = 0; redraw(); }); };
    view.addEventListener('scroll', on, { passive: true });
    const ro = new ResizeObserver(on);
    ro.observe(view);
    on();
    return () => { view.removeEventListener('scroll', on); ro.disconnect(); if (frame.current) cancelAnimationFrame(frame.current); frame.current = 0; };
  }, [vp.viewEl, redraw]);

  return (
    <>
      <div className="ruler-corner" title={cornerTitle(unitOf(units.horizontal).suffix)} onPointerDown={onCorner} onDoubleClick={onCornerDouble} onContextMenu={onMenu && ((e) => onMenu(e, 'zeroPoint'))}><Icon name="plus" size={10} /></div>
      <canvas className="ruler ruler-h" ref={h} onPointerDown={onDown && ((e) => onDown(e, 'horizontal'))} onDoubleClick={onDouble && ((e) => onDouble(e, 'horizontal'))} onContextMenu={onMenu && ((e) => onMenu(e, 'rulerH'))} />
      <canvas className="ruler ruler-v" ref={v} onPointerDown={onDown && ((e) => onDown(e, 'vertical'))} onDoubleClick={onDouble && ((e) => onDouble(e, 'vertical'))} onContextMenu={onMenu && ((e) => onMenu(e, 'rulerV'))} />
    </>
  );
}

function drawRuler(c: HTMLCanvasElement, dir: 'h' | 'v', start: number, length: number, origin: number, scale: number, unitName: string, mouse: number | null) {
  const dpr = window.devicePixelRatio || 1;
  const thick = 17;
  const w = dir === 'h' ? length : thick, hgt = dir === 'h' ? thick : length;
  if (c.width !== Math.round(w * dpr) || c.height !== Math.round(hgt * dpr)) {
    c.width = Math.round(w * dpr);
    c.height = Math.round(hgt * dpr);
    c.style.width = `${w}px`;
    c.style.height = `${hgt}px`;
  }
  const g = c.getContext('2d')!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  const cs = getComputedStyle(c);
  const bg = cs.getPropertyValue('--ruler-bg').trim() || '#2c2c2c';
  const fg = cs.getPropertyValue('--ruler-fg').trim() || '#bdbdbd';
  g.fillStyle = bg;
  g.fillRect(0, 0, w, hgt);
  const { major, minor, pxPerUnit } = rulerSteps(unitName, scale);
  const sub = major / minor;
  // screen position (in this ruler) of unit value u
  const at = (u: number) => origin - start + u * pxPerUnit;
  const first = Math.floor((start - origin) / pxPerUnit / major) * major;
  const last = (start + length - origin) / pxPerUnit;
  g.strokeStyle = fg;
  g.fillStyle = fg;
  g.lineWidth = 1;
  // the interface font (ui/tokens.css), medium weight so 9px figures stay solid on the dark ruler
  g.font = '500 9px "Folio UI Sans", system-ui, "Segoe UI", sans-serif';
  // major and half ticks at full strength, the small ticks a step quieter (less noise along the edge of the page)
  for (const small of [false, true]) {
    g.globalAlpha = small ? 0.55 : 1;
    g.beginPath();
    for (let u = first; u <= last + major; u += sub) {
      const pos = Math.round(at(u)) + 0.5;
      const k = Math.round(u / sub);
      const isMajor = k % minor === 0;
      const half = !isMajor && minor % 2 === 0 && k % (minor / 2) === 0;
      if (small !== (!isMajor && !half)) continue;
      const len = isMajor ? thick : half ? 7 : 4;
      if (dir === 'h') { g.moveTo(pos, thick); g.lineTo(pos, thick - len); } else { g.moveTo(thick, pos); g.lineTo(thick - len, pos); }
    }
    g.stroke();
  }
  g.globalAlpha = 1;
  for (let u = first; u <= last + major; u += major) {
    const pos = Math.round(at(u));
    const label = trim(Math.abs(u) < 1e-9 ? 0 : u, 3);
    if (dir === 'h') {
      g.fillText(label, pos + 2, 9);
    } else {
      // digits stacked, as InDesign writes them on the vertical ruler
      [...label].forEach((ch, i) => g.fillText(ch, 2, pos + 9 + i * 8));
    }
  }
  if (mouse !== null) {
    const m = Math.round(mouse - start) + 0.5;
    g.strokeStyle = cs.getPropertyValue('--ruler-mouse').trim() || '#e8e8e8';
    g.setLineDash([2, 2]);
    g.beginPath();
    if (dir === 'h') { g.moveTo(m, 0); g.lineTo(m, thick); } else { g.moveTo(0, m); g.lineTo(thick, m); }
    g.stroke();
    g.setLineDash([]);
  }
  g.strokeStyle = cs.getPropertyValue('--line-dark').trim() || '#1a1a1a';
  g.beginPath();
  if (dir === 'h') { g.moveTo(0, thick - 0.5); g.lineTo(w, thick - 0.5); } else { g.moveTo(thick - 0.5, 0); g.lineTo(thick - 0.5, hgt); }
  g.stroke();
}

export function useRulerCorner(p: DocWindowProps, pageAt: PageCoordinates['pageAt']) {
  const { vp } = p;
  // The ruler corner: a drag onto the page makes the point under the pointer the zero point of the rulers; two clicks put it back (InDesign 2025).
  // A spine ruler cannot move it. The numbers are the engine's (`setViewPref` key `zeroPoint`): this only finds the point.
  const onRulerCorner = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || p.readOnly || p.zeroPointLocked) return;
    e.preventDefault();
    const spine = p.rulerOrigin === 'SpineOrigin';
    const up = (ev: PointerEvent) => {
      window.removeEventListener('pointerup', up);
      window.removeEventListener('pointercancel', up);
      const view = vp.viewRef.current;
      const vr = view?.getBoundingClientRect();
      if (!vr || ev.type === 'pointercancel' || ev.clientX < vr.left || ev.clientX > vr.right || ev.clientY < vr.top || ev.clientY > vr.bottom) return;
      if (Math.hypot(ev.clientX - e.clientX, ev.clientY - e.clientY) < 4) return; // a click is not a drag
      const at = pageAt(ev.clientX, ev.clientY);
      if (!at) return;
      const zero = zeroPointAt(p.rulerOrigin, pageOf(p.info, at.index)?.spreadOffset, [at.x, at.y]);
      if (!zero) {
        if (spine) p.onNote(tr('원점을 옮기려면 먼저 가로 눈금자의 바로 가기 메뉴에서 "쪽별 눈금자" 또는 "펼침면별 눈금자"를 고르세요.'));
        return;
      }
      void p.onEdit(tr('눈금자 원점 바꾸기'), [{ op: 'setViewPref', key: 'zeroPoint', value: zero }]);
    };
    window.addEventListener('pointerup', up);
    window.addEventListener('pointercancel', up);
  }, [p, vp.viewRef, pageAt]);
  const resetZeroPoint = useCallback(() => {
    const z = zeroOf(p.info);
    if (p.readOnly || p.zeroPointLocked || (z[0] === 0 && z[1] === 0)) return;
    void p.onEdit(tr('눈금자 원점 바꾸기'), [{ op: 'setViewPref', key: 'zeroPoint', value: [0, 0] }]);
  }, [p]);
  return { onRulerCorner, resetZeroPoint };
}
