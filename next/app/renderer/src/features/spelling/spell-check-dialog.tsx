import { useEffect, useRef, useState } from 'react';
import type { EditCommand, EditResult } from '../../engine';
import { tr } from '../../i18n';
import type { TextSelectionState } from '../../ui/textedit';
import './spelling.css';
import { spellCheck, spellSuggest, spellUserDictionary, type SpellIssue, type SpellOptions, type SpellScope, type UserDictionaries } from './spelling';
import { documentRing, nextDocumentWithIssues, type SpellHandoff } from './spelling-docs';

/** What the dialog shows in its label: one of the four findings, or the two states without one. */
type Phase = 'ready' | 'issue' | 'complete';
const KIND_LABEL: Record<SpellIssue['kind'], () => string> = {
  misspelled: () => tr('사전에 없는 단어:'),
  repeated: () => tr('반복된 단어:'),
  // InDesign 2026 lists a lower-case proper noun ("monday") as "Not in Dictionary", never as "Uncapitalized Word".
  uncapitalizedWord: () => tr('사전에 없는 단어:'),
  uncapitalizedSentence: () => tr('대문자로 시작하지 않는 문장:'),
};
const keyOf = (i: SpellIssue) => `${i.story}:${i.start}:${i.word}`;

/** Kept while the app runs, as InDesign keeps them: Ignore All also holds after the dialog is closed and opened again, the
 * Search list and the Case Sensitive box keep their last value (InDesign 2026, measured). */
const remembered = { ignored: [] as string[], scope: null as SpellScope | null, caseSensitive: true };

/** The words of `list` from the text cursor on, then (when the search wraps around) the ones before it. */
function fromCursor(list: SpellIssue[], at: { story: string; start: number } | null, wrap: boolean): SpellIssue[] {
  if (!at) return list;
  const first = list.findIndex(i => i.story === at.story && i.start >= at.start);
  if (first < 0) return wrap ? list : [];
  return wrap ? [...list.slice(first), ...list.slice(0, first)] : list.slice(first);
}

/** Backwards (InDesign 2025, real clicks): the words from the one at the cursor (itself included) towards the start, then, when the search wraps, the rest from the end. */
function fromCursorBackward(list: SpellIssue[], at: { story: string; start: number } | null, wrap: boolean): SpellIssue[] {
  if (!at) return [...list].reverse();
  const first = list.findIndex(i => i.story === at.story && i.start >= at.start);
  const last = first < 0 ? list.length - 1 : list[first].start === at.start ? first : first - 1;
  const before = list.slice(0, last + 1).reverse();
  return wrap ? [...before, ...list.slice(last + 1).reverse()] : before;
}

/** Edit ▸ Spelling ▸ Check Spelling…: the order of fields and buttons, the labels, and what each button does follow InDesign 2026's
 * dialog (artifacts/spell-c2-20261023/STATUS.ko.md). It opens already checking, from the text cursor; the words come from the engine
 * (query `spellCheck`) and are changed with the same editor commands as Find/Change. The dictionaries are Hunspell dictionaries bundled
 * with Folio, so which words are called wrong differs from InDesign's. */
export function SpellCheckDialog({ selection, revision, options: windowOptions, userDictionaries: windowDictionaries, handoff, saveUserDictionaries, edit, reveal, openUserDictionary, close }: {
  selection: TextSelectionState | null; revision: string; options: SpellOptions; userDictionaries: UserDictionaries;
  /** Set when the check came from another window (Search: All Documents): this window's document is the next one to check. */
  handoff?: SpellHandoff | null;
  saveUserDictionaries: (next: UserDictionaries) => Promise<void>;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
  reveal: (m: { story: string; start: number; end: number }) => void; openUserDictionary: () => void; close: () => void;
}) {
  // A check handed over by another window brings what it was told to skip and the settings in force there (this window's copy of the settings may be older).
  if (handoff) { remembered.ignored = handoff.ignored; remembered.caseSensitive = handoff.caseSensitive; remembered.scope = 'allDocuments'; }
  const options = handoff?.options ?? windowOptions, userDictionaries = handoff?.userDictionaries ?? windowDictionaries;
  const hasRange = !!selection && selection.start !== selection.end;
  const [scope, setScope] = useState<SpellScope>(remembered.scope && (remembered.scope !== 'selection' || hasRange) && (remembered.scope === 'document' || remembered.scope === 'allDocuments' || !!selection)
    ? remembered.scope : selection ? (hasRange ? 'selection' : 'story') : 'document');
  // The windows still to check after this one (Search: All Documents), in the order the documents were opened; the last of them is the one the check started in.
  const ring = useRef<number[]>(handoff?.rest ?? []);
  const [issues, setIssues] = useState<SpellIssue[]>([]), [index, setIndex] = useState(0), [phase, setPhase] = useState<Phase>('ready');
  // The direction radios (InDesign 2025): changing one goes back to "ready" and Start checks from the word on show (the document's own selection moves with the dialog there)
  const [direction, setDirection] = useState<'forward' | 'backward'>(handoff?.direction ?? 'forward');
  const [from, setFrom] = useState<{ story: string; start: number } | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([]), [changeTo, setChangeTo] = useState('');
  const [chosen, setChosen] = useState<string | null>(null);
  // InDesign 2025 answers Change All with the alert "Search is completed.  N  replacement(s) made." (the dialog goes on with the next word after OK)
  const [notice, setNotice] = useState('');
  const [error, setError] = useState(''), [busy, setBusy] = useState(false), [caseSensitive, setCaseSensitive] = useState(remembered.caseSensitive);
  const skipped = useRef(new Set<string>());
  // the dictionaries a handed-over check brought stay in force until this window saves its own change (its copy of the settings may be older)
  const handed = useRef(!!handoff);
  const users = useRef(userDictionaries);
  if (!handed.current) users.current = userDictionaries;
  const current = phase === 'issue' ? issues[index] as SpellIssue | undefined : undefined;

  // The window does not hold the document: it stays open while the text is edited or the cursor moves (InDesign's dialog is modeless). When the
  // person changes the selection or the document, the dialog goes back to "ready" at once and Start checks from where the cursor is now
  // (InDesign 2025, measured: an edit or a new selection while it was open made it "Ready to Check Spelling"; spell-c2, inputtype-c1).
  // What the dialog itself moves (showing a word, Change) is not the person's change.
  const own = useRef<{ story: string; start: number; end: number } | null>(null);
  const where = selection ? `${selection.story}:${selection.start}:${selection.end}` : '';
  const busyNow = useRef(false), lastOwn = useRef(0), seen = useRef({ where, revision });
  const signal = `${where}|${revision}`;
  useEffect(() => {
    const before = seen.current;
    seen.current = { where, revision };
    if (before.where === where && before.revision === revision) return;
    if (busyNow.current || Date.now() - lastOwn.current < 600) return;
    // the cursor the dialog put on the word itself is not the person's (it may arrive late on a slow machine); a changed document always is
    const mine = own.current;
    if (before.revision === revision && mine && selection && selection.story === mine.story && selection.start === mine.start && selection.end === mine.end) return;
    setIssues([]); setPhase('ready'); setSuggestions([]); setChangeTo(''); setChosen(null); setFrom(null); setNotice(''); own.current = null;
    // a new range selection is searched (InDesign: Search turns to Selection); a cursor ends a search of the selection
    setScope(s => selection && selection.start !== selection.end ? 'selection' : s === 'selection' ? (selection ? 'story' : 'document') : s);
  }, [signal]); // eslint-disable-line react-hooks/exhaustive-deps

  const goNextDocument = async (): Promise<boolean> => {
    if (scope !== 'allDocuments' || !ring.current.length) return false;
    const found = await nextDocumentWithIssues(ring.current, { options, userDictionaries: users.current, ignored: remembered.ignored }, window.folio.windowQuery);
    if (!found) return false;
    const next: SpellHandoff = { rest: found.after, direction, ignored: remembered.ignored, caseSensitive, options, userDictionaries: users.current };
    const r = await window.folio.spellHandoff(found.window, next);
    if (!r.ok) return false;
    close(); // the dialog goes on in that window
    return true;
  };
  const show = async (list: SpellIssue[], at: number) => {
    const next = list[at];
    setIssues(list); setIndex(at);
    if (!next) {
      // Search: All Documents: the next document that has a flagged word takes over (its window comes to the front); after the last one the check is complete
      if (await goNextDocument()) return;
      setPhase('complete'); setSuggestions([]); setChangeTo(''); return;
    }
    setPhase('issue');
    setFrom({ story: next.story, start: next.start });
    own.current = { story: next.story, start: next.start, end: next.end }; lastOwn.current = Date.now();
    reveal({ story: next.story, start: next.start, end: next.end });
    // Change To starts as the word itself and nothing in the list is chosen (InDesign 2026).
    setChangeTo(next.word); setSuggestions(next.suggestions); setChosen(null);
    if (next.kind === 'misspelled' || next.kind === 'uncapitalizedWord') {
      const found = (await spellSuggest(next.language, next.word).catch(() => ({ suggestions: [] as string[] }))).suggestions;
      // a lower-case proper noun lists its capitalized form first: `monday` → `Monday`, `noonday`
      setSuggestions(next.kind === 'uncapitalizedWord' ? [...next.suggestions, ...found.filter(s => !next.suggestions.includes(s))] : found);
    }
  };
  /** Check from the text cursor (`at`) and show the first word found. */
  const scan = async (at: { story: string; start: number } | null, withUsers: UserDictionaries = users.current) => {
    const sel = selection;
    if (scope === 'allDocuments') {
      // this document is asked as a whole: from the text cursor (or the word on show) to its end, then round to its start (InDesign 2025: the start document
      // wraps before the check goes on in the next one); a handed-over document has no cursor and is walked from its first word
      const r = await spellCheck({ scope: 'document', options, userDictionaries: withUsers, ignored: remembered.ignored });
      const list = r.issues.filter(i => !skipped.current.has(keyOf(i)));
      await show(direction === 'backward' ? fromCursorBackward(list, at, true) : fromCursor(list, at, true), 0);
      return;
    }
    const range = scope === 'selection' && sel ? { scope: 'selection' as const, story: sel.story, start: sel.start, end: sel.end }
      : scope === 'toEnd' && sel ? { scope: 'selection' as const, story: sel.story, start: at && at.story === sel.story ? at.start : sel.start }
      : scope === 'story' && sel ? { scope: 'story' as const, story: sel.story } : { scope: 'document' as const };
    const r = await spellCheck({ ...range, options, userDictionaries: withUsers, ignored: remembered.ignored });
    const list = r.issues.filter(i => !skipped.current.has(keyOf(i)));
    const wrap = scope === 'document' || scope === 'story';
    await show(direction === 'backward' ? fromCursorBackward(list, at, wrap) : fromCursor(list, at, wrap), 0);
  };
  const guard = async (work: () => Promise<void>) => {
    if (busy) return;
    setBusy(true); busyNow.current = true; setError(''); setNotice('');
    try { await work(); } catch (e) { setError(String(e)); } finally { busyNow.current = false; lastOwn.current = Date.now(); setBusy(false); }
  };
  const cursor = () => from ?? (selection ? { story: selection.story, start: Math.min(selection.start, selection.end) } : null);
  const changeDirection = (next: 'forward' | 'backward') => { setDirection(next); setIssues([]); setPhase('ready'); setSuggestions([]); setChangeTo(''); };
  const start = () => guard(async () => {
    skipped.current.clear();
    // one round of the open documents from this one (the windows in the order the documents were opened)
    if (scope === 'allDocuments') ring.current = documentRing(await window.folio.spellWindows(), direction);
    await scan(cursor());
  });
  // InDesign opens already checking; a check handed over by another window goes on at the first word of this document.
  useEffect(() => { void (handoff ? guard(async () => { skipped.current.clear(); await scan(null); }) : start()); }, []); // eslint-disable-line react-hooks/exhaustive-deps
  const skip = () => guard(async () => { if (current) { skipped.current.add(keyOf(current)); await show(issues, index + 1); } });
  const ignoreAll = () => guard(async () => {
    if (!current) return;
    remembered.ignored = [...remembered.ignored, current.word.toLowerCase()];
    await scan({ story: current.story, start: current.start });
  });
  // The text to put in place of an item: for `the the` the two words (and what lies between them), for the rest the word.
  const fix = (i: SpellIssue, text: string): EditCommand =>
    ({ op: 'findReplace', query: { text: i.word, scope: 'selection', story: i.story, start: i.start, end: i.end, caseSensitive: true }, text, props: {} } as EditCommand);
  // Change To equal to the word leaves Change and Change All off (InDesign 2026 greys them out).
  const canChange = !!current && changeTo !== '' && changeTo !== current.word;
  const change = (all: boolean) => guard(async () => {
    if (!current || !canChange) return;
    const targets = all ? issues.filter(i => i.word === current.word && i.kind === current.kind) : [current];
    // later positions first, so earlier ones do not move
    const ordered = [...targets].sort((a, b) => (a.story === b.story ? b.start - a.start : a.story < b.story ? 1 : -1));
    const r = await edit(all ? tr('맞춤법: 모두 바꾸기') : tr('맞춤법: 바꾸기'), ordered.map(i => fix(i, changeTo)));
    if (!r) throw new Error(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.'));
    setNotice(all ? tr('검색이 끝났습니다. {count}개를 바꿨습니다.', { count: targets.length }) : '');
    await scan({ story: current.story, start: current.start });
  });
  const addToDictionary = () => guard(async () => {
    if (!current) return;
    const r = await spellUserDictionary('add', current.language, [current.word], users.current, caseSensitive);
    users.current = r.userDictionaries;
    await saveUserDictionaries(r.userDictionaries);
    handed.current = false;
    await scan({ story: current.story, start: current.start }, r.userDictionaries);
  });
  const scopes: [SpellScope, string, boolean][] = [['allDocuments', tr('모든 문서'), true], ['document', tr('문서'), true], ['story', tr('스토리'), !!selection], ['toEnd', tr('스토리 끝까지'), !!selection], ['selection', tr('선택 영역'), hasRange]];
  const label = phase === 'issue' && current ? KIND_LABEL[current.kind]() : phase === 'complete' ? tr('맞춤법 검사 완료') : tr('맞춤법 검사 준비 완료');
  const language = current?.language ?? 'English: USA';
  return <div className="spell-float" data-modeless="true" onKeyDown={e => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
    <div className="modal page-dialog spell-dialog" role="dialog" aria-label={tr('맞춤법 검사')}>
      <h2>{tr('맞춤법 검사')}</h2>
      <div className="spell-columns">
        <div className="spell-fields">
          <label><span data-field="spellKind">{label}</span>
            <input type="text" readOnly data-field="spellWord" value={current?.word ?? ''} className="spell-word" /></label>
          <label>{tr('바꿀 내용')}<input type="text" data-field="spellChangeTo" value={changeTo} disabled={!current} onChange={e => setChangeTo(e.target.value)} /></label>
          <div className="spell-suggest-field"><span>{tr('추천 단어')}</span>
            <div role="listbox" aria-label={tr('추천 단어')} className="spell-suggestions" data-field="spellSuggestions">
              {suggestions.map(s => <div key={s} role="option" aria-selected={chosen === s && changeTo === s} tabIndex={0} data-suggestion={s}
                onClick={() => { setChosen(s); setChangeTo(s); }} onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); setChosen(s); setChangeTo(s); } }}>{s || tr('(삭제)')}</div>)}
            </div></div>
          <label className="spell-inline">{tr('추가할 사전')}<select data-field="spellAddTo"><option>{tr('사용자 사전')}</option></select></label>
          <label className="spell-inline spell-check"><input type="checkbox" data-field="spellCaseSensitive" checked={caseSensitive}
            onChange={e => { remembered.caseSensitive = e.target.checked; setCaseSensitive(e.target.checked); }} />{tr('대소문자 구분')}</label>
          <p className="spell-inline">{tr('언어')}<span data-field="spellLanguage">{language}</span></p>
          <label className="spell-inline">{tr('검색 범위')}<select data-field="spellScope" value={scope} disabled={busy} onChange={e => {
            const next = e.target.value as SpellScope; remembered.scope = next; setScope(next); setIssues([]); setPhase('ready'); setSuggestions([]); setChangeTo(''); }}>
            {scopes.map(([id, name, enabled]) => <option key={id} value={id} disabled={!enabled}>{name}</option>)}</select></label>
        </div>
        <div className="spell-actions">
          <button type="button" data-field="spellClose" onClick={close}>{tr('완료')}</button>
          <fieldset className="spell-direction" disabled={busy}><legend>{tr('검색 방향')}</legend>
            <label><input type="radio" name="spellDirection" data-field="spellForward" checked={direction === 'forward'} onChange={() => changeDirection('forward')} />{tr('앞으로')}</label>
            <label><input type="radio" name="spellDirection" data-field="spellBackward" checked={direction === 'backward'} onChange={() => changeDirection('backward')} />{tr('뒤로')}</label>
          </fieldset>
          <button type="button" className="primary" data-field="spellSkip" disabled={busy || (phase === 'issue' && !current)}
            onClick={() => void (phase === 'issue' ? skip() : start())}>{phase === 'issue' ? tr('건너뛰기') : tr('시작')}</button>
          <button type="button" data-field="spellChange" disabled={busy || !canChange} onClick={() => void change(false)}>{tr('바꾸기')}</button>
          <button type="button" data-field="spellSkipAll" disabled={busy || !current} onClick={() => void ignoreAll()}>{tr('모두 건너뛰기')}</button>
          <button type="button" data-field="spellChangeAll" disabled={busy || !canChange} onClick={() => void change(true)}>{tr('모두 바꾸기')}</button>
          <button type="button" data-field="spellAdd" disabled={busy || !current || current.kind === 'repeated' || current.kind === 'uncapitalizedSentence'} onClick={() => void addToDictionary()}>{tr('추가')}</button>
          <button type="button" data-field="spellDictionary" onClick={openUserDictionary}>{tr('사전…')}</button>
        </div>
      </div>
      {error && <p className="spell-status" role="alert" data-field="spellError">{error}</p>}
      {notice && <p className="spell-status" role="status" data-field="spellNotice">{notice}</p>}
      <p className="user-dictionary-note">{tr('Folio에 들어 있는 Hunspell 사전으로 검사합니다.')}</p>
    </div>
  </div>;
}
