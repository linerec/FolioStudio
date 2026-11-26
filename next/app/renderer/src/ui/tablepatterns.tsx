// Table Options ▸ Row Strokes, Column Strokes and Fills: the alternating patterns of a table. A pattern is the counts of rows (or
// columns) with the start settings and then with the end settings, repeated; the table keeps each setting as an attribute
// (edit/src/ops/tableopts.rs, oracle tables-alternating.json). The dialog shows the values the table has now and sends the ones
// that changed as one `setTableOptions`. Settings a cell sets itself are kept (InDesign's "Preserve Local Formatting").
import { t as tr } from '../i18n';
import type { DocumentInfo } from '../inspect';
import { STROKE_TYPES } from './stroke';
import { TextNumberField, formatNumeric, parseNumeric } from './controls/lengthfield';

export type PatternTab = 'rowStrokes' | 'columnStrokes' | 'fills';
export type PatternValues = Record<string, string>;

type Dim = 'Row' | 'Column';
const SIDES = ['Start', 'End'] as const;

/** The IDML attributes of a stroke group (a start or end of a row or column pattern). */
const strokeKeys = (dim: Dim, side: typeof SIDES[number]) => {
  const p = `${side}${dim}Stroke`;
  return { count: `${p}Count`, weight: `${p}Weight`, type: side === 'End' && dim === 'Column' ? 'EndColumnLineStyle' : `${p}Type`, color: `${p}Color`, tint: `${p}Tint`,
    overprint: `${p}Overprint`, gapColor: `${p}GapColor`, gapTint: `${p}GapTint`, gapOverprint: `${p}GapOverprint` };
};
const fillKeys = (dim: Dim, side: typeof SIDES[number]) => {
  const p = `${side}${dim}Fill`;
  return { count: `${p}Count`, color: `${p}Color`, tint: `${p}Tint`, overprint: `${p}Overprint` };
};
const skipKeys = (what: 'Stroke' | 'Fill', dim: Dim) => [`SkipFirstAlternating${what}${dim}s`, `SkipLastAlternating${what}${dim}s`] as const;

/** Every attribute a pattern tab edits. */
export const patternKeys = (tab: PatternTab): string[] => {
  if (tab === 'fills') {
    return ['ColumnFillsPriority', ...(['Row', 'Column'] as Dim[]).flatMap((d) => [...SIDES.flatMap((s) => Object.values(fillKeys(d, s))), ...skipKeys('Fill', d)])];
  }
  const d: Dim = tab === 'rowStrokes' ? 'Row' : 'Column';
  return [...SIDES.flatMap((s) => Object.values(strokeKeys(d, s))), ...skipKeys('Stroke', d)];
};

const whole = (text: string, max: number) => { const n = Number(text); return text.trim() !== '' && Number.isInteger(n) && n >= 0 && n <= max; };
const decimal = (text: string, max: number) => { const n = Number(text); return text.trim() !== '' && Number.isFinite(n) && n >= 0 && n <= max; };

/** The text of every setting of `tab` is acceptable to the engine. */
export function patternsValid(tab: PatternTab, v: PatternValues): boolean {
  return patternKeys(tab).every((k) => {
    const x = v[k] ?? '';
    if (/Count$|^Skip/.test(k)) return whole(x, 999);
    if (/Weight$/.test(k)) return decimal(x, 800);
    if (/Tint$/.test(k)) return decimal(x, 100);
    return x !== '';
  });
}

/** The settings of `tab` that differ from `initial`, as the engine takes them (numbers written plainly). */
export function patternChanges(tab: PatternTab, initial: PatternValues, v: PatternValues): PatternValues {
  const out: PatternValues = {};
  for (const k of patternKeys(tab)) {
    const now = /Count$|^Skip|Weight$|Tint$/.test(k) ? String(Number(v[k])) : v[k];
    if (now !== initial[k] && !(/Count$|^Skip|Weight$|Tint$/.test(k) && Number(initial[k]) === Number(v[k]))) out[k] = now;
  }
  return out;
}

const PRESETS = ['none', '1', '2', '3', 'custom'] as const;

type Swatch = DocumentInfo['swatches'][number];

function SwatchPick({ label, value, swatches, disabled, set }: { label: string; value: string; swatches: Swatch[]; disabled: boolean; set: (v: string) => void }) {
  return (
    <select aria-label={label} value={value} disabled={disabled} onChange={(e) => set(e.target.value)}>
      {!swatches.some((s) => s.id === value) && <option value={value}>{value.replace(/^(Color|Swatch)\//, '')}</option>}
      {swatches.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
    </select>
  );
}

/** A whole number as typed, or null. */
const parseWhole = (text: string) => { const n = parseNumeric(text); return n !== null && Number.isInteger(n) ? n : null; };

type Props = { values: PatternValues; set: (key: string, value: string) => void; swatches: Swatch[] };

/** A group of settings (the start, or the end, of a pattern): how many rows (columns), the paint. */
function StrokeGroup({ title, unit, keys, values, set, swatches, off }: Props & { title: string; unit: string; keys: ReturnType<typeof strokeKeys>; off: boolean }) {
  // a number box of the pattern (the shared number field; the dialog keeps the plain number as text): the count is whole, the weight is in
  // points (to 800), the tints are percents (to 100)
  const f = (key: string, label: string, max: number, whole = false) => (
    <TextNumberField width={150} label={label} text={values[key] ?? ''} onText={(t) => set(key, t)} format={formatNumeric} parse={whole ? parseWhole : parseNumeric}
      min={0} max={max} step={1} decimals={whole ? 0 : 4} disabled={off} />
  );
  return (
    <fieldset><legend>{title}</legend>
      <label>{unit}{f(keys.count, tr('개수'), 999, true)}</label>
      <label>{tr('두께:')}{f(keys.weight, tr('두께'), 800)}</label>
      <label>{tr('유형:')}
        <select aria-label={tr('획 유형')} value={values[keys.type] ?? ''} disabled={off} onChange={(e) => set(keys.type, e.target.value)}>
          {!STROKE_TYPES.some(([id]) => id === values[keys.type]) && <option value={values[keys.type]}>{(values[keys.type] ?? '').replace('StrokeStyle/$ID/', '')}</option>}
          {STROKE_TYPES.map(([id, label]) => <option key={id} value={id}>{label()}</option>)}
        </select>
      </label>
      <label>{tr('색상:')}<SwatchPick label={tr('획 색상')} value={values[keys.color] ?? ''} swatches={swatches} disabled={off} set={(v) => set(keys.color, v)} /></label>
      <label>{tr('색조:')}{f(keys.tint, tr('획 색조'), 100)}</label>
      <label className="check"><input type="checkbox" aria-label={tr('획 오버프린트')} checked={values[keys.overprint] === 'true'} disabled={off} onChange={(e) => set(keys.overprint, String(e.target.checked))} /> {tr('획 오버프린트')}</label>
      <label>{tr('간격 색상:')}<SwatchPick label={tr('간격 색상')} value={values[keys.gapColor] ?? ''} swatches={swatches} disabled={off} set={(v) => set(keys.gapColor, v)} /></label>
      <label>{tr('간격 색조:')}{f(keys.gapTint, tr('간격 색조'), 100)}</label>
      <label className="check"><input type="checkbox" aria-label={tr('간격 오버프린트')} checked={values[keys.gapOverprint] === 'true'} disabled={off} onChange={(e) => set(keys.gapOverprint, String(e.target.checked))} /> {tr('간격 오버프린트')}</label>
    </fieldset>
  );
}

function FillGroup({ title, unit, keys, values, set, swatches, off }: Props & { title: string; unit: string; keys: ReturnType<typeof fillKeys>; off: boolean }) {
  return (
    <fieldset><legend>{title}</legend>
      <label>{unit}<TextNumberField width={150} label={tr('개수')} text={values[keys.count] ?? ''} onText={(t) => set(keys.count, t)} format={formatNumeric} parse={parseWhole} min={0} max={999} step={1} decimals={0} disabled={off} /></label>
      <label>{tr('색상:')}<SwatchPick label={tr('칠 색상')} value={values[keys.color] ?? ''} swatches={swatches} disabled={off} set={(v) => set(keys.color, v)} /></label>
      <label>{tr('색조:')}<TextNumberField width={150} label={tr('칠 색조')} text={values[keys.tint] ?? ''} onText={(t) => set(keys.tint, t)} format={formatNumeric} parse={parseNumeric} min={0} max={100} step={1} disabled={off} /></label>
      <label className="check"><input type="checkbox" aria-label={tr('칠 오버프린트')} checked={values[keys.overprint] === 'true'} disabled={off} onChange={(e) => set(keys.overprint, String(e.target.checked))} /> {tr('칠 오버프린트')}</label>
    </fieldset>
  );
}

/** The pattern a pair of counts is: none, every other / second / third, or custom. */
function presetOf(start: string, end: string): typeof PRESETS[number] {
  if (Number(start) === 0 && Number(end) === 0) return 'none';
  if (Number(start) === Number(end) && [1, 2, 3].includes(Number(start))) return String(Number(start)) as typeof PRESETS[number];
  return 'custom';
}

const PRESET_NAMES = (rows: boolean) => ({
  none: tr('없음'),
  '1': rows ? tr('한 행 걸러') : tr('한 열 걸러'),
  '2': rows ? tr('두 행 걸러') : tr('두 열 걸러'),
  '3': rows ? tr('세 행 걸러') : tr('세 열 걸러'),
  custom: rows ? tr('사용자 정의 행') : tr('사용자 정의 열'),
});

/** Row Strokes or Column Strokes. */
export function StrokePatterns({ dim, values, set, swatches }: Props & { dim: Dim }) {
  const rows = dim === 'Row';
  const [start, end] = [strokeKeys(dim, 'Start'), strokeKeys(dim, 'End')];
  const preset = presetOf(values[start.count], values[end.count]);
  const [skipFirst, skipLast] = skipKeys('Stroke', dim);
  const pick = (p: string) => {
    if (p === 'none') { set(start.count, '0'); set(end.count, '0'); return; }
    if (p === 'custom') { if (preset === 'none') { set(start.count, '1'); set(end.count, '1'); } return; }
    set(start.count, p); set(end.count, p);
  };
  const names = PRESET_NAMES(rows);
  const off = preset === 'none';
  const unit = rows ? tr('행 수:') : tr('열 수:');
  return (
    <>
      <fieldset><legend>{rows ? tr('행 획 교대 패턴') : tr('열 획 교대 패턴')}</legend>
        <label>{tr('교대 패턴:')}
          <select aria-label={tr('교대 패턴')} value={preset} onChange={(e) => pick(e.target.value)}>
            {PRESETS.map((p) => <option key={p} value={p}>{names[p]}</option>)}
          </select>
        </label>
      </fieldset>
      <div className="pattern-groups">
        <StrokeGroup title={tr('첫 번째')} unit={unit} keys={start} values={values} set={set} swatches={swatches} off={off} />
        <StrokeGroup title={tr('다음')} unit={unit} keys={end} values={values} set={set} swatches={swatches} off={off} />
      </div>
      <fieldset><legend>{tr('건너뛰기')}</legend>
        <label>{rows ? tr('처음 건너뛸 행:') : tr('처음 건너뛸 열:')}<TextNumberField width={150} label={tr('처음 건너뛰기')} text={values[skipFirst] ?? ''} onText={(t) => set(skipFirst, t)} format={formatNumeric} parse={parseWhole} min={0} max={999} step={1} decimals={0} disabled={off} /></label>
        <label>{rows ? tr('마지막 건너뛸 행:') : tr('마지막 건너뛸 열:')}<TextNumberField width={150} label={tr('마지막 건너뛰기')} text={values[skipLast] ?? ''} onText={(t) => set(skipLast, t)} format={formatNumeric} parse={parseWhole} min={0} max={999} step={1} decimals={0} disabled={off} /></label>
        <label className="check"><input type="checkbox" checked disabled aria-label={tr('로컬 서식 유지')} /> {tr('로컬 서식 유지')}</label>
      </fieldset>
    </>
  );
}

/** Fills: the rows' pattern, or the columns' when the columns have priority. */
export function FillPatterns({ values, set, swatches }: Props) {
  const columns = values.ColumnFillsPriority === 'true';
  const dim: Dim = columns ? 'Column' : 'Row';
  const [start, end] = [fillKeys(dim, 'Start'), fillKeys(dim, 'End')];
  const preset = presetOf(values[start.count], values[end.count]);
  const [skipFirst, skipLast] = skipKeys('Fill', dim);
  // the pattern list: rows, then columns; "none" zeroes every count (InDesign's alternatingFills = none)
  const mode = preset === 'none' ? 'none' : `${columns ? 'columns' : 'rows'}:${preset}`;
  const pick = (m: string) => {
    if (m === 'none') {
      for (const d of ['Row', 'Column'] as Dim[]) for (const s of SIDES) set(fillKeys(d, s).count, '0');
      set('ColumnFillsPriority', 'false');
      return;
    }
    const [kind, p] = m.split(':');
    const d: Dim = kind === 'columns' ? 'Column' : 'Row';
    const [a, b] = [fillKeys(d, 'Start').count, fillKeys(d, 'End').count];
    set('ColumnFillsPriority', String(d === 'Column'));
    if (p === 'custom') { if (Number(values[a]) + Number(values[b]) === 0) { set(a, '1'); set(b, '1'); } return; }
    set(a, p); set(b, p);
  };
  const rowNames = PRESET_NAMES(true), columnNames = PRESET_NAMES(false);
  const off = preset === 'none';
  const unit = columns ? tr('열 수:') : tr('행 수:');
  return (
    <>
      <fieldset><legend>{tr('칠 교대 패턴')}</legend>
        <label>{tr('교대 패턴:')}
          <select aria-label={tr('교대 패턴')} value={mode} onChange={(e) => pick(e.target.value)}>
            <option value="none">{rowNames.none}</option>
            {(['1', '2', '3', 'custom'] as const).map((p) => <option key={`rows:${p}`} value={`rows:${p}`}>{rowNames[p]}</option>)}
            {(['1', '2', '3', 'custom'] as const).map((p) => <option key={`columns:${p}`} value={`columns:${p}`}>{columnNames[p]}</option>)}
          </select>
        </label>
      </fieldset>
      <div className="pattern-groups">
        <FillGroup title={tr('첫 번째')} unit={unit} keys={start} values={values} set={set} swatches={swatches} off={off} />
        <FillGroup title={tr('다음')} unit={unit} keys={end} values={values} set={set} swatches={swatches} off={off} />
      </div>
      <fieldset><legend>{tr('건너뛰기')}</legend>
        <label>{columns ? tr('처음 건너뛸 열:') : tr('처음 건너뛸 행:')}<TextNumberField width={150} label={tr('처음 건너뛰기')} text={values[skipFirst] ?? ''} onText={(t) => set(skipFirst, t)} format={formatNumeric} parse={parseWhole} min={0} max={999} step={1} decimals={0} disabled={off} /></label>
        <label>{columns ? tr('마지막 건너뛸 열:') : tr('마지막 건너뛸 행:')}<TextNumberField width={150} label={tr('마지막 건너뛰기')} text={values[skipLast] ?? ''} onText={(t) => set(skipLast, t)} format={formatNumeric} parse={parseWhole} min={0} max={999} step={1} decimals={0} disabled={off} /></label>
        <label className="check"><input type="checkbox" checked disabled aria-label={tr('로컬 서식 유지')} /> {tr('로컬 서식 유지')}</label>
      </fieldset>
    </>
  );
}
