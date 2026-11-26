// Effects panel and the control bar's opacity: the blend mode and opacity of a level of the
// selected objects (object, stroke, fill; the graphic when the content is selected). The engine
// writes them as InDesign does (edit/src/ops/blending.rs); the values come from the reader
// (pageItems `blending`).
import { t as tr } from '../i18n';
import { useState } from 'react';
import type { EditCommand } from '../engine';
import type { Blend, BlendLevel, ItemInfo, PageItems } from '../inspect';
import { trim } from '../inspect';
import { PreviewNumberField } from './controls/previewnumberfield';
import { type SelEntry, findItem } from './overlay';

export const MODES = ['Normal', 'Multiply', 'Screen', 'Overlay', 'SoftLight', 'HardLight', 'ColorDodge', 'ColorBurn',
  'Darken', 'Lighten', 'Difference', 'Exclusion', 'Hue', 'Saturation', 'Color', 'Luminosity'] as const;
export const modeLabel = (m: string) => ({
  Normal: tr('표준'), Multiply: tr('곱하기'), Screen: tr('스크린'), Overlay: tr('오버레이'), SoftLight: tr('소프트 라이트'),
  HardLight: tr('하드 라이트'), ColorDodge: tr('색상 닷지'), ColorBurn: tr('색상 번'), Darken: tr('어둡게 하기'),
  Lighten: tr('밝게 하기'), Difference: tr('차이'), Exclusion: tr('제외'), Hue: tr('색조'), Saturation: tr('채도'),
  Color: tr('색상'), Luminosity: tr('광도'),
} as Record<string, string>)[m] ?? m;
const levelLabel = (l: BlendLevel) => ({ object: tr('개체'), stroke: tr('획'), fill: tr('칠'), text: tr('텍스트'), graphic: tr('그래픽') })[l];
const NORMAL: Blend = { opacity: 100, mode: 'Normal' };

export type Target = { id: string; item: ItemInfo; graphic: boolean };

/** The selected objects, or their graphics when the content is selected (not locked). */
export function blendTargets(selection: SelEntry[], peek: (page: number) => PageItems | null | undefined): Target[] {
  return selection.map((s) => {
    const item = findItem(peek(s.page), s.id)?.item;
    return item && !item.locked ? { id: s.id, item, graphic: s.content } : null;
  }).filter((t): t is Target => !!t);
}

/** The levels the Effects panel lists for these targets. */
export const levelsFor = (targets: Target[]): BlendLevel[] =>
  targets.length && targets.every((t) => t.graphic) ? ['graphic']
    : targets.length && targets.every((t) => t.item.kind === 'textFrame') ? ['object', 'stroke', 'fill', 'text'] : ['object', 'stroke', 'fill'];

const blendOf = (t: Target, level: BlendLevel): Blend => t.item.blending?.[level] ?? (level === 'object' ? { opacity: t.item.opacity, mode: 'Normal' } : NORMAL);

/** The targets' common value at a level, or null where they differ. */
export function commonBlend(targets: Target[], level: BlendLevel): { opacity: number | null; mode: string | null } {
  const all = targets.map((t) => blendOf(t, level));
  if (!all.length) return { opacity: null, mode: null };
  return { opacity: all.every((b) => b.opacity === all[0].opacity) ? all[0].opacity : null, mode: all.every((b) => b.mode === all[0].mode) ? all[0].mode : null };
}

export const blendCommand = (targets: Target[], level: BlendLevel, change: { opacity?: number; mode?: string }): EditCommand =>
  ({ op: 'setBlending', ids: targets.map((t) => t.id), level, ...change });

/** "50%", "50", "50 %" → 50 (0–100), else null. */
export function parsePercent(text: string): number | null {
  const v = parseFloat(text.replace(',', '.').replace('%', '').trim());
  return Number.isFinite(v) && v >= 0 && v <= 100 ? v : null;
}

export function EffectsPanel({ targets, editable, edit, openEffects, options = true }: {
  options?: boolean; targets: Target[]; editable: boolean; edit: (label: string, commands: EditCommand[]) => unknown;
  openEffects?: (page: import('./effectsdialog').FxPage, level?: import('./effectsdialog').FxLevel) => void;
}) {
  const levels = levelsFor(targets);
  const [chosen, setChosen] = useState<BlendLevel>('object');
  const level = levels.includes(chosen) ? chosen : levels[0];
  const c = commonBlend(targets, level);
  const can = editable && targets.length > 0;
  const apply = (change: { opacity?: number; mode?: string }) => { if (can) edit(change.mode ? tr('혼합 모드') : tr('불투명도'), [blendCommand(targets, level, change)]); };
  return (
    <div className="p-form effects-panel">
      <div className="p-row"><span className="p-label">{tr('혼합 모드:')}</span><span className="p-ctl">
        <select aria-label={tr('혼합 모드')} value={c.mode ?? ''} disabled={!can} onChange={(e) => apply({ mode: e.target.value })}>
          {c.mode === null && <option value="" disabled />}
          {MODES.map((m) => <option key={m} value={m}>{modeLabel(m)}</option>)}
        </select></span></div>
      <div className="p-row"><span className="p-label">{tr('불투명도:')}</span><span className="p-ctl">
        <PreviewNumberField value={c.opacity} disabled={!can} label={tr('불투명도')} undoLabel={tr('불투명도')} suffix="%" decimals={1} min={0} max={100} anchor={100} scrubRowLabel
          edit={edit} build={(v) => [blendCommand(targets, level, { opacity: v })]} /></span></div>
      <div className="ef-list" role="listbox" aria-label={tr('효과 대상')}>
        {levels.map((l) => {
          const b = commonBlend(targets, l);
          return (
            <button key={l} role="option" aria-selected={l === level} className={l === level ? 'ef-row on' : 'ef-row'} onClick={() => setChosen(l)} data-level={l}
              onDoubleClick={() => { if (can && openEffects) openEffects('transparency', l); }}>
              <span>{levelLabel(l)}:</span><span>{b.mode === null ? '' : modeLabel(b.mode)} {b.opacity === null ? '' : `${trim(b.opacity, 1)}%`}</span>
            </button>
          );
        })}
      </div>
      <div className="ef-buttons">
        <button type="button" className="ef-fx" aria-label={tr('효과 추가')} title={tr('효과 추가')} disabled={!can || !openEffects} onClick={() => openEffects?.('dropShadow', level)}>fx</button>
        <button type="button" className="ef-clear" aria-label={tr('모든 효과 지우고 불투명하게')} title={tr('모든 효과를 지우고 개체를 불투명하게 만듭니다')} disabled={!can}
          onClick={() => edit(tr('모든 투명도 지우기'), [{ op: 'clearAllTransparency', ids: targets.map((t) => t.id) }])}>⌫</button>
      </div>
      {options && (['isolate', 'knockout'] as const).map((k) => {
        // the objects' own switch (mixed: unchecked, as InDesign shows a mixed selection)
        const on = targets.length > 0 && targets.every((t) => !!t.item[k]);
        const label = k === 'isolate' ? tr('혼합 격리') : tr('녹아웃 그룹');
        return <label key={k} className="p-check"><input type="checkbox" aria-label={label} checked={on} disabled={!can || targets.some((t) => t.graphic)}
          onChange={(e) => edit(label, [{ op: 'setBlending', ids: targets.map((t) => t.id), level: 'object', [k]: e.target.checked }])} /> {label}</label>;
      })}
    </div>
  );
}
