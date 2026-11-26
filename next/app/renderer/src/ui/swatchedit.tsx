// Swatches panel editing: New Color Swatch, Swatch Options, Delete Swatch (with a replacement),
// tint swatches. The rules (Graphic.xml order, colour-group slots, renaming and replacing
// references) are InDesign's, measured in next/oracle/scenarios/basic-swatches.json and kept in
// the engine (edit/src/ops/swatches.rs).
import { t as tr } from '../i18n';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';
import { NumberField } from './controls/numberfield';
import './controls/numberfield.dialog.css';

export type Swatch = NonNullable<DocumentInfo['swatches']>[number];
type Edit = (label: string, commands: EditCommand[]) => unknown;
type Space = 'CMYK' | 'RGB';

const round = (v: number) => Math.round(v * 100) / 100;

/** InDesign's "Name with Color Value": C=0 M=50 Y=100 K=0, R=255 G=0 B=0. */
export function valueName(space: Space, v: number[]): string {
  const k = space === 'CMYK' ? ['C', 'M', 'Y', 'K'] : ['R', 'G', 'B'];
  return k.map((c, i) => `${c}=${round(v[i] ?? 0)}`).join(' ');
}

/** A swatch the user may change or delete: not [None], [Registration], [Paper] or [Black]. */
export function editable(s: Swatch): boolean {
  return !['Swatch/None', 'Color/Registration', 'Color/Paper', 'Color/Black'].includes(s.id) && s.model !== 'none';
}

export function Modal({ label, onSubmit, close, children, ok }: { label: string; onSubmit: () => void; close: () => void; children: React.ReactNode; ok: boolean }) {
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={label} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); if (ok) onSubmit(); }}>
        <h2>{label}</h2>
        {children}
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!ok}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}

/** New Color Swatch (from `from`'s values) or Swatch Options of `from`. */
export function SwatchDialog({ mode, from, swatches, edit, close }: { mode: 'new' | 'edit'; from: Swatch | null; swatches: Swatch[]; edit: Edit; close: () => void }) {
  const start: Space = from?.space === 'RGB' ? 'RGB' : 'CMYK';
  const [space, setSpace] = useState<Space>(start);
  const [values, setValues] = useState<number[]>(from && from.values.length === (start === 'CMYK' ? 4 : 3) ? from.values.map(round) : start === 'CMYK' ? [0, 0, 0, 100] : [0, 0, 0]);
  const [spot, setSpot] = useState(from?.model === 'spot');
  const auto0 = mode === 'new' || (!!from && from.name === valueName(start, from.values));
  const [byValue, setByValue] = useState(auto0);
  const [name, setName] = useState(mode === 'edit' && from ? from.name : '');
  const shown = byValue ? valueName(space, values) : name;
  const taken = shown.trim() !== (mode === 'edit' ? from?.name : '') && swatches.some((s) => s.name === shown.trim());
  const ok = !!shown.trim() && !taken && values.every((v, i) => Number.isFinite(v) && v >= 0 && v <= (space === 'CMYK' ? 100 : 255) && i < (space === 'CMYK' ? 4 : 3));
  const keys = space === 'CMYK' ? [tr('녹청'), tr('자홍'), tr('노랑'), tr('검정')] : [tr('빨강'), tr('초록'), tr('파랑')];
  const changeSpace = (s: Space) => { setSpace(s); setValues(s === 'CMYK' ? [0, 0, 0, 100] : [0, 0, 0]); };
  const submit = () => {
    const finalName = shown.trim();
    if (mode === 'new') {
      edit(tr('새 색상 견본'), [{ op: 'newSwatch', name: finalName, spot, space: space.toLowerCase(), value: values }]);
    } else if (from) {
      const change: Record<string, unknown> = {};
      if (space !== start) change.space = space.toLowerCase();
      if (space !== start || values.some((v, i) => v !== round(from.values[i] ?? 0))) change.value = values;
      if (spot !== (from.model === 'spot')) change.spot = spot;
      if (finalName !== from.name) change.rename = finalName;
      if (Object.keys(change).length) edit(tr('색상 견본 옵션'), [{ op: 'setSwatch', id: from.id, ...change } as EditCommand]);
    }
    close();
  };
  return (
    <Modal label={mode === 'new' ? tr('새 색상 견본') : tr('색상 견본 옵션')} onSubmit={submit} close={close} ok={ok}>
      <label>{tr('견본 이름')}<input aria-label={tr('견본 이름')} autoFocus value={shown} disabled={byValue} onChange={(e) => setName(e.target.value)} /></label>
      <label className="check"><input type="checkbox" checked={byValue} onChange={(e) => { setByValue(e.target.checked); if (!e.target.checked) setName(shown); }} /> {tr('색상 값으로 이름 지정')}</label>
      {taken && <p className="validation-message">{tr('같은 이름의 견본이 있습니다.')}</p>}
      <label>{tr('색상 유형')}<select aria-label={tr('색상 유형')} value={spot ? 'spot' : 'process'} onChange={(e) => setSpot(e.target.value === 'spot')}>
        <option value="process">{tr('원색')}</option><option value="spot">{tr('별색')}</option></select></label>
      <label>{tr('색상 모드')}<select aria-label={tr('색상 모드')} value={space} onChange={(e) => changeSpace(e.target.value as Space)}>
        <option value="CMYK">CMYK</option><option value="RGB">RGB</option></select></label>
      <div className="style-fields">
        {keys.map((k, i) => (
          <label key={k}>{k}<NumberField label={tr('색상 값 {0}', { 0: k })} min={0} max={space === 'CMYK' ? 100 : 255} step={space === 'CMYK' ? 0.1 : 1} decimals={2} width={120} inDialog
            value={values[i]} onCommit={(n) => setValues((v) => v.map((x, j) => (j === i ? n : x)))} /></label>
        ))}
      </div>
    </Modal>
  );
}

/** Delete Swatch: what takes its place where it was used. */
export function DeleteSwatchDialog({ swatch, swatches, edit, close }: { swatch: Swatch; swatches: Swatch[]; edit: Edit; close: () => void }) {
  const choices = swatches.filter((s) => s.id !== swatch.id && s.model !== 'gradient' && s.base !== swatch.id);
  const [replace, setReplace] = useState(choices.find((s) => s.id === 'Color/Black')?.id ?? choices[0]?.id ?? '');
  return (
    <Modal label={tr('색상 견본 삭제')} close={close} ok={!!replace} onSubmit={() => { edit(tr('색상 견본 삭제'), [{ op: 'deleteSwatch', id: swatch.id, replace }]); close(); }}>
      <p>{tr('"{0}"을(를) 지웁니다. 쓰던 곳에는 이 견본을 넣습니다:', { 0: swatch.name })}</p>
      <label>{tr('바꿀 견본')}<select aria-label={tr('바꿀 견본')} value={replace} onChange={(e) => setReplace(e.target.value)}>
        {choices.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}</select></label>
    </Modal>
  );
}
