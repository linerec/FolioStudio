import { t as tr } from '../i18n';
// Control panel (under the menu bar). Like InDesign's it changes with the tool and the
// selection: object controls (reference point, X/Y/W/H, scale, rotate, shear, fill, stroke,
// effects, wrap, corners, fitting, align, object style), character/paragraph controls for the
// Type tool, page size controls for the Page tool. Values are read from the selection.
// X, Y, W and H change it (folio-edit, checked against InDesign). Unavailable controls are disabled.
// Like InDesign by default ("Dimensions Include Stroke Weight"), X/Y/W/H measure the box with
// the stroke.
import { useContext, useEffect, useRef, useState } from 'react';
import { type DocumentInfo, type ItemInfo, type PageItems, apply, decompose, formatLength, pageOf, parseLength, relativeTransform, strokeOutset, trim, unionBox, unitOf } from '../inspect';
import { Icon } from './icons';
import { useHelpCard } from './toolhelpview';
import type { SelEntry } from './overlay';
import { findItem } from './overlay';
import type { ToolId } from './tools';
import { ALIGN_BUTTONS, AlignReference, type AlignmentControls } from './alignment';
import { TextFormatBar, type TextFormatting } from './textformat';
import { SHAPES, common, cornerTargets, shapeLabel } from './corners';
import { STROKE_TYPES, type StrokeChange, strokeTargets, styleName } from './stroke';
import { blendTargets, commonBlend, parsePercent } from './effects';
import type { Corner, CornerShape } from '../inspect';
import { NumberField } from './controls/numberfield';
import { PreviewEditContext, PreviewScopeContext } from './controls/usePreviewEdit';
import { rulerOffset, zeroOf } from './zeropoint';
import { ControlBarMenu, HiddenGroups, useControlBar } from './controlbar';

export type RefPoint = [number, number];

/** What the control bar (and the Transform panel) reads and changes. */
export type ControlProps = Props;

type Props = {
  tool: ToolId;
  selection: SelEntry[];
  store: { peek: (p: number) => PageItems | null | undefined };
  info: DocumentInfo | null;
  units: { horizontal: string; vertical: string };
  rulerOrigin: string;
  refPoint: RefPoint;
  setRefPoint: (r: RefPoint) => void;
  selectedPage: { index: number; width: number; height: number } | null;
  readOnly: boolean;
  textMode: 'character' | 'paragraph';
  setTextMode: (m: 'character' | 'paragraph') => void;
  onCommand: (id: string) => void;
  /** The document can be changed; X/Y/W/H then edit the selection. */
  editable: boolean;
  /** Moves the selection by a distance (points). */
  onMove: (dx: number, dy: number) => void;
  /** Sets the selected object's width and height (points; stroke included unless `includeStroke` is off) about the reference point. */
  onResize: (w: number, h: number) => void;
  /** Sets the selected page's size about the reference point. `start` is the page as it was when a value began to be tried
   *  ([width, height, origin x, origin y]): a trial is shown while the page changes, so the anchor is measured from the start. */
  onPageResize: (w: number, h: number, start?: number[]) => void;
  /** Gives the selected page a size of the Page Size list (engine command setPageSize), about the reference point. */
  onPagePreset?: (preset: string) => void;
  /** Moves the selected page by a distance in its spread (setPageGeometry). `start` is the page as it was when a value began to be tried
   *  ([origin x, origin y, width, height]). */
  onPageMove?: (dx: number, dy: number, start?: number[]) => void;
  /** Page tool ▸ Objects Move with Page (InDesign: an application preference, off by default; it acts on dragging a page and on X / Y, not on W / H). */
  objectsMoveWithPage?: boolean;
  onObjectsMoveWithPage?: (on: boolean) => void;
  /** Transform panel menu ▸ Dimensions Include Stroke Weight (default on): W and H count the stroke. */
  includeStroke?: boolean;
  /** Transform panel menu ▸ Transformations are Totals (default off): the rotation, scale and shear of an object inside a group are its page totals
   *  (on) or its own, relative to the group (off); typed values mean the same (rules/transform-totals.ko.md). */
  totals?: boolean;
  /** Rotates the selection by degrees (counter-clockwise) about the reference point. */
  onRotate: (by: number) => void;
  onFlip: (axis: 'horizontal' | 'vertical') => void;
  onFit: (how: 'proportionally' | 'fill' | 'contentToFrame' | 'frameToContent' | 'center') => void;
  onStrokeWeight: (w: number) => void;
  /** Scales the one selected object by these factors, applied to its content (scale.rs). */
  onScale: (sx: number, sy: number) => void;
  /** Sets the rotation or shear angle of the one selected object, about its own reference point. */
  onAngles: (angles: { rotation?: number; shear?: number; content?: boolean }) => void;
  /** The fill or stroke swatch the control bar's list picked (a swatch id, or Swatch/None) for the selected objects. */
  onPaint?: (kind: 'fill' | 'stroke', swatch: string) => void;
  /** The text wrap button pressed for the one selected object (the Text Wrap panel's modes). */
  onTextWrap?: (mode: string) => void;
  /** The object style the control bar's list picked for the selected objects. */
  onObjectStyle?: (style: string) => void;
  /** A Stroke panel change to the selected objects (stroke.tsx). */
  onStroke: (change: StrokeChange, label: string) => void;
  /** The fx button: Object ▸ Effects. */
  onEffects?: () => void;
  /** The drop shadow button: the shadow on or off. */
  onDropShadow?: () => void;
  /** Changes all four corners of the selected objects (corners.tsx). */
  onCorners: (change: (c: Corner[]) => Corner[], label: string) => void;
  /** Opacity of the selected objects (their graphics when the content is selected). */
  onOpacity: (v: number) => void;
  alignment: AlignmentControls;
  textFormatting:TextFormatting;
};

/** The number a numeric Field edits: its value, how it is written and read (units, sums), and how a value becomes an edit.
 *  `apply(v, base)` makes the edit for the value v; `base` is the value the box had when the trial began (the edits of the
 *  move and scale fields are relative to it). Such a field steps (arrows, ▲▼, wheel), scrubs by its label and previews. */
type Num = {
  value: number | null; format: (v: number) => string; parse: (t: string) => number | null; min: number; max: number; step: number;
  /** `start` is what `snapshot` read when the trial began (the document as it was): an edit that depends on a second value of the
   *  document (the proportion of a chain) takes it from there, since the document changes while a trial is shown. */
  apply: (v: number, base: number, start?: number[]) => void;
  snapshot?: () => number[];
};

/** A value box. Available numeric edits use the shared number field; unavailable values are disabled. */
function Field({ label, value, width = 88, icon, title = '', onCommit, num }: {
  label?: string; value: string; width?: number; icon?: string; title?: string;
  onCommit?: (text: string) => void; num?: Num;
}) {
  const [grip, setGrip] = useState<HTMLElement | null>(null);
  const session = useContext(PreviewEditContext), scope = useContext(PreviewScopeContext);
  const tried = useRef<number | null>(null);
  const start = useRef<number[] | undefined>(undefined);
  if (onCommit && num) {
    // The shared field. A trial goes to the engine's preview session through the window's own handler (scope diverts its edit),
    // a commit makes what the session shows permanent; a typed value is an ordinary edit.
    const preview = scope && session ? (v: number, base: number | null) => {
      if (tried.current === null) start.current = num.snapshot?.();
      tried.current = v;
      scope(() => num.apply(v, base ?? num.value ?? 0, start.current));
    } : undefined;
    return (
      <label className="cp-field" title={title}>
        <span ref={setGrip} className="cp-grip">{icon ? <Icon name={icon} size={16} className="cp-ficon" /> : <span className="cp-label">{label?.replace(/:$/, '')}</span>}</span>
        <span className="cp-input" style={{ width }}>
          <NumberField value={num.value} label={title || label || ''} min={num.min} max={num.max} step={num.step} width="100%" format={num.format} parse={num.parse} scrubElement={grip}
            onPreview={preview}
            onCancel={() => { tried.current = null; start.current = undefined; void session?.cancel(); }}
            onCommit={(v) => {
              const same = tried.current !== null && Math.abs(tried.current - v) < 1e-9;
              tried.current = null;
              start.current = undefined;
              if (session?.open && same) { void session.commit(); return; }
              session?.discard();
              num.apply(v, num.value ?? 0, num.snapshot?.());
            }} />
        </span>
      </label>
    );
  }
  // Unavailable now (no selection, read-only document): the box is shown grey and cannot be edited.
  return (
    <label className="cp-field" title={title}>
      {icon ? <Icon name={icon} size={16} className="cp-ficon" /> : <span className="cp-label">{label?.replace(/:$/, '')}</span>}
      <span className="cp-input" style={{ width }}>
        <input value={value} disabled readOnly tabIndex={-1} />
        <i className="cp-spin" />
      </span>
    </label>
  );
}

function Combo({ value, width = 120, chip, title = '' }: { value: string; width?: number; chip?: string | null; title?: string }) {
  return (
    <span className="cp-combo" aria-disabled="true" style={{ width }} title={title}>
      {chip !== undefined && <i className={chip ? 'cp-chip' : 'cp-chip none'} style={chip ? { background: chip } : undefined} />}
      <span className="cp-combo-text">{value}</span>
      <span className="cp-combo-arrow"><Icon name="chevronDown" size={12} /></span>
    </span>
  );
}

/** Fill or stroke of the selection: the swatch it has and the document's swatches to pick from. */
function SwatchSelect({ paint, swatches, disabled, label, onPick }: {
  paint: ItemInfo['fill'] | undefined; swatches: DocumentInfo['swatches']; disabled: boolean; label: string; onPick: (id: string) => void;
}) {
  const current = paint?.kind === 'none' ? 'Swatch/None' : swatches.find((s) => s.id === paint?.swatch || s.name === paint?.swatch)?.id ?? '';
  const chip = paint && paint.kind !== 'none' ? rgbCss(paint.rgb) : null;
  return (
    <span className="cp-swatchpick">
      <i className={chip ? 'cp-chip' : 'cp-chip none'} style={chip ? { background: chip } : undefined} />
      <select className="cp-combo cp-select" style={{ width: 92 }} title={label} aria-label={label} disabled={disabled} value={current} onChange={(e) => onPick(e.target.value)}>
        {!current && <option value="" disabled>{paintName(paint)}</option>}
        <option value="Swatch/None">{tr('[없음]')}</option>
        {swatches.filter((s) => s.id !== 'Swatch/None').map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
      </select>
    </span>
  );
}

/** A button is disabled when its action is unavailable. */
function Btn({ icon, title, on, onClick, disabled }: { icon: string; title: string; on?: boolean; onClick?: () => void; disabled?: boolean }) {
  return (
    <button className={`cp-btn${on ? ' on' : ''}`} title={title} onClick={onClick} disabled={disabled || !onClick}>
      <Icon name={icon} size={16} />
    </button>
  );
}

function RefProxy({ value, onChange }: { value: RefPoint; onChange: (r: RefPoint) => void }) {
  // the instant tool card (the same as the tools'): below the control bar
  const help = useHelpCard('below');
  return (
    <div className="ref-proxy" title={tr("기준점")} {...help.trigger('refPoint', '.ref-proxy')}>
      {[0, 0.5, 1].map((fy) => [0, 0.5, 1].map((fx) => (
        <button key={`${fx}${fy}`} className={value[0] === fx && value[1] === fy ? 'on' : ''} onClick={() => onChange([fx, fy])} aria-label={tr('기준점 {0},{1}', {0:fx,1:fy})} />
      )))}
      {help.card}
    </div>
  );
}

const rgbCss = (c: [number, number, number] | null | undefined) => (c ? `rgb(${c.join(',')})` : null);

type Readout = { x: number; y: number; w: number; h: number; sx: number | null; sy: number | null; rot: number | null; shear: number | null; item: ItemInfo | null; content: boolean };

function readout(p: Props): Readout | null {
  const resolved = p.selection
    .map((s) => ({ s, f: findItem(p.store.peek(s.page), s.id) }))
    .filter((r): r is { s: SelEntry; f: NonNullable<ReturnType<typeof findItem>> } => !!r.f);
  if (!resolved.length) return null;
  const zero = zeroOf(p.info);
  const offset = (page: number): [number, number] => rulerOffset(p.rulerOrigin, pageOf(p.info, page)?.spreadOffset, zero);
  const [fx, fy] = p.refPoint;
  if (resolved.length === 1) {
    const { s, f } = resolved[0];
    const it = f.item;
    const content = s.content && !!it.content;
    const m = content ? it.content!.transform : it.transform;
    // the box with the stroke (placed content has none)
    const o = content || p.includeStroke === false ? 0 : strokeOutset(it);
    const g = content ? it.content!.inner : it.inner;
    const b: [number, number, number, number] = [g[0] - o, g[1] - o, g[2] + o, g[3] + o];
    const [px, py] = apply(m, b[0] + (b[2] - b[0]) * fx, b[1] + (b[3] - b[1]) * fy);
    const [ox, oy] = offset(s.page);
    const d = decompose(m);
    // Transformations are Totals off: an item inside a group shows its own transform, relative to the group (W and H, X and Y are the same either way)
    const group = f.path.length > 1 ? f.path[f.path.length - 2] : null;
    const rel = !content && p.totals !== true && group ? relativeTransform(group.transform, m) : null;
    const v = rel ? decompose(rel) : d;
    return { x: px + ox, y: py + oy, w: (b[2] - b[0]) * d.sx, h: (b[3] - b[1]) * d.sy, sx: v.sx * 100, sy: v.sy * 100, rot: v.rotation, shear: v.shear, item: it, content };
  }
  const boxes = resolved.map(({ s, f }) => {
    const [ox, oy] = offset(s.page);
    const b = f.item.bounds;
    const o = p.includeStroke === false ? 0 : strokeOutset(f.item);
    return [b[0] + ox - o, b[1] + oy - o, b[2] + ox + o, b[3] + oy + o] as [number, number, number, number];
  });
  const u = unionBox(boxes)!;
  return { x: u[0] + (u[2] - u[0]) * fx, y: u[1] + (u[3] - u[1]) * fy, w: u[2] - u[0], h: u[3] - u[1], sx: 100, sy: 100, rot: 0, shear: 0, item: null, content: false };
}

export function ControlPanel(p: Props) {
  if (p.tool === 'type' || p.tool === 'typePath' || p.tool === 'typeVertical') return <TextControls {...p} />;
  if (p.tool === 'page') return <PageControls {...p} />;
  return <ObjectControls {...p} />;
}

/** X, Y, W, H, scale, rotation and shear of the selection, read and set as the control bar does
 * (the Transform panel shows the same fields). */
function useTransform(p: Props) {
  const r = readout(p);
  const includeStroke = p.includeStroke !== false;
  const measuredKey = p.editable && p.selection.every(s => !s.content)
    ? JSON.stringify([includeStroke, ...p.selection.map(s => findItem(p.store.peek(s.page), s.id)?.item ?? s.id)]) : '';
  // The size is measured by the engine (with the stroke, text frames...), once for each state of the selected objects: a state seen
  // before (the objects given back by Esc or Undo, the state a trial showed and a commit makes permanent) is known at once. While a new
  // one is being measured the size measured before is shown, so the W and H boxes stay the same boxes and the one being dragged is not
  // replaced when an edit (a value tried in the W box) changes the objects.
  const selKey = measuredKey ? JSON.stringify([includeStroke, p.selection.map((s) => [s.page, s.id])]) : '';
  const sizes = useRef(new Map<string, [number, number]>());
  const [measured, setMeasured] = useState<{ sel: string; size: [number, number] } | null>(null);
  useEffect(() => {
    if (!measuredKey || !p.selection.length) return;
    const known = sizes.current.get(measuredKey);
    if (known) { setMeasured({ sel: selKey, size: known }); return; }
    let live = true;
    window.folio.request({cmd:'selectionSize', ids:p.selection.map(s=>s.id), includeStroke}).then(({header}) => {
      if (!live || !header.ok || !Array.isArray(header.size)) return;
      const size = header.size as [number, number];
      if (sizes.current.size >= 24) sizes.current.delete(sizes.current.keys().next().value as string);
      sizes.current.set(measuredKey, size);
      setMeasured({ sel: selKey, size });
    }).catch(()=>{});
    return () => { live = false; };
  }, [measuredKey]);
  const size = sizes.current.get(measuredKey) ?? (selKey && measured?.sel === selKey ? measured.size : null);
  const measuredNow = !!size;
  if (r && size) [r.w, r.h] = size;
  const L = (v: number | null | undefined, axis: 'h' | 'v' = 'h') => (v === null || v === undefined ? '' : formatLength(v, axis === 'h' ? p.units.horizontal : p.units.vertical));
  const pct = (v: number | null | undefined) => (v === null || v === undefined ? '' : `${trim(v, 2)}%`);
  const deg = (v: number | null | undefined) => (v === null || v === undefined ? '' : `${trim(v, 3)}°`);
  const it = r?.item ?? null;
  const hasSel = p.selection.length > 0;
  // Groups and selections resize their paths while keeping stroke weights/corner radii.
  const can = p.editable && !!r && !r.content;
  const parse = (text: string, axis: 'h' | 'v') => parseLength(text, axis === 'h' ? p.units.horizontal : p.units.vertical);
  const setX = can ? (t: string) => { const v = parse(t, 'h'); if (v !== null && r) p.onMove(v - r.x, 0); } : undefined;
  const setY = can ? (t: string) => { const v = parse(t, 'v'); if (v !== null && r) p.onMove(0, v - r.y); } : undefined;
  const sizable = can && it?.kind !== 'line' && measuredNow
    && new Set(p.selection.map(s=>pageOf(p.info, s.page)?.spreadIndex)).size === 1;
  const setW = sizable ? (t: string) => { const v = parse(t, 'h'); if (v !== null && v > 0 && r) p.onResize(v, r.h); } : undefined;
  const setH = sizable ? (t: string) => { const v = parse(t, 'v'); if (v !== null && v > 0 && r) p.onResize(r.w, v); } : undefined;
  // rotation: the field says the angle, the buttons turn by 90°; flips mirror on the page
  // one object: the angle as a value about its own reference point (oracle: basic-transform-values);
  // several: a turn by the value about the selection's
  const angle = (t: string) => { const v = parseFloat(t.replace(',', '.').replace('°', '')); return Number.isFinite(v) ? v : null; };
  const one = p.selection.length === 1;
  const setRot = can ? (t: string) => { const v = angle(t); if (v !== null && r) { if (one) p.onAngles({ rotation: v }); else p.onRotate(v); } } : undefined;
  // scale: one object, its content scaled with it (the field reads 100 % again); a text frame's
  // text would scale too, which is not built
  const kinds = p.selection.map((s) => findItem(p.store.peek(s.page), s.id)?.item.kind);
  const manyText = !one && kinds.includes('textFrame');
  // a text frame scales with its text (App.tsx: `scaleMany`); the engine refuses a story with a table, a footnote or an anchored object, and a gradient frame, and changes nothing
  const scalable = can;
  const percent = (t: string) => { const v = parseFloat(t.replace(',', '.').replace('%', '')); return Number.isFinite(v) && v > 0 ? v : null; };
  const setSx = scalable ? (t: string) => { const v = percent(t); if (v !== null && r?.sx) p.onScale(v / r.sx, 1); } : undefined;
  const setSy = scalable ? (t: string) => { const v = percent(t); if (v !== null && r?.sy) p.onScale(1, v / r.sy); } : undefined;
  const scaleNote = it?.kind === 'textFrame' || manyText ? '\n' + tr('표·각주·인라인 개체가 든 글과 그러데이션 채움이 있는 틀은 비율을 바꿀 수 없습니다.') : '';
  // shear: one object as a value, several objects by the value about the selection's reference point (the field reads 0° again), the
  // placed graphic when the content is selected (oracle: ctl2-shear-content-*, ctl2-shear-many-*)
  const shearable = p.editable && !!r && (r.content ? one : can);
  const setShear = shearable ? (t: string) => { const v = angle(t); if (v !== null && Math.abs(v) < 90) p.onAngles({ shear: v, ...(r?.content ? { content: true } : {}) }); } : undefined;
  const turn = can ? (by: number) => () => p.onRotate(by) : undefined;
  const flip = can ? (axis: 'horizontal' | 'vertical') => () => p.onFlip(axis) : undefined;
  // the numbers behind the fields: the shared field steps, scrubs and previews them (see Num)
  const unitName = (axis: 'h' | 'v') => (axis === 'h' ? p.units.horizontal : p.units.vertical);
  const stepOf = (name: string | undefined) => { const u = unitOf(name); return u.kind ? 1 : 1 / u.perPoint; };
  const len = (axis: 'h' | 'v', value: number | null | undefined, set: ((t: string) => void) | undefined, apply: (v: number, base: number) => void, min = -1e5): Num | undefined =>
    set ? { value: value ?? null, format: (v) => formatLength(v, unitName(axis)), parse: (t) => parseLength(t, unitName(axis)), min, max: 1e5, step: stepOf(unitName(axis)), apply } : undefined;
  const nx = len('h', r?.x, setX, (v, base) => p.onMove(v - base, 0));
  const ny = len('v', r?.y, setY, (v, base) => p.onMove(0, v - base));
  const nw = len('h', r?.w, setW, (v) => r && p.onResize(v, r.h), 0.001);
  const nh = len('v', r?.h, setH, (v) => r && p.onResize(r.w, v), 0.001);
  const nrot: Num | undefined = setRot ? { value: r?.rot ?? null, format: (v) => `${trim(v, 3)}°`, parse: angle, min: -1e5, max: 1e5, step: 1, apply: (v) => { if (one) p.onAngles({ rotation: v }); else p.onRotate(v); } } : undefined;
  const nsx: Num | undefined = setSx ? { value: r?.sx ?? null, format: (v) => `${trim(v, 2)}%`, parse: percent, min: 0.01, max: 1e5, step: 1, apply: (v, base) => { if (base) p.onScale(v / base, 1); } } : undefined;
  const nsy: Num | undefined = setSy ? { value: r?.sy ?? null, format: (v) => `${trim(v, 2)}%`, parse: percent, min: 0.01, max: 1e5, step: 1, apply: (v, base) => { if (base) p.onScale(1, v / base); } } : undefined;
  const nshear: Num | undefined = setShear ? { value: r?.shear ?? null, format: (v) => `${trim(v, 3)}°`, parse: angle, min: -89.999, max: 89.999, step: 1, apply: (v) => p.onAngles({ shear: v, ...(r?.content ? { content: true } : {}) }) } : undefined;
  return { r, L, pct, deg, it, hasSel, can, scaleNote, parse, one, setX, setY, setW, setH, setRot, setSx, setSy, setShear, turn, flip, nx, ny, nw, nh, nrot, nsx, nsy, nshear };
}

/** Window ▸ Object & Layout ▸ Transform: the control bar's position, size, scale and angle fields. */
export function TransformFields(p: Props) {
  const t = useTransform(p);
  const { r } = t;
  return (
    <div className="p-form transform-fields">
      <div className="tp-ref"><RefProxy value={p.refPoint} onChange={p.setRefPoint} /></div>
      <div className="tp-grid">
        <Field label="X:" value={t.L(r?.x)} title={tr('X 위치 (기준점)')} onCommit={t.setX} num={t.nx} />
        <Field label="W:" value={t.L(r?.w)} title={tr('폭 (획 포함)')} onCommit={t.setW} num={t.nw} />
        <Field label="Y:" value={t.L(r?.y, 'v')} title={tr('Y 위치 (기준점)')} onCommit={t.setY} num={t.ny} />
        <Field label="H:" value={t.L(r?.h, 'v')} title={tr('높이 (획 포함)')} onCommit={t.setH} num={t.nh} />
        <Field icon="scale" value={t.pct(r?.sx)} width={64} title={tr('X 비율') + t.scaleNote} onCommit={t.setSx} num={t.nsx} />
        <Field icon="scale" value={t.pct(r?.sy)} width={64} title={tr('Y 비율') + t.scaleNote} onCommit={t.setSy} num={t.nsy} />
        <Field icon="rotate" value={t.deg(r?.rot)} width={64} title={tr('회전 각도 (시계 반대 방향 +)')} onCommit={t.setRot} num={t.nrot} />
        <Field icon="shear" value={t.deg(r?.shear)} width={64} title={tr('기울이기 각도')} onCommit={t.setShear} num={t.nshear} />
      </div>
    </div>
  );
}

/** The Text Wrap panel's modes (its buttons): the DOM name, the icon, the label, the model's name. */
const WRAP_BUTTONS: [string, string, () => string, string][] = [
  ['NONE', 'wrapNone', () => tr('텍스트 감싸기 없음'), ''],
  ['BOUNDING_BOX_TEXT_WRAP', 'wrapBox', () => tr('경계 상자 감싸기'), 'BoundingBox'],
  ['CONTOUR', 'wrapShape', () => tr('개체 모양 감싸기'), 'Contour'],
  ['JUMP_OBJECT_TEXT_WRAP', 'wrapJump', () => tr('개체 건너뛰기'), 'JumpObject'],
  ['NEXT_COLUMN_TEXT_WRAP', 'textWrap', () => tr('다음 단으로 건너뛰기'), 'NextColumn'],
];

function ObjectControls(p: Props) {
  // Customize (build-c2): the groups the person hid (engine user settings)
  const bar = useControlBar();
  const { r, L, pct, deg, it, hasSel, can, scaleNote, parse, setX, setY, setW, setH, setRot, setSx, setSy, setShear, turn, flip, nx, ny, nw, nh, nrot, nsx, nsy, nshear } = useTransform(p);
  const fill = it?.fill, stroke = it?.stroke;
  // the two chains: W and H (and the scale percentages) change together, in proportion
  const [sizeChain, setSizeChain] = useState(false), [scaleChain, setScaleChain] = useState(false);
  // With a chain on, the other value follows in proportion. A trial of the field is shown while the document changes under it, so
  // the proportion is taken from the size when the trial began (`snapshot`), and the scale chain scales both by the factor from the
  // value the box had then (`base`).
  const chainW: Num | undefined = nw && sizeChain && r && r.w > 0
    ? { ...nw, snapshot: () => [r.w, r.h], apply: (v, _base, s) => { const [w0, h0] = s ?? [r.w, r.h]; if (w0 > 0) p.onResize(v, h0 * v / w0); } } : nw;
  const chainH: Num | undefined = nh && sizeChain && r && r.h > 0
    ? { ...nh, snapshot: () => [r.w, r.h], apply: (v, _base, s) => { const [w0, h0] = s ?? [r.w, r.h]; if (h0 > 0) p.onResize(w0 * v / h0, v); } } : nh;
  const chainSx: Num | undefined = nsx && scaleChain && r?.sx ? { ...nsx, apply: (v, base) => { if (base) p.onScale(v / base, v / base); } } : nsx;
  const chainSy: Num | undefined = nsy && scaleChain && r?.sy ? { ...nsy, apply: (v, base) => { if (base) p.onScale(v / base, v / base); } } : nsy;
  // fitting: a frame holding a placed graphic
  const framed = p.editable && p.selection.length === 1 && !!it?.content;
  const fitWith = (how: Parameters<Props['onFit']>[0]) => (framed ? () => p.onFit(how) : undefined);
  // stroke type: the selected objects' own strokes
  const strokes = strokeTargets(p.selection, (pg) => p.store.peek(pg));
  const strokeEditable = p.editable && strokes.length > 0 && strokes.length === p.selection.length;
  const kind = strokes.length && strokes.every((t) => t.item.stroke.type === strokes[0].item.stroke.type) ? strokes[0].item.stroke.type : null;
  // corners: all four of every selected object with corners of its own
  const corners = cornerTargets(p.selection, (pg) => p.store.peek(pg));
  const corner = p.editable && corners.length > 0 && corners.length === p.selection.length;
  const cc = common(corner ? corners : []);
  const setCornerSize = corner ? (t: string) => { const v = parse(t, 'h'); if (v !== null && v >= 0) p.onCorners((cs) => cs.map((c) => ({ ...c, radius: v })), tr('모퉁이 크기')); } : undefined;
  // the corner size: a length that sets the size of all four corners (absolute, so a trial needs nothing from the start)
  const cornerNum: Num | undefined = setCornerSize ? {
    value: cc.radius, format: (v) => formatLength(v, p.units.horizontal), parse: (t) => { const v = parse(t, 'h'); return v !== null && v >= 0 ? v : null; },
    min: 0, max: 1e5, step: unitOf(p.units.horizontal).kind ? 1 : 1 / unitOf(p.units.horizontal).perPoint,
    apply: (v) => p.onCorners((cs) => cs.map((c) => ({ ...c, radius: v })), tr('모퉁이 크기')),
  } : undefined;
  // opacity: the objects', or their graphics' when the content is selected
  const blendT = blendTargets(p.selection, (pg) => p.store.peek(pg));
  const opacityShown = commonBlend(blendT, blendT.length && blendT.every((x) => x.graphic) ? 'graphic' : 'object').opacity;
  const setOpacity = p.editable && blendT.length === p.selection.length && blendT.length ? (t: string) => { const v = parsePercent(t); if (v !== null) p.onOpacity(v); } : undefined;
  const strokeUnit = p.info?.view?.strokeUnits ?? 'Points';
  const setStroke = can ? (t: string) => { const v = parseLength(t, strokeUnit); if (v !== null && v >= 0) p.onStrokeWeight(v); } : undefined;
  return (
    <div className="control">
      <HiddenGroups hidden={bar.hidden} />
      <div className="cp-group" data-cp="reference"><RefProxy value={p.refPoint} onChange={p.setRefPoint} /></div>
      <div className="cp-group" data-cp="position">
        <div className="cp-row"><Field label="X:" value={L(r?.x)} title={tr("X 위치 (기준점)")} onCommit={setX} num={nx} /></div>
        <div className="cp-row"><Field label="Y:" value={L(r?.y, 'v')} title={tr("Y 위치 (기준점)")} onCommit={setY} num={ny} /></div>
      </div>
      <div className="cp-group" data-cp="size">
        <div className="cp-row"><Field label="W:" value={L(r?.w)} title={tr("폭 (획 포함)")} onCommit={setW} num={chainW} /></div>
        <div className="cp-row"><Field label="H:" value={L(r?.h, 'v')} title={tr("높이 (획 포함)")} onCommit={setH} num={chainH} /></div>
      </div>
      <div className="cp-group narrow" data-cp="sizeChain"><Btn icon="chain" title={tr('폭과 높이 비율 유지')} on={sizeChain} onClick={() => setSizeChain((v) => !v)} /></div>
      <div className="cp-group" data-cp="scale">
        <div className="cp-row"><Field icon="scale" value={pct(r?.sx)} width={64} title={tr('X 비율') + scaleNote} onCommit={setSx} num={chainSx} /></div>
        <div className="cp-row"><Field icon="scale" value={pct(r?.sy)} width={64} title={tr('Y 비율') + scaleNote} onCommit={setSy} num={chainSy} /></div>
      </div>
      <div className="cp-group narrow" data-cp="scaleChain"><Btn icon="chain" title={tr('비율 유지')} on={scaleChain} onClick={() => setScaleChain((v) => !v)} /></div>
      <div className="cp-group" data-cp="rotate">
        <div className="cp-row"><Field icon="rotate" value={deg(r?.rot)} width={64} title={tr("회전 각도 (시계 반대 방향 +)")} onCommit={setRot} num={nrot} /></div>
        <div className="cp-row"><Field icon="shear" value={deg(r?.shear)} width={64} title={tr('기울이기 각도')} onCommit={setShear} num={nshear} /></div>
      </div>
      <div className="cp-group grid2" data-cp="turnFlip">
        <Btn icon="rotateCw" title={tr("시계 방향으로 90° 회전")} onClick={turn?.(-90)} />
        <Btn icon="flipH" title={tr("가로로 뒤집기")} onClick={flip?.('horizontal')} />
        <Btn icon="rotateCcw" title={tr("시계 반대 방향으로 90° 회전")} onClick={turn?.(90)} />
        <Btn icon="flipV" title={tr("세로로 뒤집기")} onClick={flip?.('vertical')} />
      </div>
      <div className="cp-group grid2" data-cp="select">
        <Btn icon="selContainer" title={tr("컨테이너 선택 (Esc)")} onClick={() => p.onCommand('object.selectContainer')} disabled={!hasSel} />
        <Btn icon="selPrev" title={tr("이전 개체 선택")} onClick={() => p.onCommand('object.selectNextAbove')} disabled={!hasSel} />
        <Btn icon="selContent" title={tr("내용 선택 (Shift+Esc)")} onClick={() => p.onCommand('object.selectContent')} disabled={!hasSel} />
        <Btn icon="selNext" title={tr("다음 개체 선택")} onClick={() => p.onCommand('object.selectNextBelow')} disabled={!hasSel} />
      </div>
      <div className="cp-group" data-cp="paint">
        {p.onPaint && p.info
          ? <>
            <div className="cp-row"><span className="cp-label wide">{tr('칠')}</span><SwatchSelect paint={fill} swatches={p.info.swatches} disabled={!can} label={tr('칠 견본')} onPick={(id) => p.onPaint!('fill', id)} /></div>
            <div className="cp-row"><span className="cp-label wide">{tr('획')}</span><SwatchSelect paint={stroke} swatches={p.info.swatches} disabled={!can} label={tr('획 견본')} onPick={(id) => p.onPaint!('stroke', id)} /></div>
          </>
          : <>
            <div className="cp-row"><span className="cp-label wide">{tr('칠')}</span><Combo title={tr('칠 견본')} value={paintName(fill)} chip={fill && fill.kind !== 'none' ? rgbCss(fill.rgb) : null} width={92} /></div>
            <div className="cp-row"><span className="cp-label wide">{tr('획')}</span><Combo title={tr('획 견본')} value={paintName(stroke)} chip={stroke && stroke.kind !== 'none' ? rgbCss(stroke.rgb) : null} width={92} /></div>
          </>}
      </div>
      <div className="cp-group" data-cp="stroke">
        <div className="cp-row"><Field icon="stroke" value={stroke ? formatLength(stroke.weight, strokeUnit) : ''} width={64} title={tr("획 두께")} onCommit={setStroke}
          num={setStroke ? { value: stroke ? stroke.weight : null, format: (v) => formatLength(v, strokeUnit), parse: (t) => { const v = parseLength(t, strokeUnit); return v !== null && v >= 0 ? v : null; }, min: 0, max: 1e4, step: unitOf(strokeUnit).kind ? 1 : 1 / unitOf(strokeUnit).perPoint, apply: (v) => p.onStrokeWeight(v) } : undefined} /></div>
        <div className="cp-row">
          <select className="cp-combo cp-select" style={{ width: 88 }} title={tr('획 유형')} aria-label={tr('획 유형')} disabled={!strokeEditable}
            value={kind ?? ''} onChange={(e) => p.onStroke({ kind: e.target.value }, tr('획 유형'))}>
            {(kind === null || !STROKE_TYPES.some(([id]) => id === kind)) && <option value={kind ?? ''} disabled>{kind ? styleName(kind) : ''}</option>}
            {STROKE_TYPES.map(([id, label]) => <option key={id} value={id}>{label()}</option>)}
          </select>
        </div>
      </div>
      <div className="cp-group" data-cp="effects">
        <div className="cp-row"><Field icon="effects" value={opacityShown === null ? '' : `${trim(opacityShown, 1)}%`} width={56} title={tr('불투명도')} onCommit={setOpacity}
          num={setOpacity ? { value: opacityShown, format: (v) => `${trim(v, 1)}%`, parse: parsePercent, min: 0, max: 100, step: 1, apply: (v) => p.onOpacity(v) } : undefined} /></div>
        <div className="cp-row"><Btn icon="fx" title={tr('효과')} onClick={p.onEffects} /><Btn icon="effects" title={tr('그림자')} onClick={p.onDropShadow} /></div>
      </div>
      <div className="cp-group grid3 wrap" data-cp="wrap">
        {WRAP_BUTTONS.map(([mode, icon, title, model]) => (
          <Btn key={mode} icon={icon} title={title()} on={(it?.text_wrap?.mode ?? '') === model && !!it}
            onClick={p.onTextWrap && p.editable && it && !it.locked && p.selection.length === 1 && !p.selection[0].content ? () => p.onTextWrap!(mode) : undefined} />
        ))}
        <Btn icon="textFrameOptions" title={tr('텍스트 프레임 옵션 (Ctrl+B)')} onClick={p.editable && it?.kind === 'textFrame' ? () => p.onCommand('object.textFrameOptions') : undefined} />
      </div>
      <div className="cp-group" data-cp="corners">
        <div className="cp-row"><Btn icon="cornerOptions" title={tr('모퉁이 옵션')} onClick={corner ? () => p.onCommand('object.cornerOptions') : undefined} /><Field value={cc.radius === null ? '' : L(cc.radius)} width={60} title={tr('모퉁이 크기')} onCommit={setCornerSize} num={cornerNum} /></div>
        <div className="cp-row">
          <select className="cp-combo cp-select" style={{ width: 92 }} title={tr('모퉁이 모양')} aria-label={tr('모퉁이 모양')} disabled={!corner}
            value={cc.shape ?? ''} onChange={(e) => p.onCorners((cs) => cs.map((c) => ({ ...c, shape: e.target.value as CornerShape })), tr('모퉁이 모양'))}>
            {cc.shape === null && <option value="" disabled />}
            {SHAPES.map((s) => <option key={s} value={s}>{shapeLabel(s)}</option>)}
          </select>
        </div>
      </div>
      <div className="cp-group grid2" data-cp="fitting">
        <Btn icon="fillFrame" title={tr("프레임을 비율에 맞게 채우기 (Ctrl+Alt+Shift+C)")} onClick={fitWith('fill')} />
        <Btn icon="fitProportional" title={tr("내용을 비율에 맞게 맞추기 (Ctrl+Alt+Shift+E)")} onClick={fitWith('proportionally')} />
        <Btn icon="fitContent" title={tr("내용을 프레임에 맞추기 (Ctrl+Alt+E)")} onClick={fitWith('contentToFrame')} />
        <Btn icon="fitFrame" title={tr("프레임을 내용에 맞추기 (Ctrl+Alt+C)")} onClick={fitWith('frameToContent')} />
      </div>
      <div className="cp-group grid3" data-cp="align">
        {ALIGN_BUTTONS.map((b) => <Btn key={b.how} icon={b.icon} title={tr('{0} 정렬', {0:b.title})}
          disabled={!p.alignment.canAlign} onClick={() => p.alignment.run('align', b.how)} />)}
      </div>
      <div className="cp-group narrow" data-cp="alignTo">
        <AlignReference control={p.alignment} compact />
        <button className="al-panel-link" onClick={() => p.onCommand('panel.align')}>{tr("정렬 패널…")}</button>
      </div>
      <div className="cp-group" data-cp="objectStyle">
        <div className="cp-row"><Icon name="objectStyles" size={14} className="cp-ficon" />
          {p.onObjectStyle && p.info
            ? <select className="cp-combo cp-select" style={{ width: 140 }} title={tr('개체 스타일')} aria-label={tr('개체 스타일')} disabled={!can}
                value={p.info.objectStyles.some((s) => s.id === it?.style) ? it!.style! : ''} onChange={(e) => p.onObjectStyle!(e.target.value)}>
                {!p.info.objectStyles.some((s) => s.id === it?.style) && <option value="" disabled></option>}
                {p.info.objectStyles.map((s) => <option key={s.id} value={s.id}>{[...s.group, s.name].join(' / ')}</option>)}
              </select>
            : <Combo value="" width={140} title={tr("개체 스타일 (적용된 스타일은 편집 엔진과 함께 읽습니다)")} />}
        </div>
        <div className="cp-row cp-meta">{selectionText(p, r)}</div>
      </div>
      <div className="cp-fill" />
      <ControlBarMenu customizable />
    </div>
  );
}

function paintName(paint: ItemInfo['fill'] | undefined): string {
  if (!paint) return '';
  if (paint.kind === 'none') return tr("[없음]");
  const n = paint.swatch ?? (paint.kind === 'gradient' ? tr("그레이디언트") : tr("색상"));
  return paint.tint !== null && paint.tint !== undefined && paint.tint !== 100 ? `${n} ${trim(paint.tint, 1)}%` : n;
}

const KIND: Record<string, string> = { get textFrame() { return tr("텍스트 프레임"); }, get rectangle() { return tr("사각형"); }, get oval() { return tr("타원"); }, get polygon() { return tr("다각형"); }, get line() { return tr("선"); }, get group() { return tr("그룹"); }, get other() { return tr("개체"); } };

function selectionText(p: Props, r: Readout | null) {
  if (p.selection.length > 1) return tr('개체 {0}개 선택', {0:p.selection.length});
  if (!r?.item) return p.readOnly ? tr("읽기 전용 문서") : '';
  if (r.content && r.item.content) {
    const c = r.item.content;
    return `${c.link ?? c.kind}${c.ppi ? ` · ${tr('유효 {0} ppi', {0:Math.round(Math.min(c.ppi[0], c.ppi[1]))})}` : ''}`;
  }
  return `${KIND[r.item.kind] ?? r.item.kind}${r.item.name ? ` · ${r.item.name}` : ''}${r.item.locked ? ` · ${tr('잠김')}` : ''}`;
}

function TextControls(p: Props) {
  return <TextFormatBar control={p.textFormatting} mode={p.textMode} setMode={p.setTextMode}/>;
}

function PageControls(p: Props) {
  const bar = useControlBar();
  const pg = p.selectedPage;
  const can = p.editable && !!pg;
  const resize = (axis: 'h' | 'v', value: string) => {
    const size = parseLength(value, axis === 'h' ? p.units.horizontal : p.units.vertical);
    if (pg && size !== null && size >= 1 && size <= 15552) p.onPageResize(axis === 'h' ? size : pg.width, axis === 'v' ? size : pg.height);
  };
  const L = (v: number | undefined, axis: 'h' | 'v' = 'h') => (v === undefined ? '' : formatLength(v, axis === 'h' ? p.units.horizontal : p.units.vertical));
  // the Page Size list and the preset the page is, both from the engine (query pageSizePresets: the names InDesign's DOM accepts)
  const [sizes, setSizes] = useState<string[]>([]);
  const [preset, setPreset] = useState<string | null>(null);
  useEffect(() => {
    if (!pg || !window.folio?.request) { setPreset(null); return; }
    let alive = true;
    window.folio.request({ cmd: 'pageSizePresets', width: pg.width, height: pg.height }).then(({ header }) => {
      if (!alive || !header || header.ok === false) return;
      const h = header as unknown as { presets?: { name: string }[]; preset?: string | null };
      if (h.presets) setSizes(h.presets.map((x) => x.name));
      setPreset(h.preset ?? null);
    }).catch(() => undefined);
    return () => { alive = false; };
  }, [pg?.width, pg?.height]);
  // the page's width and height as numbers: a trial is shown while the page changes, so its start is taken when the trial begins
  const origin = pg ? pageOf(p.info, pg.index)?.spreadOrigin : undefined;
  const pageNum = (axis: 'h' | 'v'): Num | undefined => {
    if (!can || !pg) return undefined;
    const name = axis === 'h' ? p.units.horizontal : p.units.vertical;
    return {
      value: axis === 'h' ? pg.width : pg.height, format: (v) => formatLength(v, name),
      parse: (t) => { const v = parseLength(t, name); return v !== null && v >= 1 && v <= 15552 ? v : null; },
      min: 1, max: 15552, step: unitOf(name).kind ? 1 : 1 / unitOf(name).perPoint,
      snapshot: () => [pg.width, pg.height, origin?.[0] ?? 0, origin?.[1] ?? 0],
      apply: (v, _base, start) => p.onPageResize(axis === 'h' ? v : (start?.[0] ?? pg.width), axis === 'v' ? v : (start?.[1] ?? pg.height), start),
    };
  };
  // the page's place in its spread: typing a value moves the page by the difference (the spread then arranges its pages again,
  // as InDesign's Page.reframe does: the objects stay where they are, so they shift against the page)
  const offset = pg ? pageOf(p.info, pg.index)?.spreadOffset : undefined;
  const placeNum = (axis: 'h' | 'v'): Num | undefined => {
    if (!can || !pg || !offset || !p.onPageMove) return undefined;
    const name = axis === 'h' ? p.units.horizontal : p.units.vertical, k = axis === 'h' ? 0 : 1;
    return {
      value: offset[k], format: (v) => formatLength(v, name),
      parse: (t) => { const v = parseLength(t, name); return v !== null && Math.abs(v) <= 15552 ? v : null; },
      min: -15552, max: 15552, step: unitOf(name).kind ? 1 : 1 / unitOf(name).perPoint,
      snapshot: () => [offset[0], offset[1], origin?.[0] ?? 0, origin?.[1] ?? 0, pg.width, pg.height],
      apply: (v, _base, start) => {
        const from = start ?? [offset[0], offset[1], origin?.[0] ?? 0, origin?.[1] ?? 0, pg.width, pg.height];
        p.onPageMove?.(axis === 'h' ? v - from[0] : 0, axis === 'v' ? v - from[1] : 0, from.slice(2));
      },
    };
  };
  const place = (axis: 'h' | 'v') => (value: string) => {
    const name = axis === 'h' ? p.units.horizontal : p.units.vertical, v = parseLength(value, name);
    if (v !== null && offset && Math.abs(v) <= 15552) p.onPageMove?.(axis === 'h' ? v - offset[0] : 0, axis === 'v' ? v - offset[1] : 0);
  };
  return (
    <div className="control page-bar">
      <HiddenGroups hidden={bar.page} bar="page" />
      <div className="cp-group" data-cp="page.reference"><RefProxy value={p.refPoint} onChange={p.setRefPoint} /></div>
      <div className="cp-group" data-cp="page.position">
        <div className="cp-row"><Field label="X:" value={pg ? L(offset?.[0] ?? 0) : ''} title={tr('페이지 X')} onCommit={can && p.onPageMove ? place('h') : undefined} num={placeNum('h')} /></div>
        <div className="cp-row"><Field label="Y:" value={pg ? L(offset?.[1] ?? 0, 'v') : ''} title={tr('페이지 Y')} onCommit={can && p.onPageMove ? place('v') : undefined} num={placeNum('v')} /></div>
      </div>
      <div className="cp-group" data-cp="page.size">
        <div className="cp-row"><Field label="W:" value={pg ? L(pg.width) : ''} title={tr('페이지 폭')} onCommit={can ? (v) => resize('h', v) : undefined} num={pageNum('h')} /></div>
        <div className="cp-row"><Field label="H:" value={pg ? L(pg.height, 'v') : ''} title={tr('페이지 높이')} onCommit={can ? (v) => resize('v', v) : undefined} num={pageNum('v')} /></div>
      </div>
      <div className="cp-group" data-cp="page.preset">
        <div className="cp-row"><span className="cp-label wide">{tr('크기')}</span>
          <select className="cp-combo cp-select" style={{ width: 130 }} title={tr('페이지 크기')} aria-label={tr('페이지 크기')} data-field="pageSizePreset" disabled={!can || !p.onPagePreset} value={pg ? preset ?? '' : ''}
            onChange={(e) => { if (e.target.value) p.onPagePreset?.(e.target.value); }}>
            <option value="" disabled={preset !== null}>{tr('사용자 정의')}</option>
            {sizes.map((n) => <option key={n} value={n}>{n}</option>)}
          </select></div>
        <div className="cp-row"><Btn icon="pages" title={tr('세로 방향')} disabled={!can} on={!!pg && pg.height >= pg.width} onClick={() => { if (pg && pg.width > pg.height) p.onPageResize(pg.height, pg.width); }} /><Btn icon="screenPreview" title={tr('가로 방향')} disabled={!can} on={!!pg && pg.width > pg.height} onClick={() => { if (pg && pg.height > pg.width) p.onPageResize(pg.height, pg.width); }} /></div>
      </div>
      <div className="cp-group" data-cp="page.options">
        <div className="cp-row"><label className="cp-check" title={tr('페이지를 옮길 때 그 위의 개체와 안내선도 함께 옮깁니다. 잠긴 개체와 대지의 개체는 그대로 있습니다.') + '\n' + tr('맞쪽·여러 쪽 펼침면의 쪽은 이 옵션을 켜면 끌어 옮길 수 없습니다.')}>
          <input type="checkbox" data-field="objectsMoveWithPage" disabled={!p.onObjectsMoveWithPage} checked={!!p.objectsMoveWithPage} onChange={(e) => p.onObjectsMoveWithPage?.(e.target.checked)} /> {tr('개체 함께 이동')}</label></div>
        <div className="cp-row"><label className="cp-check" title={tr('기준 페이지 오버레이 표시') + '\n' + tr('지원하지 않음')}><input type="checkbox" disabled /> {tr('기준 페이지 오버레이 표시')}</label></div>
      </div>
      <div className="cp-group" data-cp="page.liquid">
        <div className="cp-row"><span className="cp-label wide">{tr('액체 페이지 규칙')}</span>
          <select className="cp-combo cp-select" style={{ width: 150 }} disabled title={tr('액체 페이지 규칙') + '\n' + tr('지원하지 않음')} aria-label={tr('액체 페이지 규칙')} data-field="liquidPageRule"><option>{tr('기준 페이지가 정함')}</option></select></div>
      </div>
      <div className="cp-group" data-cp="page.info"><div className="cp-row cp-meta">{pg ? tr('{0}번째 페이지', {0:pg.index + 1}) : tr("페이지를 누르세요")}</div></div>
      <div className="cp-fill" />
      <ControlBarMenu bar="page" />
    </div>
  );
}
