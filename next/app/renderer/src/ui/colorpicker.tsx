// The Color Picker dialog (InDesign: double-click the Fill or Stroke chip of the toolbox or of the Color panel). A colour field and a vertical
// slider that follows one of nine components (H S B, R G B, L a b), the new colour over the current one in one box, number fields in HSB, RGB,
// Lab and CMYK, a hexadecimal field, an eyedropper and one Add Swatch button whose name follows the radio button. Layout (710 x 467 px, every
// box at the position measured in InDesign 2025), keys (no arrow keys on the field or slider, Tab order, Enter = OK, alerts for numbers outside
// their range, six-digit hexadecimal only) and the missing gamut warning are measured: next/oracle/rules/colorpicker-c2.ko.md.
// The profile-free maths is ui/colorpickermath.ts; CMYK comes from the engine query colorPick (folio_color::pick_color).
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { rgbToLab } from '../features/color/convert';
import './colorpicker.css';
import { NumberField } from './controls/numberfield';
import { RANGE, AXIS, atPosition, clamp, comps, fieldPixels, fromRgb, hexOf, parseHex, position, sliderPixels, withComps, type Axis, type Mode, type V3, type Working } from './colorpickermath';

export type PickSpace = 'RGB' | 'CMYK' | 'LAB';
/** The kind of swatch the Add Swatch button makes: the component radio decides (H S B: HSB, R G B: the colour's own space, L a b: Lab). */
export type SwatchKind = PickSpace | 'HSB';
/** A colour as the picker hands it back: the values in the space the user last worked in (RGB when a field, slider, HSB or hex made it). `name`: the swatch name InDesign gives it. */
export type PickedColor = { space: SwatchKind; value: number[]; rgb: V3; name?: string };
export type PickerInitial = { rgb: V3; exact: { space: PickSpace; value: number[] } | null; none: boolean };
type Answer = { cmyk: number[]; rgb: number[]; inGamut: boolean };

const round = (v: number, digits = 0) => { const k = 10 ** digits; return Math.round(v * k) / k; };
const css = (c: number[]) => `rgb(${c.map((v) => Math.round(v)).join(',')})`;

/** InDesign's swatch name for a value, every number rounded to a whole one: R=255 G=0 B=0, C=0 M=50 Y=100 K=0, L=50 a=0 b=0, H=137 S=15 B=55. */
export function pickedName(c: { space: SwatchKind; value: number[] }): string {
  const keys = c.space === 'CMYK' ? ['C', 'M', 'Y', 'K'] : c.space === 'LAB' ? ['L', 'a', 'b'] : c.space === 'HSB' ? ['H', 'S', 'B'] : ['R', 'G', 'B'];
  return keys.map((k, i) => `${k}=${Math.round(c.value[i] ?? 0)}`).join(' ');
}

// Positions are InDesign's, in px from the dialog's top left (title bar included); `y` is the top of the row's label, its box starts 4 px lower.
const ROWS: { mode: Mode; i: 0 | 1 | 2; key: string; unit: string; axis: Axis; x: number; y: number }[] = [
  { mode: 'hsb', i: 0, key: 'H', unit: '°', axis: 'h', x: 398, y: 193 }, { mode: 'hsb', i: 1, key: 'S', unit: '%', axis: 's', x: 398, y: 226 }, { mode: 'hsb', i: 2, key: 'B', unit: '%', axis: 'v', x: 398, y: 258 },
  { mode: 'rgb', i: 0, key: 'R', unit: '', axis: 'r', x: 398, y: 303 }, { mode: 'rgb', i: 1, key: 'G', unit: '', axis: 'g', x: 398, y: 336 }, { mode: 'rgb', i: 2, key: 'B', unit: '', axis: 'b', x: 398, y: 368 },
  { mode: 'lab', i: 0, key: 'L', unit: '', axis: 'l', x: 549, y: 192 }, { mode: 'lab', i: 1, key: 'a', unit: '', axis: 'a', x: 549, y: 224 }, { mode: 'lab', i: 2, key: 'b', unit: '', axis: 'bl', x: 549, y: 257 },
];
const INKS = ['C', 'M', 'Y', 'K'];
const INK_Y = [302, 334, 367, 399];
const ROW = (n: number) => ROWS[n];
// Tab order measured in InDesign: H S B, the radio group, R G B, hex, eyedropper and buttons, L a b, C M Y K (DOM order = Tab order; the boxes sit where `style` says)
const TAB_LEFT = [0, 1, 2], TAB_RGB = [3, 4, 5], TAB_LAB = [6, 7, 8];

/** A number box of the picker: the shared field (arrows: 1, Shift: 10, wheel, scrubbing the letter beside it). A number outside the range, or not a number, is refused
 *  with InDesign's alert and the box goes back to what it had. */
export function ColorNumber({ label, value, min, max, unit, name, digits = 0, onCommit, onInvalid, style, unitX }: { label: string; value: number | null; min: number; max: number; unit?: string; name: string; digits?: number; onCommit: (n: number) => void; onInvalid?: (message: string) => void; style: React.CSSProperties; unitX: number }) {
  return (
    <>
      <div className="cp-box" style={style}>
        <NumberField value={value} label={label} min={min} max={max} step={1} decimals={digits} width="100%" disabled={value === null} inDialog spin={false}
          inputData={{ 'data-cp-field': name }} scrubRowLabel={false}
          parse={(t) => {
            const text = t.trim().replace(',', '.');
            if (text === '') return null;
            const n = Number(text);
            if (!Number.isFinite(n)) { onInvalid?.(tr('숫자가 올바르지 않습니다.')); return null; }
            if (n < min || n > max) { onInvalid?.(tr('값은 {0}에서 {1} 사이여야 합니다.', { 0: min, 1: max })); return null; }
            return n;
          }}
          onCommit={onCommit} />
      </div>
      {unit ? <span className="cp-unit" style={{ left: unitX, top: (style.top as number) - 4 }}>{unit}</span> : null}
    </>
  );
}

function paint(canvas: HTMLCanvasElement | null, pixels: Uint8ClampedArray, w: number, h: number) {
  const ctx = canvas?.getContext('2d');
  if (!canvas || !ctx) return;
  canvas.width = w; canvas.height = h;
  ctx.putImageData(new ImageData(new Uint8ClampedArray(pixels), w, h), 0, 0);
}

type EyeDropperApi = { open: () => Promise<{ sRGBHex: string }> };
const eyeDropper = (): EyeDropperApi | null => { const E = (window as unknown as { EyeDropper?: new () => EyeDropperApi }).EyeDropper; return E ? new E() : null; };

export function ColorPickerDialog({ target, initial, canApply, onOk, onCancel, onAddSwatch }: {
  target: 'fill' | 'stroke'; initial: PickerInitial; canApply: boolean;
  onOk: (c: PickedColor) => void; onCancel: () => void; onAddSwatch: (c: PickedColor) => Promise<string | null> | string | null;
}) {
  const [w, setW] = useState<Working>(() => fromRgb(initial.rgb));
  // the values the user last gave in the document's own spaces (CMYK, Lab): they are applied as typed, not through RGB
  const [src, setSrc] = useState<{ space: PickSpace; value: number[] } | null>(initial.exact && initial.exact.space !== 'RGB' ? initial.exact : null);
  // InDesign opens on the R radio (the L radio for a Lab colour)
  const [axis, setAxis] = useState<Axis>(initial.exact?.space === 'LAB' ? 'l' : 'r');
  const [answer, setAnswer] = useState<Answer | null>(null);
  const [note, setNote] = useState('');
  const [alert, setAlert] = useState<string | null>(null);
  const [dirty, setDirty] = useState(false);
  const field = useRef<HTMLDivElement>(null), slider = useRef<HTMLDivElement>(null);
  const fieldCanvas = useRef<HTMLCanvasElement>(null), sliderCanvas = useRef<HTMLCanvasElement>(null);
  const sequence = useRef(0);
  const alertOpen = useRef(false), submitRef = useRef<() => void>(() => undefined);

  const rgb8 = w.rgb.map((v) => Math.round(v)) as V3;
  const current: PickedColor = src ? { space: src.space, value: src.value, rgb: w.rgb } : { space: 'RGB', value: rgb8, rgb: w.rgb };
  const pos = position(axis, w);
  const lab = src?.space === 'LAB' ? src.value : rgbToLab(w.rgb);
  const cmyk = src?.space === 'CMYK' ? src.value : answer?.cmyk ?? null;
  const group = AXIS[axis].mode;
  const origin: PickSpace = initial.exact?.space ?? 'RGB';
  const kind: SwatchKind = group === 'hsb' ? 'HSB' : group === 'lab' ? 'LAB' : origin;

  // the field and the slider are drawn again when the component the slider carries changes, the slider when the field's own components do
  const k = round(pos.k, 4);
  useEffect(() => { paint(fieldCanvas.current, fieldPixels(axis, k, 160), 160, 160); }, [axis, k]);
  const sliderKey = `${axis}:${round(pos.x, 3)}:${round(pos.y, 3)}`;
  useEffect(() => { paint(sliderCanvas.current, sliderPixels(axis, w, 256), 1, 256); }, [sliderKey]);

  // CMYK: the engine converts (colour management belongs to the engine); the newest answer wins
  const asked = JSON.stringify(src ?? ['RGB', rgb8]);
  useEffect(() => {
    const id = ++sequence.current;
    const request = src ? { space: src.space, value: src.value } : { space: 'RGB', value: rgb8 };
    const timer = window.setTimeout(() => {
      window.folio.request({ cmd: 'colorPick', ...request }).then(({ header }) => {
        if (id !== sequence.current || !header || header.ok === false) return;
        const a = header as unknown as Answer;
        setAnswer(a);
        // a CMYK colour typed in: the working colour becomes what the document shows it as
        if (request.space === 'CMYK') setW((prev) => fromRgb(a.rgb as V3, prev));
      }).catch(() => undefined);
    }, 30);
    return () => window.clearTimeout(timer);
  }, [asked]);

  const refuse = (message: string) => { alertOpen.current = true; setAlert(message); };
  const edit = (next: Working, exact: { space: PickSpace; value: number[] } | null = null) => { setW(next); setSrc(exact); setNote(''); setDirty(true); };
  const setComp = (mode: Mode, i: number, v: number) => {
    // Lab edits start from the Lab values shown (typed ones may lie outside what the screen can show)
    const c = (mode === 'lab' ? [...lab] : [...comps(mode, w)]) as V3;
    c[i] = v;
    if (mode === 'lab') edit(withComps('lab', c, w), { space: 'LAB', value: c.map((x) => round(x, 2)) });
    else edit(withComps(mode, c, w));
  };
  const setInk = (i: number, v: number) => {
    const base = cmyk ?? [0, 0, 0, 0];
    setSrc({ space: 'CMYK', value: base.map((x, j) => (j === i ? v : x)) });
    setNote(''); setDirty(true);
  };
  const fromPoint = (e: React.PointerEvent, el: HTMLElement | null, what: 'field' | 'slider') => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    const nx = clamp((e.clientX - r.left) / Math.max(1, r.width), 0, 1), ny = clamp(1 - (e.clientY - r.top) / Math.max(1, r.height), 0, 1);
    edit(atPosition(axis, what === 'field' ? { x: nx, y: ny, k: pos.k } : { x: pos.x, y: pos.y, k: ny }, w));
  };
  // no keys on the field or the slider (InDesign: the arrow keys, Shift, PageUp/Down and Home/End do nothing); Shift while pointing makes no difference
  const drag = (what: 'field' | 'slider', el: React.RefObject<HTMLDivElement | null>) => ({
    onPointerDown: (e: React.PointerEvent) => { if (e.button !== 0) return; try { e.currentTarget.setPointerCapture(e.pointerId); } catch { /* a synthetic event has no pointer to capture */ } fromPoint(e, el.current, what); (e.currentTarget as HTMLElement).dataset.dragging = '1'; },
    onPointerMove: (e: React.PointerEvent) => { if ((e.currentTarget as HTMLElement).dataset.dragging === '1') fromPoint(e, el.current, what); },
    onPointerUp: (e: React.PointerEvent) => { delete (e.currentTarget as HTMLElement).dataset.dragging; try { e.currentTarget.releasePointerCapture(e.pointerId); } catch { /* see above */ } },
  });
  // Add Swatch: the swatch InDesign makes for the radio (it stays open; a second click makes a second swatch)
  const addSwatch = async () => {
    const value = kind === 'CMYK' ? cmyk?.map((x) => Math.round(x)) : kind === 'LAB' ? lab.map((x) => Math.round(x)) : rgb8;
    if (!value) return;
    const name = pickedName({ space: kind, value: kind === 'HSB' ? w.hsb : value });
    const made = await onAddSwatch({ space: kind, value, rgb: w.rgb, name });
    setNote(made === null ? tr('견본을 추가하지 못했습니다.') : tr('견본을 추가했습니다: {0}', { 0: made }));
  };
  // OK without a change leaves the colour (and the swatch it came from) alone
  const submit = () => { if (!dirty) onCancel(); else if (canApply) onOk(current); };
  submitRef.current = submit;
  // H, S and B round a tie down (InDesign 2025: RGB 250 240 10 shows H 57, 24 37 48 shows H 207)
  const shownValue = (r: typeof ROWS[number]): number => (r.mode === 'hsb' ? Math.ceil(w.hsb[r.i] - 0.5) : r.mode === 'rgb' ? w.rgb[r.i] : lab[r.i]);
  const sample = () => {
    const dropper = eyeDropper();
    if (!dropper) { setNote(tr('이 화면에서는 화면에서 색 따기를 쓸 수 없습니다.')); return; }
    dropper.open().then((r) => { const c = parseHex(r.sRGBHex); if (c) edit(fromRgb(c, w)); }).catch(() => undefined);
  };
  const numberBox = (n: number) => {
    const r = ROW(n);
    return (
      <ColorNumber key={r.axis} label={tr('{0} 값', { 0: r.key })} name={`${r.mode}${r.i}`} digits={0} value={shownValue(r)}
        min={RANGE[r.mode][r.i][0]} max={RANGE[r.mode][r.i][1]} unit={r.unit} unitX={r.x === 398 ? 508 : 657} style={{ left: r.x === 398 ? 453 : 602, top: r.y + 4 }}
        onInvalid={refuse} onCommit={(v) => setComp(r.mode, r.i, v)} />
    );
  };
  const radio = (r: typeof ROWS[number]) => (
    <label key={r.axis} className="cp-radio" style={{ left: r.x, top: r.y }}>
      <input type="radio" name="cp-axis" checked={axis === r.axis} data-cp-axis={r.axis} aria-label={tr('슬라이더를 {0}에 맞춤', { 0: r.key })} onChange={() => setAxis(r.axis)} />
      <span>{r.key}:</span>
    </label>
  );
  const viewName = group === 'hsb' ? tr('HSB 색 공간 보기') : group === 'lab' ? tr('Lab 색 공간 보기') : tr('RGB 색 공간 보기');
  const addLabel = kind === 'HSB' ? tr('HSB 견본 추가') : kind === 'CMYK' ? tr('CMYK 견본 추가') : kind === 'LAB' ? tr('Lab 견본 추가') : tr('RGB 견본 추가');

  return createPortal(
    <div className="cp-modal" onPointerDown={(e) => e.stopPropagation()}>
      <div className="cp-dialog" role="dialog" aria-label={target === 'stroke' ? tr('획 색상 선택') : tr('칠 색상 선택')} data-cp="dialog" data-cp-target={target}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (alertOpen.current) {
            if (e.key === 'Enter' || e.key === 'Escape') { e.preventDefault(); alertOpen.current = false; setAlert(null); }
            return;
          }
          if (e.key === 'Escape') { e.preventDefault(); onCancel(); }
          else if (e.key === 'Enter' && !(e.target instanceof HTMLButtonElement)) {
            e.preventDefault();
            // a number typed in a box is taken first (the box does that on this same key), then OK; a refused number keeps the dialog open
            if (e.target instanceof HTMLInputElement && e.target.type === 'text') window.setTimeout(() => { if (!alertOpen.current) submitRef.current(); }, 0);
            else submit();
          }
        }}>
        <div className="cp-title">{tr('색상 선택기')}</div>
        <div className="cp-view" data-cp="view">{viewName}</div>
        {/* the Tab order is InDesign's: H S B, the radio group, R G B, hex, the eyedropper and buttons, L a b, C M Y K */}
        {TAB_LEFT.map(numberBox)}
        {ROWS.map(radio)}
        {TAB_RGB.map(numberBox)}
        <span className="cp-hexlabel" style={{ left: 398, top: 413 }}>#:</span>
        <HexField value={hexOf(w.rgb)} onCommit={(c) => edit(fromRgb(c, w))} />
        <button type="button" className="cp-eye" data-cp="eyedropper" style={{ left: 503, top: 413 }} onClick={sample} aria-label={tr('화면에서 색 따기')}
          title={tr('화면 어디서든 색을 땁니다. 누른 뒤 따려는 곳을 누르세요.')}>
          <svg viewBox="0 0 16 16" width="16" height="16" aria-hidden="true"><path d="M11.5 1.5l3 3-2 2-.8-.8-5.1 5.1-.9 2.7-1.3-.2-.2-1.3 2.7-.9 5.1-5.1-.8-.8z" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" /></svg>
        </button>
        <button type="button" className="cpk-btn" data-cp="add" data-cp-kind={kind} style={{ left: 530, top: 147 }} disabled={kind === 'CMYK' && !cmyk} onClick={() => void addSwatch()}>{addLabel}</button>
        <button type="button" className="cpk-btn" data-cp="cancel" style={{ left: 530, top: 102 }} onClick={onCancel}>{tr('취소')}</button>
        <button type="button" className="cpk-btn primary" data-cp="ok" style={{ left: 530, top: 57 }} disabled={!canApply && dirty} onClick={submit}>{tr('확인')}</button>
        {TAB_LAB.map(numberBox)}
        {INKS.map((key, i) => (
          <ColorNumber key={key} label={tr('{0} 값', { 0: key })} name={`cmyk${i}`} digits={0} value={cmyk ? cmyk[i] : null} min={0} max={100} unit="%" unitX={657}
            style={{ left: 602, top: INK_Y[i] + 4 }} onInvalid={refuse} onCommit={(v) => setInk(i, v)} />
        ))}
        {INKS.map((key, i) => <span key={key} className="cp-k" style={{ left: 549, top: INK_Y[i] }}>{key}:</span>)}

        <div className="cp-area" ref={field} role="group" aria-label={tr('색 영역')} data-cp="field" data-x={pos.x.toFixed(4)} data-y={pos.y.toFixed(4)} {...drag('field', field)}>
          <canvas ref={fieldCanvas} width={160} height={160} />
          <i className="cp-ring" style={{ left: `${pos.x * 100}%`, top: `${(1 - pos.y) * 100}%` }} />
        </div>
        <div className="cp-slider" ref={slider} role="slider" aria-label={tr('색 슬라이더')} aria-orientation="vertical" aria-valuemin={0} aria-valuemax={100}
          aria-valuenow={Math.round(pos.k * 100)} data-cp="slider" data-k={pos.k.toFixed(4)} {...drag('slider', slider)}>
          <canvas ref={sliderCanvas} width={1} height={256} />
          <i className="cp-arrow l" style={{ top: `${(1 - pos.k) * 100}%` }} /><i className="cp-arrow r" style={{ top: `${(1 - pos.k) * 100}%` }} />
        </div>
        {/* new colour on top, the current colour below, in one box */}
        <div className="cp-preview" data-cp="preview">
          <div data-cp="new" style={{ background: css(w.rgb) }} title={tr('새 색')} />
          <div data-cp="old" className={initial.none ? 'none' : ''} style={initial.none ? undefined : { background: css(initial.rgb) }} title={tr('현재 색')} />
        </div>
        <div className="cp-note" role="status" aria-live="polite" data-cp="note">{note || (canApply ? '' : tr('선택한 개체가 없어 칠과 획을 바꿀 수 없습니다. 견본은 추가할 수 있습니다.'))}</div>
        {alert !== null && (
          <div className="cp-alertbg" onPointerDown={(e) => e.stopPropagation()}>
            <div className="cp-alert" role="alertdialog" aria-label={tr('색상 선택기')} data-cp="alert">
              <p>{alert}</p>
              <button type="button" className="cpk-btn primary" data-cp="alert-ok" autoFocus onClick={() => { alertOpen.current = false; setAlert(null); }}>{tr('확인')}</button>
            </div>
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}

/** The hexadecimal box: exactly six hexadecimal digits are a colour (any case, shown in lower case); anything else is ignored without a word and stays as typed. */
function HexField({ value, onCommit }: { value: string; onCommit: (c: V3) => void }) {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => { setText(null); }, [value]);
  const done = () => { const c = text === null || !/^[0-9a-fA-F]{6}$/.test(text) ? null : parseHex(text); if (c) { setText(null); onCommit(c); } };
  return (
    <input type="text" className="cp-hex" style={{ left: 428, top: 417 }} aria-label={tr('16진수 색상')} data-cp-field="hex" maxLength={7} value={text ?? value}
      onFocus={(e) => { setText(value); e.currentTarget.select(); }} onChange={(e) => setText(e.target.value)} onBlur={done}
      onKeyDown={(e) => { if (e.key === 'Enter') done(); else if (e.key === 'Escape' && text !== null) setText(null); }} />
  );
}
