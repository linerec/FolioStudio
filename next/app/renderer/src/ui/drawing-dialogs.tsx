import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../i18n';
import { fieldKey } from './keys';
import { NumberField } from './controls/numberfield';
import { formatInteger, parseInteger } from './controls/lengthfield';
import { drawingSettings, type PolygonSettings } from './drawing-settings';

export function PolygonDialog({ close }: { close: () => void }) {
  const [value, setValue] = useState<PolygonSettings | null>(null);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  // the first box has the focus when the settings have come (the old boxes had autoFocus)
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { if (value) box.current?.querySelector('input')?.focus(); }, [value !== null]);
  useEffect(() => { drawingSettings().then(s => setValue(s.polygon)).catch(e => setError(String(e))); }, []);
  const save = async () => {
    if (!value || busy) return;
    setBusy(true);
    try { await drawingSettings(value); close(); } catch (e) { setError(String(e)); setBusy(false); }
  };
  return createPortal(<div ref={box} className="style-modal" onPointerDown={e => e.stopPropagation()} onKeyDown={e => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) close(); }}>
    <form className="card dialog" style={{ width: 360, display: 'grid', gap: 12 }} role="dialog" aria-label={tr('다각형 설정')} onSubmit={e => { e.preventDefault(); void save(); }}>
      <h3>{tr('다각형 설정')}</h3>
      {value && <>
        <label>{tr('변 수:')} <NumberField value={value.sides} onCommit={v => setValue({ ...value, sides: v })} min={3} max={100} step={1} decimals={0} parse={parseInteger} format={formatInteger} label={tr('변 수:')} width={160} inDialog /></label>
        <label>{tr('별 안쪽 비율(%):')} <NumberField value={value.inset} onCommit={v => setValue({ ...value, inset: v })} min={0} max={100} step={1} decimals={4} label={tr('별 안쪽 비율(%):')} width={160} inDialog /></label>
      </>}
      {error && <p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={!value || busy}>{tr('확인')}</button></div>
    </form>
  </div>, document.body);
}

/** A click with a shape tool: the size of what is drawn there. With a Polygon tool (`polygon`) the number of sides and the star inset too, as
 *  InDesign's Polygon window: they are the tool's settings (the engine's `drawingSettings`) and stay for the next drawing. */
export function DrawingSizeDialog({ create, close, polygon = false }: { create: (width: number, height: number) => Promise<void>; close: () => void; polygon?: boolean }) {
  const [width, setWidth] = useState(100), [height, setHeight] = useState(100);
  const [star, setStar] = useState<PolygonSettings | null>(null);
  const [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { box.current?.querySelector('input')?.focus(); }, []);
  useEffect(() => { if (polygon) drawingSettings().then(s => setStar(s.polygon)).catch(e => setError(String(e))); }, [polygon]);
  return createPortal(<div ref={box} className="style-modal" onPointerDown={e => e.stopPropagation()} onKeyDown={e => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) close(); }}>
    <form className="card dialog" style={{ width: 360, display: 'grid', gap: 12 }} role="dialog" aria-label={tr('그리기 치수')} data-polygon={polygon ? 'true' : undefined} onSubmit={async e => {
      e.preventDefault(); if (busy || (polygon && !star)) return; setBusy(true);
      try { if (polygon && star) await drawingSettings(star); await create(width, height); close(); } catch (e) { setError(String(e)); setBusy(false); }
    }}>
      <h3>{tr('그리기 치수')}</h3>
      <label>{tr('너비(pt):')} <NumberField value={width} onCommit={setWidth} min={0.01} max={1e6} step={1} decimals={4} label={tr('너비(pt):')} width={160} inDialog /></label>
      <label>{tr('높이(pt):')} <NumberField value={height} onCommit={setHeight} min={0.01} max={1e6} step={1} decimals={4} label={tr('높이(pt):')} width={160} inDialog /></label>
      {polygon && star && <>
        <label>{tr('변 수:')} <NumberField value={star.sides} onCommit={v => setStar({ ...star, sides: v })} min={3} max={100} step={1} decimals={0} parse={parseInteger} format={formatInteger} label={tr('변 수:')} width={160} inDialog /></label>
        <label>{tr('별 안쪽 비율(%):')} <NumberField value={star.inset} onCommit={v => setStar({ ...star, inset: v })} min={0} max={100} step={1} decimals={4} label={tr('별 안쪽 비율(%):')} width={160} inDialog /></label>
      </>}
      {error && <p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy}>{tr('확인')}</button></div>
    </form>
  </div>, document.body);
}
