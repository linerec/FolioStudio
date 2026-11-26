import { useEffect, useState } from 'react';
import { t as tr } from '../i18n';
import type { TextProperties } from '../engine';
import type { Command } from './commands';
import type { TextFormatting } from './textformat';
import { fieldKey } from './keys';
import { NumberField } from './controls/numberfield';
import { STROKE_TYPES } from './stroke';
import './charlines.css';

type LineKind = 'underline' | 'strikeThrough';
export function useCharacterLines(control: TextFormatting, documentPath?: string) {
  const [open, setOpen] = useState<LineKind | null>(null);
  useEffect(() => setOpen(null), [documentPath]);
  const handlers: Record<string, Partial<Command>> = {
    'id.panel.character.underlineOptions': { run: control.selection ? () => setOpen('underline') : undefined },
    'id.panel.character.strikethroughOptions': { run: control.selection ? () => setOpen('strikeThrough') : undefined },
  };
  return { handlers, dialog: open && <LineDialog key={open} kind={open} control={control} close={() => setOpen(null)} /> };
}

function LineDialog({ kind, control, close }: { kind: LineKind; control: TextFormatting; close: () => void }) {
  const initial = control.selection?.format ?? {};
  const [draft, setDraft] = useState<Record<string, string>>({});
  const title = kind === 'underline' ? tr('밑줄 옵션') : tr('취소선 옵션');
  const value = (key: string) => draft[key] ?? (initial[key] == null ? '' : String(initial[key]));
  const change = (key: string, v: string) => setDraft(d => ({ ...d, [key]: v }));
  const toggle = (key: string, label: string) => <label>{label}<select aria-label={label} data-property={key}
    value={value(key)} onChange={e => change(key, e.target.value)}>
    <option value="">{tr('혼합')}</option><option value="true">{tr('켜짐')}</option><option value="false">{tr('꺼짐')}</option>
  </select></label>;
  const number = (suffix: string, label: string, automatic: number) => {
    const key = kind + suffix, v = value(key), isAuto = v === String(automatic);
    const n = v === '' || isAuto ? null : Number(v), tint = suffix.endsWith('Tint');
    return <label>{label}<span className="charline-number"><NumberField value={n !== null && Number.isFinite(n) ? n : null} label={label} inputData={{ 'data-property': key }}
      placeholder={isAuto ? tr('자동') : tr('혼합')} min={tint ? -1 : -1e5} max={tint ? 100 : 1e5} step={1} decimals={4} width={88} inDialog
      onCommit={x => change(key, String(x))} />
      <button type="button" onClick={() => change(key, String(automatic))}>{tr('자동')}</button></span></label>;
  };
  const color = (suffix: string, label: string) => {
    const key = kind + suffix, v = value(key);
    const colors = [...(suffix === 'Color' ? [{ id: 'Text Color', name: tr('텍스트 색상') }] : []), ...(control.info?.swatches ?? [])];
    return <label>{label}<select aria-label={label} data-property={key} value={v} onChange={e => change(key, e.target.value)}>
      <option value="">{tr('혼합')}</option>{v && !colors.some(c => c.id === v) && <option value={v}>{v}</option>}
      {colors.map(c => <option key={c.id} value={c.id}>{c.name}</option>)}
    </select></label>;
  };
  const valid = Object.entries(draft).every(([key, raw]) => {
    if (!raw) return true;
    if (/Weight$|Offset$|Tint$/.test(key)) {
      const n = Number(raw);
      return Number.isFinite(n) && (!key.endsWith('Tint') || n === -1 || n >= 0 && n <= 100);
    }
    return true;
  });
  const typeKey = kind + 'Type', stroke = value(typeKey);
  // the document's own dashed / dotted / striped stroke styles (Stroke Styles dialog), after the built-in ones
  const custom = (control.info?.strokeStyles ?? []).filter(s => s.kind !== 'StrokeStyle');
  return <div className="style-modal" role="dialog" aria-modal="true" aria-label={title}
    onPointerDown={e => e.stopPropagation()} onKeyDown={e => {
      e.stopPropagation(); if (fieldKey(e) === 'cancel') { e.preventDefault(); close(); }
    }}>
    <form className="card dialog style-options charlines" onSubmit={e => {
      e.preventDefault(); if (!valid) return;
      const props: TextProperties = {};
      for (const [key, raw] of Object.entries(draft)) if (raw !== '') {
        props[key] = /Weight$|Offset$|Tint$/.test(key) ? Number(raw) : raw === 'true' ? true : raw === 'false' ? false : raw;
      }
      if (Object.keys(props).length) control.apply(props);
      close();
    }}>
      <h2>{title}</h2>
      {toggle(kind === 'underline' ? 'underline' : 'strikeThru', tr('선 켜기'))}
      <label>{tr('선 종류')}<select aria-label={tr('선 종류')} data-property={typeKey} value={stroke} onChange={e => change(typeKey, e.target.value)}>
        <option value="">{tr('혼합')}</option>{stroke && !STROKE_TYPES.some(([id]) => stroke === id) && !custom.some(s => s.id === stroke) && <option value={stroke}>{stroke}</option>}
        {STROKE_TYPES.map(([id, label]) => <option key={id} value={id}>{label()}</option>)}
        {custom.map(s => <option key={s.id} value={s.id}>{s.name}</option>)}
      </select></label>
      {number('Weight', tr('선 두께 (pt)'), -9999)}
      {number('Offset', tr('기준선 간격 (pt)'), -9999)}
      {color('Color', tr('선 색상'))}
      {number('Tint', tr('선 농도 (%)'), -1)}
      {toggle(kind + 'Overprint', tr('선 겹치기'))}
      {color('GapColor', tr('간격 색상'))}
      {number('GapTint', tr('간격 농도 (%)'), -1)}
      {toggle(kind + 'GapOverprint', tr('간격 겹치기'))}
      <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button>
        <button type="submit" disabled={!valid}>{tr('확인')}</button></div>
    </form>
  </div>;
}
