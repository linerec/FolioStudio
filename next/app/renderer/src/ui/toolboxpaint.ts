// The toolbox's fill and stroke controls (measured in InDesign 2025 on a rectangle, menu actions 11020, 11021, 11022-11024):
//   Swap Fill and Stroke (Shift+X): the fill and the stroke trade colours and tints, the stroke weight stays;
//   Default Fill and Stroke (D): fill [None], stroke [Black], tints and weight stay;
//   Apply Color (,) / Apply Gradient (.) / Apply None (/): the colour (gradient) last shown in the toolbox, or none, goes to the
//   fill or the stroke, whichever the toolbox has in front; the tint stays. On text (characters, a caret, T-on frames) a gradient goes to the characters' fill
//   or stroke the same way (the DOM `text.fillColor = gradient`; where it sits on the text is the text's own placement, `gradientFill…`). A document that has shown no gradient gets InDesign's default
//   one (white to black, an unnamed hidden swatch: applyDefaultGradient) along the object's width.
// Every one is a setPaint edit of the selected objects (one undo step).
// With T (Formatting Affects Text) on and text frames selected, or with characters selected, the widget shows the paint of the TEXT (the whole story of
// the frames, the range, or the character at the caret; a mixed value shows `?`) and every button acts on the characters; swap trades fill and stroke run
// by run (swapTextPaint / swapFrameTextPaint). Measured in InDesign 2025 with real input: artifacts/toolbox-c2-20261023/STATUS.ko.md.
import { useEffect, useRef, useState } from 'react';
import { t as tr } from '../i18n';
import { textFormat, textStory, type EditCommand, type EditResult, type TextFormat } from '../engine';
import type { DocumentInfo, ItemInfo, Paint } from '../inspect';
import type { TextFormatting } from './textformat';

type Edit = (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
type Paintable = { id: string; fill: Paint; stroke: Paint };
/** T on, text frames selected: what the Gradient tool needs (gradienttools.ts). */
export type ToolboxText = { ids: string[]; /** characters selected (no frames): the range the drag goes to; a caret (`caret`) changes nothing */ range?: { story: string; start: number; end: number } | null; caret?: boolean; front: (stroke: boolean) => Paint | undefined; gradient: () => { prefix: EditCommand[]; swatch: string } };

const NONE = 'Swatch/None', BLACK = 'Color/Black';
/** The default gradient the engine makes (`ensureDefaultGradient`; an unnamed hidden swatch). */
const DEFAULT_GRADIENT = 'Gradient/u7e1b3c01';

export function useToolboxPaint(p: { editable: boolean; textMode: boolean; text: TextFormatting; ids: string[]; items: ItemInfo[]; swatches: DocumentInfo['swatches'] | undefined; strokeFocus: boolean; edit: Edit; /** Changes with every edit of the document: the paint of the frames' text is read again. */ revision?: unknown }) {
  const swatches = p.swatches ?? [];
  const selection = p.editable ? p.text.selection : null;
  // T on, text frames selected (no characters): the paint of their stories, one value when they agree, else mixed (null)
  const frameStories = p.editable && !selection && p.textMode ? [...new Set(p.items.filter((it) => it.text?.story).map((it) => it.text!.story))] : [];
  const framesKey = frameStories.join('|');
  const [framePaint, setFramePaint] = useState<{ key: string; format: TextFormat } | null>(null);
  useEffect(() => {
    if (!frameStories.length) { setFramePaint(null); return; }
    let alive = true;
    void Promise.all(frameStories.map(async (story) => (await textFormat(story, 0, (await textStory(story)).length)).format)).then((formats) => {
      const format: TextFormat = {};
      for (const key of ['fillColor', 'fillTint', 'strokeColor', 'strokeTint']) {
        const first = formats[0]?.[key];
        format[key] = formats.every((f) => f[key] === first) ? first ?? null : null;
      }
      if (alive) setFramePaint({ key: framesKey, format });
    }).catch(() => { if (alive) setFramePaint(null); });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [framesKey, p.revision]);
  const frameFormat = framePaint && framePaint.key === framesKey ? framePaint.format : null;
  const source: TextFormat | null = selection ? selection.format : frameFormat;
  const textPaint = (stroke: boolean): Paint | undefined => {
    if (!source) return undefined;
    const id = source[stroke ? 'strokeColor' : 'fillColor'];
    const tint = source[stroke ? 'strokeTint' : 'fillTint'];
    const swatch = swatches.find(s => s.id === id);
    // an unset fill is [Black], an unset stroke [None]
    if (id === '' || id === undefined) return { kind: stroke ? 'none' : 'swatch', swatch: stroke ? NONE : BLACK, tint: typeof tint === 'number' ? tint : null, rgb: stroke ? null : swatches.find((x) => x.id === BLACK)?.rgb ?? [0, 0, 0] };
    return { kind: id === NONE ? 'none' : swatch?.model === 'gradient' || (typeof id === 'string' && id.startsWith('Gradient/')) ? 'gradient' : id ? 'swatch' : 'other',
      swatch: typeof id === 'string' ? id : null, tint: typeof tint === 'number' ? tint : null,
      rgb: swatch?.rgb ?? null };
  };
  const fill = textPaint(false), stroke = textPaint(true);
  const fill_ = fill, stroke_ = stroke;
  const mixed = (stroke: boolean) => !!source && source[stroke ? 'strokeColor' : 'fillColor'] === null;
  const idOf = (paint: Paint): string | null => (paint.kind === 'none' ? NONE : swatches.find((s) => s.id === paint.swatch || s.name === paint.swatch)?.id ?? null);
  // the colour and the gradient last shown in the front paint of the toolbox
  const lastSolid = useRef(BLACK), lastGradient = useRef<string | null>(null);
  const front = source ? (p.strokeFocus ? stroke : fill) : p.items[0] ? (p.strokeFocus ? p.items[0].stroke : p.items[0].fill) : null;
  if (front && front.kind === 'swatch') { const id = idOf(front); if (id && id !== NONE) lastSolid.current = id; }
  if (front && front.kind === 'gradient') { const id = idOf(front); if (id) lastGradient.current = id; }
  const gradient = lastGradient.current ?? swatches.find((s) => s.model === 'gradient')?.id ?? null;
  const itemById = new Map(p.items.map((it) => [it.id, it]));
  const targets: Paintable[] = p.items.length === p.ids.length ? p.items.map((it) => ({ id: it.id, fill: it.fill, stroke: it.stroke })) : [];
  const can = p.editable && !selection && (!p.textMode || !frameStories.length) && targets.length > 0;
  const run = (label: string, commands: EditCommand[]) => { if (commands.length) void p.edit(label, commands); };
  // T on, text frames selected (no characters): the colour goes to the characters of their stories (formatFrameText: the whole story,
  // threaded frames included; static proof from InDesign 2025, artifacts/tbutton-c2-20261023/STATUS.ko.md). Other objects stay as they are.
  const frameIds = frameStories.length ? p.items.filter((it) => it.text?.story).map((it) => it.id) : [];
  const toFrames = (label: string, props: Record<string, string>) => frameIds.length ? () => run(label, [{ op: 'formatFrameText', ids: frameIds, props }]) : undefined;
  // a tint goes along with the colour it belongs to (a paint of none has none)
  const tint = (paint: Paint) => (paint.kind === 'none' || paint.tint === null ? {} : { tint: paint.tint });
  // Apply Gradient on text (a range, a caret, or the story of T-on frames): the gradient the toolbox last showed, else InDesign's default one
  // (white to black), made in the same edit; the text's own placement stays (automatic unless set: IDML GradientFillStart/Length/Angle).
  const textGradient = (): { prefix: EditCommand[]; swatch: string } => gradient ? { prefix: [], swatch: gradient } : { prefix: [{ op: 'ensureDefaultGradient' }], swatch: DEFAULT_GRADIENT };
  const applyTextGradient = (sel: NonNullable<typeof selection>) => {
    const key = p.strokeFocus ? 'strokeColor' : 'fillColor', g = textGradient();
    if (!g.prefix.length) return p.text.apply({ [key]: g.swatch });
    if (sel.start === sel.end) return void p.edit(tr('그레이디언트 적용'), g.prefix).then(() => p.text.apply({ [key]: g.swatch }));
    run(tr('그레이디언트 적용'), [...g.prefix, { op: 'textFormat', story: sel.story, start: sel.start, end: sel.end, props: { [key]: g.swatch } }]);
  };
  const swap = can ? () => run(tr('칠과 획 맞바꾸기'), targets.flatMap((t): EditCommand[] => {
    const f = idOf(t.fill), s = idOf(t.stroke);
    if (!f || !s) return [];
    const [ft, st] = [tint(t.fill).tint, tint(t.stroke).tint];
    // a gradient's placement (start, length, angle) goes with it (InDesign 2025, toolbox-c3 session 6)
    const it = itemById.get(t.id);
    const place = (g: ItemInfo['fill_gradient'], stroke: boolean): EditCommand[] => g ? [{ op: 'setGradientGeometry', id: t.id, stroke, start: [g.start.x, g.start.y], angle: g.angle, length: g.length }] : [];
    return [{ op: 'setPaint', ids: [t.id], fill: s, stroke: f, ...(st === undefined ? {} : { fillTint: st }), ...(ft === undefined ? {} : { strokeTint: ft }) },
      ...(t.stroke.kind === 'gradient' ? place(it?.stroke_gradient, false) : []), ...(t.fill.kind === 'gradient' ? place(it?.fill_gradient, true) : [])];
  })) : undefined;
  const defaults = can ? () => run(tr('기본 칠과 획'), [{ op: 'setPaint', ids: targets.map((t) => t.id), fill: NONE, stroke: BLACK }]) : undefined;
  const apply = (label: string, swatch: string) => () => run(label, [{ op: 'setPaint', ids: targets.map((t) => t.id), ...(p.strokeFocus ? { stroke: swatch } : { fill: swatch }) }]);
  // Swap Fill and Stroke on characters: a caret trades the paint the next typed text gets (the control bar's caret format); a range, run by run
  // (swapTextPaint: a range of several colours swaps each run, not one pair for the whole range); the stories of T-on frames, the same for the whole story
  const caretSwap = selection && selection.start === selection.end && source ? () => {
    const f = String(source.fillColor || BLACK), s = String(source.strokeColor || NONE);
    const [ft, st] = [source.fillTint, source.strokeTint];
    p.text.apply({ fillColor: s, strokeColor: f, ...(typeof st === 'number' ? { fillTint: st } : {}), ...(typeof ft === 'number' ? { strokeTint: ft } : {}) });
  } : undefined;
  const textSwap = selection ? caretSwap ?? (() => run(tr('칠과 획 맞바꾸기'), [{ op: 'swapTextPaint', story: selection.story, start: selection.start, end: selection.end }]))
    : frameIds.length ? () => run(tr('칠과 획 맞바꾸기'), [{ op: 'swapFrameTextPaint', ids: frameIds }]) : undefined;
  return {
    fill, stroke,
    /** The widget shows the text's paint (T glyphs), and which of the two is mixed (`?`). */
    glyph: !!source, fillMixed: mixed(false), strokeMixed: mixed(true),
    /** T on and text frames selected: what the Gradient tool needs to put a gradient on the characters of their stories. */
    text: frameIds.length ? { ids: frameIds, front: (stroke: boolean) => (stroke ? stroke_ : fill_), gradient: textGradient }
      : selection ? { ids: [], range: selection.start === selection.end ? null : { story: selection.story, start: selection.start, end: selection.end }, caret: selection.start === selection.end,
        front: (stroke: boolean) => (stroke ? stroke_ : fill_), gradient: textGradient } : null,
    swap: textSwap ?? swap,
    defaults: selection ? () => p.text.apply({ fillColor: BLACK, strokeColor: NONE }) : toFrames(tr('기본 칠과 획'), { fillColor: BLACK, strokeColor: NONE }) ?? defaults,
    applyNone: selection ? () => p.text.apply({ [p.strokeFocus ? 'strokeColor' : 'fillColor']: NONE }) : toFrames(tr('없음 적용'), { [p.strokeFocus ? 'strokeColor' : 'fillColor']: NONE }) ?? (can ? apply(tr('없음 적용'), NONE) : undefined),
    applyColor: selection ? () => p.text.apply({ [p.strokeFocus ? 'strokeColor' : 'fillColor']: lastSolid.current }) : toFrames(tr('색상 적용'), { [p.strokeFocus ? 'strokeColor' : 'fillColor']: lastSolid.current }) ?? (can ? apply(tr('색상 적용'), lastSolid.current) : undefined),
    // a gradient the toolbox has shown, else InDesign's default one (white to black) along the object's width
    applyGradient: selection ? () => applyTextGradient(selection) : frameIds.length ? () => {
      const key = p.strokeFocus ? 'strokeColor' : 'fillColor', g = textGradient();
      run(tr('그레이디언트 적용'), [...g.prefix, { op: 'formatFrameText', ids: frameIds, props: { [key]: g.swatch } }]);
    } : can ? () => run(tr('그레이디언트 적용'), gradient
      ? [{ op: 'setPaint', ids: targets.map((t) => t.id), ...(p.strokeFocus ? { stroke: gradient } : { fill: gradient }) }]
      : targets.flatMap((t): EditCommand[] => {
        const it = itemById.get(t.id);
        if (!it) return [];
        const [x0, y0, x1, y1] = it.inner;
        return [{ op: 'applyDefaultGradient', id: t.id, stroke: p.strokeFocus, start: [x0, (y0 + y1) / 2], angle: 0, length: x1 - x0 }];
      })) : undefined,
  };
}
