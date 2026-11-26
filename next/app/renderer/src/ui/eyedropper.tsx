import { useEffect, useRef, useState } from 'react';
import { t as tr } from '../i18n';
import { textFormat, textStory, type EditCommand, type TextProperties } from '../engine';
import type { DocumentInfo, ItemInfo } from '../inspect';
import { Live } from './store';
import { styleFields } from './textstyles/fields';
import type { ToolId } from './tools';

type Options = { fill: boolean; stroke: boolean; character: boolean; paragraph: boolean };
const options = new Live<Options>({ fill: true, stroke: true, character: true, paragraph: true });
const choices = (): [keyof Options, string][] => [['fill', tr('칠')], ['stroke', tr('획')], ['character', tr('문자 서식')], ['paragraph', tr('단락 서식')]];
type Sample = { item: ItemInfo; text: TextProperties };
type Props = { tool: ToolId; editable: boolean; document: string; info: DocumentInfo | null; edit: (label: string, commands: EditCommand[]) => unknown; note: (message: string) => void };

export function useEyedropper(p: Props) {
  const sample = useRef<Sample | null>(null), queue = useRef<Promise<void>>(Promise.resolve()), generation = useRef(0);
  useEffect(() => { sample.current = null; generation.current++; }, [p.document, p.tool, p.editable]);
  // Esc with attributes picked up empties the eyedropper and keeps the tool (InDesign); an empty one leaves Esc to the app (back to Selection)
  const tool = useRef(p.tool), note = useRef(p.note); tool.current = p.tool; note.current = p.note;
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.key !== 'Escape' || tool.current !== 'eyedropper' || !sample.current) return;
      sample.current = null; generation.current++;
      e.preventDefault(); e.stopImmediatePropagation();
      note.current(tr('스포이드를 비웠습니다. 집을 개체를 누르세요.'));
    };
    window.addEventListener('keydown', key, true);
    return () => window.removeEventListener('keydown', key, true);
  }, []);
  // Every click is one step of the pick-up/apply sequence: a click made while the engine is still being asked (the first one reads the story, a loaded PC is slow)
  // waits for the one before it instead of being dropped, so "click the source, click the target" always means that. A tool or document change cancels the waiting ones.
  const run = async (item: ItemInfo, resample: boolean, gen: number) => {
    if (gen !== generation.current) return;
    try {
      if (!sample.current || resample) {
        let text: TextProperties = {};
        if (item.text) {
          const story = await textStory(item.text.story);
          const { format } = await textFormat(item.text.story, 0, story.length);
          text = Object.fromEntries(Object.entries(format).filter((e): e is [string, string | number | boolean] => e[1] !== null));
        }
        if (gen !== generation.current) return;
        sample.current = { item: structuredClone(item), text };
        p.note(tr('속성을 집었습니다. 적용할 개체를 클릭하세요. Alt+클릭(또는 Shift+클릭)으로 다시 집고, Esc 로 비웁니다.'));
        return;
      }
      const src = sample.current, opt = options.get(), commands: EditCommand[] = [];
      const paint: Extract<EditCommand, { op: 'setPaint' }> = { op: 'setPaint', ids: [item.id] };
      const swatch = (paint: ItemInfo['fill']) => paint.kind === 'none' ? 'Swatch/None'
        : p.info?.swatches.find(s => s.id === paint.swatch || s.name === paint.swatch)?.id;
      const fill = swatch(src.item.fill), stroke = swatch(src.item.stroke);
      if (opt.fill && fill) { paint.fill = fill; if (src.item.fill.tint !== null) paint.fillTint = src.item.fill.tint; }
      if (opt.stroke && stroke) {
        paint.stroke = stroke; paint.strokeWeight = src.item.stroke.weight;
        if (src.item.stroke.tint !== null) paint.strokeTint = src.item.stroke.tint;
      }
      if (paint.fill || paint.stroke) commands.push(paint);
      if (opt.stroke) {
        const s = src.item.stroke;
        const gap = swatch(s.gap);
        commands.push({ op: 'setStroke', ids: [item.id], change: { cap: s.cap, join: s.join, miterLimit: s.miterLimit, alignment: `${s.alignment}Alignment`, kind: s.type, start: s.start, end: s.end, startScale: s.startScale, endScale: s.endScale, ...(gap ? { gapColor: gap } : {}), ...(s.gap.tint !== null ? { gapTint: s.gap.tint } : {}) } });
      }
      if (item.text && src.item.text && (opt.character || opt.paragraph)) {
        const fields = styleFields().filter(f => !f.unfinished && ((opt.character && f.page === 'character') || (opt.paragraph && f.page === 'paragraph'))).map(f => f.key);
        if (opt.paragraph) fields.push('justification');
        const props = Object.fromEntries(fields.filter(k => src.text[k] !== undefined).map(k => [k, src.text[k]]));
        const story = await textStory(item.text.story);
        if (Object.keys(props).length) commands.push({ op: 'textFormat', story: item.text.story, start: 0, end: story.length, props });
      }
      if (gen === generation.current && commands.length) await p.edit(tr('스포이트 적용'), commands);
    } catch (e) { if (gen === generation.current) p.note(String(e)); }
  };
  const pick = (item: ItemInfo, resample: boolean) => { const gen = generation.current; queue.current = queue.current.then(() => run(item, resample, gen)); };
  return { down: (e: React.PointerEvent, item: ItemInfo | undefined) => {
    if (p.tool !== 'eyedropper') return false;
    e.preventDefault();
    if (p.editable && item && !item.locked && item.visible) pick(item, e.shiftKey || e.altKey); // Alt+click picks up again (InDesign); Shift too, as before
    return true;
  } };
}

export function EyedropperDialog({ close }: { close: () => void }) {
  const [value, setValue] = useState(options.get());
  return <div className="overlay page-dialog-scrim" onKeyDown={e => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
    <div className="modal page-dialog eyedropper-dialog" role="dialog" aria-modal="true" aria-label={tr('스포이트 옵션')}>
      <h2>{tr('스포이트 옵션')}</h2>
      <form onSubmit={e => { e.preventDefault(); options.set(value); close(); }}>
        {choices().map(([key, label]) => <p key={key}><label><input type="checkbox" name={key} checked={value[key]} onChange={e => setValue({ ...value, [key]: e.target.checked })} />{label}</label></p>)}
        <p>{tr('텍스트는 연결된 글 전체에 적용합니다. 혼합 서식과 스타일 연결은 복사하지 않습니다.')}</p>
        <div className="actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" autoFocus>{tr('확인')}</button></div>
      </form>
    </div>
  </div>;
}
