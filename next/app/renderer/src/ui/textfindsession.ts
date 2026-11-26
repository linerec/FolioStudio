import { useEffect, useRef, useState } from 'react';
import { findText, textStory, type EditCommand, type FindQuery, type FoundText } from '../engine';
import { tr } from '../i18n';

/** `change`: the Change To text (the Find/Change window's, or loaded with Ctrl+F2), used by the Replace keys (Ctrl+F3, Shift+F3). */
export type TextFindSession = { query: FindQuery; last: FoundText | null; change?: string };

/** The match after `last` in the order the engine gave them (story order, or from the end back when the search runs backward). */
export function nextTextMatch(matches: FoundText[], last: FoundText | null, backward = false): FoundText | undefined {
  if (!last) return matches[0];
  const index = matches.findIndex(m => m.story === last.story && m.start === last.start);
  if (index >= 0) return matches[(index + 1) % matches.length];
  return matches.find(m => m.story === last.story && (backward ? m.end <= last.start : m.start >= last.end)) ?? matches[0];
}

export function useTextFindSession(document: string | undefined, reveal: (m: FoundText) => void, report: (message: string) => void) {
  const [session, remember] = useState<TextFindSession | null>(null);
  const [busy, setBusy] = useState(false);
  const generation = useRef(0), running = useRef(false);
  useEffect(() => { generation.current++; remember(null); setBusy(false); running.current = false; }, [document]);
  const next = async (from: TextFindSession | null = session) => {
    if (!from || running.current) return;
    const version = generation.current;
    running.current = true; setBusy(true);
    try {
      const { matches } = await findText(from.query);
      if (version !== generation.current) return;
      const match = nextTextMatch(matches, from.last, !!from.query.backward);
      remember({ query: from.query, last: match ?? null, change: from.change });
      if (match) reveal(match); else report(tr('찾은 글이 없습니다.'));
    } catch (e) { if (version === generation.current) report(String(e)); }
    finally { if (version === generation.current) { running.current = false; setBusy(false); } }
  };
  /** The selected text into Find What (Ctrl+F1; Shift+F1 then finds the next one) or Change To (Ctrl+F2): InDesign's Load keys (tools-c1). */
  const load = async (sel: { story: string; start: number; end: number }, into: 'find' | 'change', findNext = false) => {
    const text = (await textStory(sel.story)).text.slice(sel.start, sel.end);
    if (!text) return;
    const base: TextFindSession = session ?? { query: { text: '', scope: 'document' }, last: null };
    const made: TextFindSession = into === 'find' ? { ...base, query: { ...base.query, text }, last: findNext ? { ...sel, text } : null } : { ...base, change: text };
    remember(made);
    report(into === 'find' ? tr('찾을 내용에 넣었습니다: {0}', { 0: text }) : tr('바꿀 내용에 넣었습니다: {0}', { 0: text }));
    if (findNext) await next(made);
  };
  /** The found text (still selected) becomes the Change To text (Ctrl+F3; Shift+F3 then finds the next one); one undo step. */
  const replace = async (selected: { story: string; start: number; end: number } | null, edit: (label: string, commands: EditCommand[]) => Promise<unknown>, findNext = false) => {
    const last = session?.last;
    if (!session || !last || session.change === undefined) { report(tr('먼저 찾기·바꾸기로 찾을 내용과 바꿀 내용을 정하세요.')); return; }
    if (!selected || selected.story !== last.story || selected.start !== last.start || selected.end !== last.end) { if (findNext) await next(); return; }
    const change = session.change;
    await edit(tr('바꾸기'), [{ op: 'replaceTextRange', story: last.story, start: last.start, end: last.end, text: change }]);
    const after: TextFindSession = { ...session, last: { story: last.story, start: last.start, end: last.start + Array.from(change).length, text: change } };
    remember(after);
    if (findNext) await next(after);
  };
  /** Ctrl+Alt+Enter: the search direction turns (forward / backward) for the next Find Next, also from the keys (tools-c2). */
  const toggleDirection = () => {
    const base: TextFindSession = session ?? { query: { text: '', scope: 'document' }, last: null };
    const backward = !base.query.backward;
    remember({ ...base, query: { ...base.query, backward } });
    report(backward ? tr('찾기 방향: 뒤로') : tr('찾기 방향: 앞으로'));
    return backward;
  };
  return { session, remember, next, load, replace, toggleDirection, ready: !!session && !busy };
}
