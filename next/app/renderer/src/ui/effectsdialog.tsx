// Object ▸ Effects (the Effects dialog): opacity and blend mode and every effect of one level of
// the selected objects. The engine reads the settings (`effects`: InDesign's defaults where the
// object has none, edit/src/ops/effects.rs) and on OK writes only what changed, as one undo step
// (setBlending, setEffect). Settings are named as IDML names them; this file only lays them out.
import { t as tr } from '../i18n';
import { useEffect, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';
import { trim } from '../inspect';
import { MODES, modeLabel, type Target } from './effects';
import { NumberField } from './controls/numberfield';
import { stepOfLength } from './controls/lengthfield';

export type EffectName = 'dropShadow' | 'innerShadow' | 'outerGlow' | 'innerGlow' | 'bevelAndEmboss' | 'satin' | 'basicFeather' | 'directionalFeather' | 'gradientFeather';
export type FxLevel = 'object' | 'stroke' | 'fill' | 'text' | 'graphic';
export type FxPage = 'transparency' | EffectName;
type Value = number | string | boolean | number[];
type Values = Record<string, Value>;
type LevelView = { opacity: number; mode: string; effects: Record<EffectName, { on: boolean; values: Values }> };
export type EffectsView = { levels: Partial<Record<FxLevel, LevelView>>; globalLight: { angle: number; altitude: number } };

/** The effects in the dialog's order. */
export const EFFECTS: EffectName[] = ['dropShadow', 'innerShadow', 'outerGlow', 'innerGlow', 'bevelAndEmboss', 'satin', 'basicFeather', 'directionalFeather', 'gradientFeather'];
export const effectLabel = (e: FxPage) => ({
  transparency: tr('투명도'), dropShadow: tr('그림자'), innerShadow: tr('내부 그림자'), outerGlow: tr('외부 광선'), innerGlow: tr('내부 광선'),
  bevelAndEmboss: tr('경사와 엠보스'), satin: tr('새틴'), basicFeather: tr('기본 페더'), directionalFeather: tr('방향 페더'), gradientFeather: tr('그레이디언트 페더'),
})[e];
/** The levels in the dialog's order. */
const LEVELS: FxLevel[] = ['object', 'stroke', 'fill', 'text', 'graphic'];
const levelLabel = (l: FxLevel) => ({ object: tr('개체'), stroke: tr('획'), fill: tr('칠'), text: tr('텍스트'), graphic: tr('그래픽') })[l];

type Field =
  | { key: string; label: () => string; kind: 'mode' | 'color' | 'bool' }
  | { key: string; label: () => string; kind: 'pct' | 'len' | 'angle'; min: number; max: number }
  | { key: string; label: () => string; kind: 'pick'; options: () => [string, string][] };
type Group = { title: () => string; fields: Field[] };

const mode = (key = 'BlendMode'): Field => ({ key, label: () => tr('모드:'), kind: 'mode' });
const color = (key = 'EffectColor'): Field => ({ key, label: () => tr('색상:'), kind: 'color' });
const pct = (key: string, label: () => string, min = 0, max = 100): Field => ({ key, label, kind: 'pct', min, max });
const len = (key: string, label: () => string, min = 0, max = 1000): Field => ({ key, label, kind: 'len', min, max });
const angle = (key = 'Angle', label = () => tr('각도:')): Field => ({ key, label, kind: 'angle', min: -360, max: 360 });
const bool = (key: string, label: () => string): Field => ({ key, label, kind: 'bool' });
const pick = (key: string, label: () => string, options: () => [string, string][]): Field => ({ key, label, kind: 'pick', options });
const blending = (opacity = 'Opacity'): Group => ({ title: () => tr('혼합'), fields: [mode(), color(), pct(opacity, () => tr('불투명도:'))] });
const technique = () => pick('Technique', () => tr('기법:'), () => [['Softer', tr('더 부드럽게')], ['Precise', tr('정밀하게')]]);

/** Each effect's settings as the dialog groups them. */
const LAYOUT: Record<EffectName, Group[]> = {
  dropShadow: [blending(),
    { title: () => tr('위치'), fields: [len('Distance', () => tr('거리:')), angle(), bool('UseGlobalLight', () => tr('전체 조명 사용')),
      len('XOffset', () => tr('X 오프셋:'), -1000), len('YOffset', () => tr('Y 오프셋:'), -1000)] },
    { title: () => tr('옵션'), fields: [len('Size', () => tr('크기:')), pct('Spread', () => tr('스프레드:')), pct('Noise', () => tr('노이즈:')),
      bool('KnockedOut', () => tr('개체가 그림자를 녹아웃')), bool('HonorOtherEffects', () => tr('그림자가 다른 효과를 따름'))] }],
  innerShadow: [blending(),
    { title: () => tr('위치'), fields: [len('Distance', () => tr('거리:')), angle(), bool('UseGlobalLight', () => tr('전체 조명 사용'))] },
    { title: () => tr('옵션'), fields: [len('Size', () => tr('크기:')), pct('ChokeAmount', () => tr('경계 감소:')), pct('Noise', () => tr('노이즈:'))] }],
  outerGlow: [blending(),
    { title: () => tr('옵션'), fields: [technique(), len('Size', () => tr('크기:')), pct('Noise', () => tr('노이즈:')), pct('Spread', () => tr('스프레드:'))] }],
  innerGlow: [blending(),
    { title: () => tr('옵션'), fields: [technique(), pick('Source', () => tr('소스:'), () => [['CenterSourced', tr('가운데')], ['EdgeSourced', tr('가장자리')]]),
      len('Size', () => tr('크기:')), pct('Spread', () => tr('경계 감소:')), pct('Noise', () => tr('노이즈:'))] }],
  bevelAndEmboss: [
    { title: () => tr('구조'), fields: [
      pick('Style', () => tr('스타일:'), () => [['InnerBevel', tr('내부 경사')], ['OuterBevel', tr('외부 경사')], ['Emboss', tr('엠보스')], ['PillowEmboss', tr('쿠션 엠보스')]]),
      len('Size', () => tr('크기:')),
      pick('Technique', () => tr('기법:'), () => [['SmoothContour', tr('매끄럽게')], ['ChiselHard', tr('강하게 깎기')], ['ChiselSoft', tr('부드럽게 깎기')]]),
      len('Soften', () => tr('부드럽게:')), pick('Direction', () => tr('방향:'), () => [['Up', tr('위로')], ['Down', tr('아래로')]]),
      pct('Depth', () => tr('깊이:'), 1, 1000)] },
    { title: () => tr('음영'), fields: [angle(), { key: 'Altitude', label: () => tr('고도:'), kind: 'angle', min: 0, max: 90 }, bool('UseGlobalLight', () => tr('전체 조명 사용'))] },
    { title: () => tr('강조'), fields: [mode('HighlightBlendMode'), color('HighlightColor'), pct('HighlightOpacity', () => tr('불투명도:'))] },
    { title: () => tr('그림자'), fields: [mode('ShadowBlendMode'), color('ShadowColor'), pct('ShadowOpacity', () => tr('불투명도:'))] }],
  satin: [blending(),
    { title: () => tr('구조'), fields: [angle(), len('Distance', () => tr('거리:')), len('Size', () => tr('크기:')), bool('InvertEffect', () => tr('반전'))] }],
  basicFeather: [
    { title: () => tr('옵션'), fields: [len('Width', () => tr('페더 폭:')), pct('ChokeAmount', () => tr('경계 감소:')),
      pick('CornerType', () => tr('모퉁이:'), () => [['Sharp', tr('선명하게')], ['Rounded', tr('둥글게')], ['Diffusion', tr('확산')]]), pct('Noise', () => tr('노이즈:'))] }],
  directionalFeather: [
    { title: () => tr('페더 폭'), fields: [len('TopWidth', () => tr('위쪽:')), len('BottomWidth', () => tr('아래쪽:')), len('LeftWidth', () => tr('왼쪽:')), len('RightWidth', () => tr('오른쪽:'))] },
    { title: () => tr('옵션'), fields: [pct('Noise', () => tr('노이즈:')), pct('ChokeAmount', () => tr('경계 감소:')),
      pick('FollowShapeMode', () => tr('모양:'), () => [['LeadingEdge', tr('선행 가장자리만')], ['AllEdges', tr('모든 가장자리')], ['FirstEdge', tr('첫 번째 가장자리만')]]), angle()] }],
  gradientFeather: [
    { title: () => tr('옵션'), fields: [pick('Type', () => tr('유형:'), () => [['Linear', tr('선형')], ['Radial', tr('방사형')]]), angle()] }],
};

/** A drop shadow's offsets from its distance and angle (the light's), and back. */
const offsets = (d: number, a: number) => [-d * Math.cos((a * Math.PI) / 180), d * Math.sin((a * Math.PI) / 180)];
const polar = (x: number, y: number) => [Math.hypot(x, y), (Math.atan2(y, -x) * 180) / Math.PI];

const same = (a: Value | undefined, b: Value | undefined) => JSON.stringify(a) === JSON.stringify(b);

/**
 * A number box (the shared number field): Enter or leaving it applies, arrows, the ▲▼ buttons and the wheel step it; Esc takes the typing
 * back and does not close the dialog; an entry outside the range is held to the range.
 */
function Num({ value, kind, min, max, format, parse, label, disabled, onChange }: {
  value: number; kind: 'pct' | 'len' | 'angle'; min: number; max: number; label: string; disabled?: boolean;
  format: (pt: number) => string; parse: (text: string) => number | null; onChange: (v: number) => void;
}) {
  const read = (text: string) => {
    const v = kind === 'len' ? parse(text) : parseFloat(text.replace(',', '.').replace(/[%°]/g, '').trim());
    return v !== null && Number.isFinite(v) ? Math.min(max, Math.max(min, v)) : null;
  };
  const write = (v: number) => (kind === 'len' ? format(v) : kind === 'pct' ? `${trim(v, 1)}%` : `${trim(v, 2)}°`);
  // inside the dialog's form: Enter applies the entry only (it does not apply the dialog), Esc does not close the dialog
  return <span className="fx-num" onKeyDown={(e) => { if (e.key === 'Escape') e.stopPropagation(); if (e.key === 'Enter') e.preventDefault(); }}>
    <NumberField value={value} onCommit={onChange} min={min} max={max} step={kind === 'len' ? stepOfLength(format, parse) : 1} label={label} disabled={disabled}
      format={write} parse={read} width={220} inDialog />
  </span>;
}

export function EffectsDialog({ targets, info, start, startLevel, format, parse, edit, close }: {
  targets: Target[]; info: DocumentInfo | null; start: FxPage; startLevel?: FxLevel;
  format: (pt: number) => string; parse: (text: string) => number | null;
  edit: (label: string, commands: EditCommand[]) => unknown; close: () => void;
}) {
  const [view, setView] = useState<EffectsView | null>(null);
  const [draft, setDraft] = useState<EffectsView['levels'] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [page, setPage] = useState<FxPage>(start);
  const [light, setLight] = useState<{ angle: number; altitude: number } | null>(null);
  const [level, setLevel] = useState<FxLevel>(startLevel ?? (targets.every((t) => t.graphic) ? 'graphic' : 'object'));
  const first = targets[0]?.id;
  useEffect(() => {
    if (!first) return;
    window.folio.request({ cmd: 'effects', item: first }).then(({ header }) => {
      if (!header.ok) { setError(String(header.error ?? 'effects')); return; }
      const v = header.effects as EffectsView;
      setView(v);
      setLight(v.globalLight);
      // Object ▸ Effects ▸ Drop Shadow… opens with the drop shadow on
      const d = structuredClone(v.levels);
      const l = d[level];
      if (start !== 'transparency' && l) l.effects[start].on = true;
      setDraft(d);
    });
  }, [first]);
  const swatches = useMemo(() => (info?.swatches ?? []).filter((s) => s.model !== 'none' && s.model !== 'gradient' && s.id !== 'Swatch/None'), [info]);
  const graphicOnly = targets.every((t) => t.graphic);
  const levels = LEVELS.filter((l) => draft?.[l] && (graphicOnly ? l === 'graphic' : l !== 'graphic'));
  const cur = draft?.[level];
  const setLevelView = (f: (l: LevelView) => LevelView) => setDraft((d) => (d && d[level] ? { ...d, [level]: f(d[level]!) } : d));
  const setValue = (effect: EffectName, key: string, v: Value) => setLevelView((l) => {
    const e = l.effects[effect];
    const values: Values = { ...e.values, [key]: v };
    // a drop shadow's distance and angle and its offsets are one setting
    if (effect === 'dropShadow' && (key === 'Distance' || key === 'Angle')) [values.XOffset, values.YOffset] = offsets(values.Distance as number, values.Angle as number);
    if (effect === 'dropShadow' && (key === 'XOffset' || key === 'YOffset')) [values.Distance, values.Angle] = polar(values.XOffset as number, values.YOffset as number);
    // changing a setting switches the effect on, as choosing its name does
    return { ...l, effects: { ...l.effects, [effect]: { on: true, values } } };
  });
  const setOn = (effect: EffectName, on: boolean) => setLevelView((l) => ({ ...l, effects: { ...l.effects, [effect]: { ...l.effects[effect], on } } }));

  const commands = (): EditCommand[] => {
    if (!view || !draft) return [];
    const ids = targets.map((t) => t.id);
    const out: EditCommand[] = [];
    for (const l of Object.keys(draft) as FxLevel[]) {
      const a = view.levels[l], b = draft[l];
      if (!a || !b) continue;
      if (a.opacity !== b.opacity || a.mode !== b.mode) {
        out.push({ op: 'setBlending', ids, level: l, ...(a.opacity !== b.opacity ? { opacity: b.opacity } : {}), ...(a.mode !== b.mode ? { mode: b.mode } : {}) });
      }
      for (const e of EFFECTS) {
        const x = a.effects[e], y = b.effects[e];
        const values: Values = {};
        for (const k of Object.keys(y.values)) if (!same(x.values[k], y.values[k])) values[k] = y.values[k];
        if (e === 'dropShadow') {
          // the engine works the offsets out of a distance and angle the user typed
          if ('XOffset' in values || 'YOffset' in values) { delete values.Distance; delete values.Angle; }
          else if ('Distance' in values || 'Angle' in values) { delete values.XOffset; delete values.YOffset; }
        }
        if (x.on !== y.on || Object.keys(values).length) {
          out.push({ op: 'setEffect', ids, level: l, effect: e, ...(x.on !== y.on ? { on: y.on } : {}), values });
        }
      }
    }
    return out;
  };
  const submit = () => {
    const c = commands();
    // an angle edited with the global light on changes the document's light, as in InDesign
    if (view && light && (light.angle !== view.globalLight.angle || light.altitude !== view.globalLight.altitude)) {
      c.push({ op: 'setGlobalLight', ...(light.angle !== view.globalLight.angle ? { angle: light.angle } : {}), ...(light.altitude !== view.globalLight.altitude ? { altitude: light.altitude } : {}) });
    }
    if (c.length) edit(tr('효과'), c);
    close();
  };

  const field = (effect: EffectName, f: Field, values: Values) => {
    const v = values[f.key];
    const global = !!values.UseGlobalLight && (f.key === 'Angle' || f.key === 'Altitude') && effect !== 'satin';
    const label = f.label();
    if (f.kind === 'bool') {
      return <label key={f.key} className="check"><input type="checkbox" aria-label={label.replace(/:$/, '')} checked={!!v} onChange={(e) => setValue(effect, f.key, e.target.checked)} /> {label}</label>;
    }
    let ctl;
    if (f.kind === 'mode') {
      ctl = <select aria-label={`${effectLabel(effect)} ${label.replace(/:$/, '')} (${f.key})`} value={String(v)} onChange={(e) => setValue(effect, f.key, e.target.value)}>
        {MODES.map((m) => <option key={m} value={m}>{modeLabel(m)}</option>)}</select>;
    } else if (f.kind === 'color') {
      const known = swatches.some((s) => s.id === v);
      ctl = <select aria-label={`${effectLabel(effect)} ${label.replace(/:$/, '')} (${f.key})`} value={String(v)} onChange={(e) => setValue(effect, f.key, e.target.value)}>
        {!known && <option value={String(v)} disabled>{String(v).replace(/^\w+\//, '')}</option>}
        {swatches.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}</select>;
    } else if (f.kind === 'pick') {
      ctl = <select aria-label={`${effectLabel(effect)} ${label.replace(/:$/, '')}`} value={String(v)} onChange={(e) => setValue(effect, f.key, e.target.value)}>
        {f.options().map(([k, name]) => <option key={k} value={k}>{name}</option>)}</select>;
    } else {
      // with the global light the angle (and a bevel's altitude) is the document's light
      const n = f as Extract<Field, { min: number }>;
      const key = f.key === 'Angle' ? 'angle' : 'altitude';
      const shownValue = global && light ? light[key] : (v as number);
      ctl = <Num value={shownValue} kind={n.kind} min={global ? (key === 'angle' ? -180 : 0) : n.min} max={global ? (key === 'angle' ? 180 : 90) : n.max} format={format} parse={parse}
        label={`${effectLabel(effect)} ${label.replace(/:$/, '')}`}
        onChange={(x) => (global ? setLight((l) => (l ? { ...l, [key]: x } : l)) : setValue(effect, f.key, x))} />;
    }
    return <label key={f.key}>{label}{ctl}</label>;
  };

  const summary = (l: FxLevel) => {
    const lv = draft?.[l];
    if (!lv) return '';
    const on = EFFECTS.filter((e) => lv.effects[e].on).map(effectLabel);
    return `${modeLabel(lv.mode)} ${trim(lv.opacity, 1)}%${on.length ? `; ${on.join(', ')}` : ''}`;
  };

  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('효과')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options effects-dialog" onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <h2>{tr('효과')}</h2>
        {error && <p className="error">{error}</p>}
        {draft && cur && (
          <>
            <label className="fx-level">{tr('설정 대상:')}<select aria-label={tr('설정 대상')} value={level} onChange={(e) => setLevel(e.target.value as FxLevel)}>
              {levels.map((l) => <option key={l} value={l}>{levelLabel(l)}</option>)}</select></label>
            <div className="fx-body">
              <div className="fx-list" role="listbox" aria-label={tr('효과 목록')}>
                <button type="button" role="option" aria-selected={page === 'transparency'} className={page === 'transparency' ? 'on' : ''} onClick={() => setPage('transparency')}>{effectLabel('transparency')}</button>
                {EFFECTS.map((e) => (
                  <div key={e} className={page === e ? 'fx-item on' : 'fx-item'}>
                    <input type="checkbox" aria-label={`${effectLabel(e)} ${tr('켜기')}`} checked={cur.effects[e].on} onChange={(ev) => { setOn(e, ev.target.checked); setPage(e); }} />
                    <button type="button" role="option" aria-selected={page === e} onClick={() => { setPage(e); setOn(e, true); }} data-effect={e}>{effectLabel(e)}</button>
                  </div>
                ))}
                <div className="fx-summary">{levels.map((l) => <div key={l}><b>{levelLabel(l)}:</b> {summary(l)}</div>)}</div>
              </div>
              <div className="fx-settings">
                <h3>{effectLabel(page)}</h3>
                {page === 'transparency' ? (
                  <>
                    <fieldset><legend>{tr('기본 혼합')}</legend>
                      <label>{tr('모드:')}<select aria-label={tr('혼합 모드')} value={cur.mode} onChange={(e) => setLevelView((l) => ({ ...l, mode: e.target.value }))}>
                        {MODES.map((m) => <option key={m} value={m}>{modeLabel(m)}</option>)}</select></label>
                      <label>{tr('불투명도:')}<Num value={cur.opacity} kind="pct" min={0} max={100} format={format} parse={parse} label={tr('불투명도')}
                        onChange={(v) => setLevelView((l) => ({ ...l, opacity: v }))} /></label>
                    </fieldset>
                    {level === 'object' && (
                      <fieldset><legend>{tr('옵션')}</legend>
                        <label className="check" title={tr('지원하지 않는 옵션입니다.')}><input type="checkbox" disabled /> {tr('혼합 격리')}</label>
                        <label className="check" title={tr('지원하지 않는 옵션입니다.')}><input type="checkbox" disabled /> {tr('녹아웃 그룹')}</label>
                      </fieldset>
                    )}
                  </>
                ) : (
                  <>
                    {LAYOUT[page].map((g, k) => (
                      <fieldset key={k}><legend>{g.title()}</legend>{g.fields.map((f) => field(page, f, cur.effects[page].values))}</fieldset>
                    ))}
                    {page === 'gradientFeather' && <p className="muted" title={tr('지원하지 않는 옵션입니다.')}>{tr('그레이디언트 정지점')}</p>}
                  </>
                )}
              </div>
            </div>
          </>
        )}
        <div className="dialog-actions">
          <label className="check" title={tr('지원하지 않는 옵션입니다.')}><input type="checkbox" disabled /> {tr('미리 보기')}</label>
          <button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!draft}>{tr('확인')}</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}

/** Object ▸ Effects ▸ Global Light: the document's light angle and altitude. */
export function GlobalLightDialog({ edit, close }: { edit: (label: string, commands: EditCommand[]) => unknown; close: () => void }) {
  const [light, setLight] = useState<{ angle: number; altitude: number } | null>(null);
  const [was, setWas] = useState<{ angle: number; altitude: number } | null>(null);
  useEffect(() => {
    window.folio.request({ cmd: 'effects' }).then(({ header }) => {
      if (header.ok) { const g = (header.effects as EffectsView).globalLight; setLight(g); setWas(g); }
    });
  }, []);
  const submit = () => {
    if (light && was && (light.angle !== was.angle || light.altitude !== was.altitude)) {
      edit(tr('전체 조명'), [{ op: 'setGlobalLight', ...(light.angle !== was.angle ? { angle: light.angle } : {}), ...(light.altitude !== was.altitude ? { altitude: light.altitude } : {}) }]);
    }
    close();
  };
  const fmt = (v: number) => String(v);
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('전체 조명')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options global-light" onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <h2>{tr('전체 조명')}</h2>
        {light && (
          <>
            <label>{tr('각도:')}<Num value={light.angle} kind="angle" min={-180} max={180} format={fmt} parse={() => null} label={tr('전체 조명 각도')} onChange={(v) => setLight({ ...light, angle: v })} /></label>
            <label>{tr('고도:')}<Num value={light.altitude} kind="angle" min={0} max={90} format={fmt} parse={() => null} label={tr('전체 조명 고도')} onChange={(v) => setLight({ ...light, altitude: v })} /></label>
          </>
        )}
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!light}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
