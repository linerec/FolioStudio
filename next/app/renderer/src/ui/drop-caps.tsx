import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { getStyleInfo, textFormat, type EditCommand, type NestedLineStyle, type NestedStyle } from '../engine';
import type { PanelCtx } from './panels';
import { NestedFields } from './textstyles/nested';
import { NestedLineFields } from './textstyles/nested-lines';
import { fieldKey } from './keys';
import { TextNumberField, formatInteger } from './controls/lengthfield';

// Paragraph panel menu ▸ Drop Caps and Nested Styles…: the drop cap part (lines and characters). One engine
// command does the same job with values: `textFormat` with `dropCapLines` and `dropCapCharacters`
// (next/oracle/rules/dropcap.ko.md). The nested styles are those of the selection's paragraph style (command
// `setNestedStyles`, next/oracle/rules/nested-styles.ko.md); its first item, delimiter `Dropcap`, is the drop cap's
// character style (also `setDropCapStyle`). Align Left Edge and Scale for Descenders are the bits of `textFormat`'s
// `dropCapDetail` (1 and 2; next/oracle/rules/dropcap.ko.md). The nested line styles are the paragraph style's too (command
// `setLineStyles`, the same rules file); what is not built yet is marked on the nested styles part.
export const ALIGN_LEFT_EDGE = 1, SCALE_FOR_DESCENDERS = 2;
export const DROP_CAP_LIMITS = {lines: 25, characters: 150} as const;

/** A whole number within 0..max typed in a field, or null. */
export function wholeNumber(text: string, max: number): number | null {
  if (!/^\d{1,4}$/.test(text.trim())) return null;
  const n = Number(text);
  return n <= max ? n : null;
}

export function DropCapsDialog({ctx, close}: {ctx: PanelCtx; close: () => void}) {
  const [selection] = useState(ctx.textFormatting.selection);
  const [format, setFormat] = useState(selection?.format ?? {});
  const [ready, setReady] = useState(false);
  const initial = (key: string) => (typeof format[key] === 'number' ? String(format[key]) : '');
  const [lines, setLines] = useState(initial('dropCapLines'));
  // `dropCapDetail` is a number unless the selection mixes values: then the defaults show and only a touched box is written.
  const [detail, setDetail] = useState<number>(typeof format.dropCapDetail === 'number' ? format.dropCapDetail : ALIGN_LEFT_EDGE), [detailDirty, setDetailDirty] = useState(false);
  const [characters, setCharacters] = useState(initial('dropCapCharacters'));
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const styleId = ready && typeof format.paragraphStyle === 'string' ? format.paragraphStyle : '';
  const [nested, setNested] = useState<NestedStyle[]>([]), [nestedDirty, setNestedDirty] = useState(false), [styleLocked, setStyleLocked] = useState(true);
  const [lineStyles, setLineStyleItems] = useState<NestedLineStyle[]>([]), [linesDirty, setLinesDirty] = useState(false);
  useEffect(() => {
    if (!selection) return;
    let live = true;
    // A panel edit may still be refreshing when this dialog opens. Read the captured
    // range from the engine before making the dialog's independent draft editable.
    void textFormat(selection.story, selection.start, selection.end).then(({format:f}) => {
      if (!live) return;
      setFormat(f);
      setLines(typeof f.dropCapLines === 'number' ? String(f.dropCapLines) : '');
      setCharacters(typeof f.dropCapCharacters === 'number' ? String(f.dropCapCharacters) : '');
      setDetail(typeof f.dropCapDetail === 'number' ? f.dropCapDetail : ALIGN_LEFT_EDGE);
      setReady(true);
    }).catch(e => { if (live) setError(String(e)); });
    return () => { live = false; };
  }, [selection]);
  useEffect(() => {
    if (!styleId) return;
    let live = true;
    void getStyleInfo('paragraph', styleId).then(r => { if (live) { setNested(r.style.nestedStyles ?? []); setLineStyleItems(r.style.lineStyles ?? []); setStyleLocked(r.style.protected); } }).catch(() => {});
    return () => { live = false; };
  }, [styleId]);
  const title = tr('단락 시작 장식 문자 및 중첩 스타일');
  const submit = async () => {
    if (!selection || !ctx.editable || !ready) return;
    const props: Record<string, number> = {};
    if (detailDirty) props.dropCapDetail = detail;
    for (const [key, text, max] of [['dropCapLines', lines, DROP_CAP_LIMITS.lines], ['dropCapCharacters', characters, DROP_CAP_LIMITS.characters]] as const) {
      if (text.trim() === '') continue;
      const n = wholeNumber(text, max);
      if (n === null) { setError(tr('수치를 확인하세요.')); return; }
      props[key] = n;
    }
    const commands: EditCommand[] = [];
    if (Object.keys(props).length) commands.push({op: 'textFormat', story: selection.story, start: selection.start, end: selection.end, props});
    if (nestedDirty && styleId) commands.push({op: 'setNestedStyles', style: styleId, items: nested});
    if (linesDirty && styleId) commands.push({op: 'setLineStyles', style: styleId, items: lineStyles});
    if (!commands.length) { close(); return; }
    setBusy(true);
    try {
      const result = await ctx.edit(title, commands);
      if (result) close(); else setError(tr('서식을 저장하지 못했습니다. 입력값을 확인하세요.'));
    } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return createPortal(<div className="style-modal" role="dialog" aria-label={title} onPointerDown={e => e.stopPropagation()} onKeyDown={e => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) close(); }}>
    <form className="card dialog style-options type-style-options" onSubmit={e => { e.preventDefault(); if (!busy) void submit(); }}>
      <h2>{title}</h2>
      {ready && <div role="group" aria-label={tr('장식 문자')}>
        <label>{tr('장식 문자 줄 수')}<TextNumberField label={tr('장식 문자 줄 수')} placeholder={tr('혼합')} text={lines} onText={t => { setError(''); setLines(t); }} onClear={() => { setError(''); setLines(''); }}
          format={formatInteger} parse={t => wholeNumber(t, DROP_CAP_LIMITS.lines)} min={0} max={DROP_CAP_LIMITS.lines} width={80} /></label>
        <label>{tr('장식 문자 글자 수')}<TextNumberField label={tr('장식 문자 글자 수')} placeholder={tr('혼합')} text={characters} onText={t => { setError(''); setCharacters(t); }} onClear={() => { setError(''); setCharacters(''); }}
          format={formatInteger} parse={t => wholeNumber(t, DROP_CAP_LIMITS.characters)} min={0} max={DROP_CAP_LIMITS.characters} width={80} /></label>
        <label className="check"><input type="checkbox" aria-label={tr('왼쪽 가장자리 맞추기')} checked={(detail & ALIGN_LEFT_EDGE) !== 0} onChange={e => { setError(''); setDetailDirty(true); setDetail(d => e.target.checked ? d | ALIGN_LEFT_EDGE : d & ~ALIGN_LEFT_EDGE); }}/>{tr('왼쪽 가장자리 맞추기')}</label>
        <label className="check"><input type="checkbox" aria-label={tr('내림 글자 크기 맞춤')} checked={(detail & SCALE_FOR_DESCENDERS) !== 0} onChange={e => { setError(''); setDetailDirty(true); setDetail(d => e.target.checked ? d | SCALE_FOR_DESCENDERS : d & ~SCALE_FOR_DESCENDERS); }}/>{tr('내림 글자 크기 맞춤')}</label>
      </div>}
      <NestedFields items={nested} styles={ctx.info?.characterStyles ?? []} disabled={busy || styleLocked || !styleId || !ctx.editable} set={items => { setNested(items); setNestedDirty(true); setError(''); }}/>
      <NestedLineFields items={lineStyles} styles={ctx.info?.characterStyles ?? []} disabled={busy || styleLocked || !styleId || !ctx.editable} set={items => { setLineStyleItems(items); setLinesDirty(true); setError(''); }}/>
      {error && <p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy || !ctx.editable || !ready}>{tr('확인')}</button></div>
    </form>
  </div>, document.body);
}
