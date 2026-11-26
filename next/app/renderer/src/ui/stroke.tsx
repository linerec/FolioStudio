// Stroke panel: weight, cap, miter limit, join, alignment, type, arrowheads and their scale,
// gap colour and tint of the selected objects. The engine writes each as InDesign does
// (edit/src/ops/stroke.rs, next/oracle/scenarios/basic-stroke.json); the values come from the
// reader (pageItems `stroke`). A field shows nothing where the selected objects differ.
import { t as tr } from '../i18n';
import type { EditCommand } from '../engine';
import type { DocumentInfo, ItemInfo, PageItems } from '../inspect';
import { unitOf } from '../inspect';
import { PreviewNumberField } from './controls/previewnumberfield';
import { Icon } from './icons';
import { type SelEntry, findItem } from './overlay';

export type StrokeChange = {
  cap?: string; join?: string; miterLimit?: number; alignment?: string; kind?: string;
  start?: string; end?: string; startScale?: number; endScale?: number; gapColor?: string; gapTint?: number;
};

const CAPS = ['ButtEndCap', 'RoundEndCap', 'ProjectingEndCap'];
const JOINS = ['MiterEndJoin', 'RoundEndJoin', 'BevelEndJoin'];
const ALIGNMENTS = ['Center', 'Inside', 'Outside'];
const SOLID = 'StrokeStyle/$ID/Solid';

/** InDesign's built-in stroke styles in the Stroke panel's order: `Self` and label. */
export const STROKE_TYPES: [string, () => string][] = [
  ['Solid', () => tr('실선')],
  ['ThickThick', () => tr('굵게 - 굵게')],
  ['ThickThin', () => tr('굵게 - 가늘게')],
  ['ThickThinThick', () => tr('굵게 - 가늘게 - 굵게')],
  ['ThinThick', () => tr('가늘게 - 굵게')],
  ['ThinThickThin', () => tr('가늘게 - 굵게 - 가늘게')],
  ['ThinThin', () => tr('가늘게 - 가늘게')],
  ['Triple_Stroke', () => tr('3중')],
  ['Canned Dashed 3x2', () => tr('파선(3 및 2)')],
  ['Canned Dashed 4x4', () => tr('파선(4 및 4)')],
  ['Left Slant Hash', () => tr('왼쪽 사선 해시')],
  ['Right Slant Hash', () => tr('오른쪽 사선 해시')],
  ['Straight Hash', () => tr('직선 해시')],
  ['Canned Dotted', () => tr('점선')],
  ['Wavy', () => tr('물결선')],
  ['White Diamond', () => tr('흰색 다이아몬드')],
  ['Japanese Dots', () => tr('일본식 점')],
  ['Dashed', () => tr('파선')],
].map(([id, label]) => [`StrokeStyle/$ID/${id as string}`, label as () => string]);

export const ARROWHEADS: [string, () => string][] = [
  ['None', () => tr('없음')],
  ['SimpleArrowHead', () => tr('단순')],
  ['SimpleWideArrowHead', () => tr('단순 - 넓게')],
  ['TriangleArrowHead', () => tr('삼각형')],
  ['TriangleWideArrowHead', () => tr('삼각형 - 넓게')],
  ['BarbedArrowHead', () => tr('미늘')],
  ['CurvedArrowHead', () => tr('곡선')],
  ['CircleArrowHead', () => tr('원')],
  ['CircleSolidArrowHead', () => tr('원 - 단색')],
  ['SquareArrowHead', () => tr('사각형')],
  ['SquareSolidArrowHead', () => tr('사각형 - 단색')],
  ['BarArrowHead', () => tr('막대')],
];

/** The weights InDesign's Stroke panel lists. */
const WEIGHTS = [0.25, 0.5, 0.75, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100];

/** A document stroke style's name from its `Self` ("DashedStrokeStyle/House dash" → "House dash"). */
export const styleName = (id: string) => id.slice(id.indexOf('/') + 1).replace(/^\$ID\//, '');

type Target = { id: string; item: ItemInfo };
type Stroke = ItemInfo['stroke'];

/** The selected objects (not their content, not locked). */
export function strokeTargets(selection: SelEntry[], peek: (page: number) => PageItems | null | undefined): Target[] {
  return selection.filter((s) => !s.content).map((s) => {
    const item = findItem(peek(s.page), s.id)?.item;
    return item && !item.locked ? { id: s.id, item } : null;
  }).filter((t): t is Target => !!t);
}

/** The targets' common value, or null where they differ. */
function common<T>(targets: Target[], get: (s: Stroke) => T): T | null {
  if (!targets.length) return null;
  const first = get(targets[0].item.stroke);
  return targets.every((t) => get(t.item.stroke) === first) ? first : null;
}

function Choice({ ids, icons, titles, value, onPick, disabled }: { ids: string[]; icons: string[]; titles: string[]; value: string | null; onPick: (v: string) => void; disabled: boolean }) {
  return (
    <span className="seg" role="radiogroup">
      {ids.map((id, i) => (
        <button key={id} role="radio" aria-checked={value === id} className={value === id ? 'on' : ''} title={titles[i]} aria-label={titles[i]} disabled={disabled}
          onClick={() => { if (value !== id) onPick(id); }} data-value={id}>
          <Icon name={icons[i]} size={16} />
        </button>
      ))}
    </span>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="p-row">
      <span className="p-label">{label}</span>
      <span className="p-ctl">{children}</span>
    </div>
  );
}

export function StrokePanel({ targets, info, editable, edit, options = true }: { targets: Target[]; info: DocumentInfo | null; editable: boolean; options?: boolean; edit: (label: string, commands: EditCommand[]) => unknown }) {
  const can = editable && targets.length > 0;
  const ids = targets.map((t) => t.id);
  const change = (label: string, c: StrokeChange) => { if (can) edit(label, [{ op: 'setStroke', ids, change: c }]); };
  const weight = common(targets, (s) => s.weight);
  // Preferences > Units & Increments > Stroke: the weight is shown and typed in points or millimetres
  const su = unitOf(info?.view?.strokeUnits), k = su.perPoint;
  const kind = common(targets, (s) => s.type);
  const solid = kind === SOLID;
  const gapSwatch = common(targets, (s) => (s.gap.kind === 'none' ? '[None]' : s.gap.swatch ?? ''));
  const swatches = (info?.swatches ?? []).filter((s) => s.model !== 'none');
  const gapId = (name: string | null) => (name === '[None]' ? 'Swatch/None' : swatches.find((s) => s.name === name)?.id ?? '');
  const custom = (info?.strokeStyles ?? []).filter(s=>s.kind!=='StrokeStyle');
  const known = kind === null || STROKE_TYPES.some(([id]) => id === kind) || custom.some(s=>s.id===kind);
  return (
    <div className="p-form stroke-panel">
      <Row label={tr('두께:')}>
        <PreviewNumberField value={weight === null ? null : weight * k} suffix={su.suffix ? ` ${su.suffix}` : ''} min={0} max={1000 * k} step={k} width={86} label={tr('획 두께')} undoLabel={tr('획 두께')} disabled={!can}
          options={WEIGHTS.map((w) => w * k)} scrubRowLabel anchor={k} edit={edit}
          build={(v) => [{ op: 'setPaint', ids, strokeWeight: Math.round((v / k) * 1e6) / 1e6 }]} />
      </Row>
      <Row label={tr('단면:')}>
        <Choice ids={CAPS} icons={['capJoinButt', 'capRound', 'capSquare']} titles={[tr('평평한 단면'), tr('둥근 단면'), tr('돌출 단면')]}
          value={common(targets, (s) => s.cap)} disabled={!can} onPick={(v) => change(tr('획 단면'), { cap: v })} />
      </Row>
      <Row label={tr('이음 한도:')}>
        <PreviewNumberField value={common(targets, (s) => s.miterLimit)} min={1} max={500} width={50} label={tr('이음 한도')} undoLabel={tr('이음 한도')} disabled={!can}
          scrubRowLabel anchor={4} edit={edit} build={(v) => [{ op: 'setStroke', ids, change: { miterLimit: v } }]} /> <span className="p-unit">x</span>
      </Row>
      <Row label={tr('연결:')}>
        <Choice ids={JOINS} icons={['joinMiter', 'joinRound', 'joinBevel']} titles={[tr('마이터 연결'), tr('둥근 연결'), tr('경사 연결')]}
          value={common(targets, (s) => s.join)} disabled={!can} onPick={(v) => change(tr('획 연결'), { join: v })} />
      </Row>
      <Row label={tr('획 정렬:')}>
        <Choice ids={ALIGNMENTS} icons={['strokeCenter', 'strokeInside', 'strokeOutside']} titles={[tr('가운데 정렬'), tr('안쪽 정렬'), tr('바깥쪽 정렬')]}
          value={common(targets, (s) => s.alignment)} disabled={!can} onPick={(v) => change(tr('획 정렬'), { alignment: `${v}Alignment` })} />
      </Row>
      {options && <>
      <div className="p-sep" />
      <Row label={tr('유형:')}>
        <select aria-label={tr('획 유형')} style={{ width: 140 }} value={kind ?? ''} disabled={!can} onChange={(e) => change(tr('획 유형'), { kind: e.target.value })}>
          {kind === null && <option value="" disabled />}
          {!known && kind && <option value={kind} disabled>{styleName(kind)}</option>}
          {STROKE_TYPES.map(([id, label]) => <option key={id} value={id}>{label()}</option>)}
          {custom.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </Row>
      <Row label={tr('시작/끝:')}>
        {(['start', 'end'] as const).map((end) => (
          <select key={end} aria-label={end === 'start' ? tr('시작 화살촉') : tr('끝 화살촉')} style={{ width: 62 }} value={common(targets, (s) => s[end]) ?? ''} disabled={!can}
            onChange={(e) => change(tr('화살촉'), { [end]: e.target.value })}>
            {common(targets, (s) => s[end]) === null && <option value="" disabled />}
            {ARROWHEADS.map(([id, label]) => <option key={id} value={id}>{label()}</option>)}
          </select>
        ))}
        <span className="seg"><button disabled title={tr('시작과 끝 화살촉 바꾸기') + ' — ' + tr('지원하지 않는 옵션입니다.')}><Icon name="swapArrow" size={14} /></button></span>
      </Row>
      <Row label={tr('크기 조정:')}>
        <PreviewNumberField value={common(targets, (s) => s.startScale)} suffix=" %" min={1} max={1000} width={58} label={tr('시작 화살촉 크기')} undoLabel={tr('화살촉 크기')} disabled={!can}
          scrubRowLabel anchor={100} edit={edit} build={(v) => [{ op: 'setStroke', ids, change: { startScale: v } }]} />
        <PreviewNumberField value={common(targets, (s) => s.endScale)} suffix=" %" min={1} max={1000} width={58} label={tr('끝 화살촉 크기')} undoLabel={tr('화살촉 크기')} disabled={!can}
          anchor={100} edit={edit} build={(v) => [{ op: 'setStroke', ids, change: { endScale: v } }]} />
        <span className="seg"><button disabled title={tr('시작과 끝 화살촉 크기 연결') + ' — ' + tr('지원하지 않는 옵션입니다.')}><Icon name="chain" size={14} /></button></span>
      </Row>
      <Row label={tr('정렬:')}>
        <span className="seg">
          <button disabled title={tr('화살촉 끝을 패스 끝 너머로 연장') + ' — ' + tr('지원하지 않는 옵션입니다.')}><Icon name="alignLeft" size={16} /></button>
          <button disabled title={tr('화살촉 끝을 패스 끝에 배치') + ' — ' + tr('지원하지 않는 옵션입니다.')}><Icon name="alignRight" size={16} /></button>
        </span>
      </Row>
      <Row label={tr('간격 색상:')}>
        <select aria-label={tr('간격 색상')} style={{ width: 140 }} value={gapSwatch === null ? '' : gapId(gapSwatch)} disabled={!can || solid}
          onChange={(e) => change(tr('간격 색상'), { gapColor: e.target.value })}>
          {(gapSwatch === null || !gapId(gapSwatch)) && <option value="" disabled>{gapSwatch ?? ''}</option>}
          <option value="Swatch/None">{tr('[없음]')}</option>
          {swatches.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
        </select>
      </Row>
      <Row label={tr('간격 색조:')}>
        <PreviewNumberField value={gapSwatch === '[None]' ? null : common(targets, (s) => s.gap.tint ?? 100)} suffix=" %" min={0} max={100} width={66} label={tr('간격 색조')} undoLabel={tr('간격 색조')}
          disabled={!can || solid || gapSwatch === '[None]'} scrubRowLabel anchor={100} edit={edit} build={(v) => [{ op: 'setStroke', ids, change: { gapTint: v } }]} />
      </Row>
      </>}
    </div>
  );
}
