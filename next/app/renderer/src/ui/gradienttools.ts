import { useEffect, useRef } from 'react';
import { t as tr } from '../i18n';
import { textSelection, type EditCommand } from '../engine';
import { apply, invert, type ItemInfo, type ItemStore } from '../inspect';
import { findItem, type Ghost, type SelEntry } from './overlay';
import type { ToolId } from './tools';
import type { ToolboxText } from './toolboxpaint';
import { round4 as round } from './util/round';

type Point = [number, number];
type Event = React.PointerEvent<HTMLDivElement>;
type Props = {
  tool: ToolId; editable: boolean; selection: SelEntry[]; store: ItemStore;
  view: React.RefObject<HTMLDivElement | null>; pxPerPt: (el: HTMLElement, rect: DOMRect) => number;
  ghost: (v: { pages: number[]; g: Ghost } | null) => void;
  edit: (label: string, commands: EditCommand[]) => unknown; note: (message: string) => void;
  strokeFocus: boolean;
  /** T on with text frames selected: the drag puts the gradient on the characters of their stories (not on the frames). */
  text?: ToolboxText | null;
};
/**
 * The Gradient tool dragged over text frames with T on: the characters of each story (once per story, from the first selected frame of it) get the
 * dragged line as their `gradientFill…` (or `gradientStroke…` with the stroke in front). InDesign 2025, measured with real input: the start is the
 * pressed point measured from the top left of the frame's text area (the frame less its insets), not limited to the frame; the length is the distance
 * to the release point; the angle is -atan2(dy, dx) in degrees; the frame's own fill stays. A fill that is no gradient gets one first (the one the
 * toolbox last showed, else the default one).
 */
/** The dragged line as the text's own gradient placement: the start measured from the top left of the frame's text area (the frame less its insets), the length, the angle. */
function placement(i: ItemInfo, from: Point, to: Point) {
  const inv = invert(i.transform)!;
  const a = apply(inv, ...from), b = apply(inv, ...to);
  const [top, left] = [i.text?.options?.insets?.[0] ?? 0, i.text?.options?.insets?.[1] ?? 0];
  const dx = b[0] - a[0], dy = b[1] - a[1];
  return { start: `${round(a[0] - (i.inner[0] + left))} ${round(a[1] - (i.inner[1] + top))}`, length: round(Math.hypot(dx, dy)), angle: round(-Math.atan2(dy, dx) * 180 / Math.PI) };
}
function textGradientProps(text: ToolboxText, stroke: boolean, at: ReturnType<typeof placement>): Record<string, string | number> {
  const g = text.gradient(), key = stroke ? 'gradientStroke' : 'gradientFill', color = stroke ? 'strokeColor' : 'fillColor';
  return { ...(text.front(stroke)?.kind === 'gradient' ? {} : { [color]: g.swatch }), [`${key}Start`]: at.start, [`${key}Length`]: at.length, [`${key}Angle`]: at.angle };
}
function textGradientCommands(text: ToolboxText, frames: ItemInfo[], from: Point, to: Point, stroke: boolean): EditCommand[] {
  const done = new Set<string>(), out: EditCommand[] = [];
  for (const i of frames) {
    const story = i.text!.story;
    if (done.has(story)) continue;
    done.add(story);
    out.push({ op: 'formatFrameText', ids: [i.id], props: textGradientProps(text, stroke, placement(i, from, to)) });
  }
  return text.front(stroke)?.kind === 'gradient' ? out : [...text.gradient().prefix, ...out];
}
/**
 * The Gradient tool dragged with characters selected (the text selection survives choosing the tool; InDesign 2025, real input, toolbox-c3): only the
 * characters of the range get the dragged line, measured from the text area of the frame that holds the range (the first rect of the range: the frame
 * where the drag is made does not matter, nor the line the range is on). A caret changes nothing.
 */
async function rangeGradientCommands(text: ToolboxText, store: ItemStore, from: Point, to: Point, stroke: boolean): Promise<EditCommand[]> {
  const r = text.range;
  if (!r) return [];
  const g = await textSelection(r.story, r.start, r.end), at = g.rects[0];
  const frame = at ? findItem(store.peek(at.page), at.frame)?.item : undefined;
  if (!frame || !invert(frame.transform)) return [];
  const props = textGradientProps(text, stroke, placement(frame, from, to));
  return [...(text.front(stroke)?.kind === 'gradient' ? [] : text.gradient().prefix), { op: 'textFormat', story: r.story, start: r.start, end: r.end, props }];
}

export function useGradientTools(p: Props) {
  const drag = useRef<{ page: number; from: Point; to: Point; items: ItemInfo[]; feather: boolean; stroke: boolean; moved: boolean; range?: boolean } | null>(null);
  const cancel = () => { drag.current = null; p.ghost(null); };
  useEffect(() => { cancel(); }, [p.tool, p.selection, p.editable, p.strokeFocus]);
  useEffect(() => {
    const escape = (e: KeyboardEvent) => { if (e.key === 'Escape' && drag.current) { cancel(); e.preventDefault(); e.stopImmediatePropagation(); } };
    window.addEventListener('keydown', escape, true);
    return () => window.removeEventListener('keydown', escape, true);
  }, []);
  const down = (e: Event, here: { index: number; x: number; y: number } | null) => {
    if (p.tool !== 'gradient' && p.tool !== 'gradientFeather') return false;
    if (!here || !p.editable) return true;
    // characters selected (the text selection stays when this tool is chosen): the drag goes to the range, a caret does nothing
    if (p.tool === 'gradient' && p.text && !p.text.ids.length) {
      if (p.text.caret) return true;
      e.preventDefault();
      drag.current = { page: here.index, from: [here.x, here.y], to: [here.x, here.y], items: [], feather: false, stroke: p.strokeFocus, moved: false, range: true };
      return true;
    }
    const items = p.selection.filter(s => s.page === here.index && !s.content)
      .map(s => findItem(p.store.peek(s.page), s.id)?.item)
      .filter((i): i is ItemInfo => !!i && !i.locked && i.visible && !!invert(i.transform));
    if (!items.length) { p.note(p.tool === 'gradient' ? tr('그레이디언트를 적용할 개체를 선택하세요.') : tr('페더를 적용할 개체를 선택하세요.')); return true; }
    e.preventDefault();
    drag.current = { page: here.index, from: [here.x, here.y], to: [here.x, here.y], items, feather: p.tool === 'gradientFeather', stroke: p.strokeFocus, moved: false };
    return true;
  };
  const move = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    const el = p.view.current?.querySelector<HTMLElement>(`.page[data-page="${d.page}"]`);
    if (!el) return true;
    const r = el.getBoundingClientRect(), k = p.pxPerPt(el, r);
    let x = (e.clientX - r.left) / k, y = (e.clientY - r.top) / k;
    const len = Math.hypot(x - d.from[0], y - d.from[1]);
    if (len * k < 3) return true;
    if (e.shiftKey) {
      const a = Math.round(Math.atan2(y - d.from[1], x - d.from[0]) / (Math.PI / 4)) * Math.PI / 4;
      x = d.from[0] + Math.cos(a) * len; y = d.from[1] + Math.sin(a) * len;
    }
    d.to = [x, y]; d.moved = true;
    p.ghost({ pages: [d.page], g: { kind: 'gradient', from: d.from, to: d.to } });
    return true;
  };
  const up = (e: Event) => {
    const d = drag.current;
    if (!d) return false;
    move(e); cancel();
    if (!d.moved) return true;
    if (d.range && p.text) {
      const { from, to, stroke } = d;
      void rangeGradientCommands(p.text, p.store, from, to, stroke).then((commands) => { if (commands.length) p.edit(tr('그레이디언트 방향'), commands); }).catch((e) => p.note(String(e?.message ?? e)));
      return true;
    }
    const text = !d.feather && p.text ? p.text : null;
    const frames = text ? d.items.filter(i => text.ids.includes(i.id) && !!i.text?.story) : [];
    const commands: EditCommand[] = text && frames.length ? textGradientCommands(text, frames, d.from, d.to, d.stroke) : d.items.map(i => {
      const inv = invert(i.transform)!;
      const start = apply(inv, ...d.from), end = apply(inv, ...d.to);
      const dx = end[0] - start[0], dy = end[1] - start[1];
      const angle = -Math.atan2(dy, dx) * 180 / Math.PI, length = Math.hypot(dx, dy);
      return d.feather
        ? { op: 'setEffect', ids: [i.id], level: 'object', effect: 'gradientFeather', on: true, values: { GradientStart: start, Angle: angle, Length: length } }
        // InDesign gives a fill (or stroke) that is no gradient its default one, along the dragged line
        : (d.stroke ? i.stroke : i.fill).kind === 'gradient'
          ? { op: 'setGradientGeometry', id: i.id, start, angle, length, stroke: d.stroke }
          : { op: 'applyDefaultGradient', id: i.id, start, angle, length, stroke: d.stroke };
    });
    p.edit(d.feather ? tr('그레이디언트 페더') : tr('그레이디언트 방향'), commands);
    return true;
  };
  return { down, move, up, cancel };
}
