// Type ▸ Text Variables ▸ Define…: the document's text variables, listed; New… and Edit… open the variable's settings, Insert puts an
// instance at the text cursor. The engine writes the variable as InDesign does (edit/src/ops/textvariables.rs, oracle variables
// var-def-* and var-edit-*): a settings element of its kind in the designmap, the list kept sorted by name; a rename reaches the
// instances. Settings are the IDML attributes of that element (`TextBefore`, `SearchStrategy`, …), so the dialog edits them as the
// file has them. Delete and Convert to Text are not built.
import { t as tr } from '../i18n';
import { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';

type Variable = DocumentInfo['textVariables'][number];
type Edit = (label: string, commands: EditCommand[]) => unknown;
type Values = Record<string, string>;

/** The settings a kind of variable has, and whether the engine works out what an instance shows (else Insert stays unavailable). */
const KINDS: { kind: string; label: () => string; fields: string[]; computed: boolean }[] = [
  { kind: 'CustomTextType', label: () => tr('사용자 지정 텍스트'), fields: ['Contents'], computed: true },
  { kind: 'MatchParagraphStyleType', label: () => tr('머리글(단락 스타일)'), fields: ['TextBefore', 'AppliedParagraphStyle', 'SearchStrategy', 'ChangeCase', 'DeleteEndPunctuation', 'TextAfter'], computed: true },
  { kind: 'MatchCharacterStyleType', label: () => tr('머리글(문자 스타일)'), fields: ['TextBefore', 'AppliedCharacterStyle', 'SearchStrategy', 'ChangeCase', 'DeleteEndPunctuation', 'TextAfter'], computed: true },
  { kind: 'ChapterNumberType', label: () => tr('장 번호'), fields: ['TextBefore', 'TextAfter'], computed: true },
  { kind: 'LastPageNumberType', label: () => tr('마지막 페이지 번호'), fields: ['TextBefore', 'Scope', 'TextAfter'], computed: true },
  { kind: 'CreationDateType', label: () => tr('작성 날짜'), fields: ['TextBefore', 'Format', 'TextAfter'], computed: false },
  { kind: 'ModificationDateType', label: () => tr('수정 날짜'), fields: ['TextBefore', 'Format', 'TextAfter'], computed: false },
  { kind: 'OutputDateType', label: () => tr('출력 날짜'), fields: ['TextBefore', 'Format', 'TextAfter'], computed: false },
  { kind: 'FileNameType', label: () => tr('파일 이름'), fields: ['TextBefore', 'IncludePath', 'IncludeExtension', 'TextAfter'], computed: false },
];
const kindOf = (kind: string) => KINDS.find((k) => k.kind === kind);
/** Kinds the list names but the form does not edit yet. */
const kindName = (kind: string) => kindOf(kind)?.label() ?? (kind === 'LiveCaptionType' ? tr('이미지 이름') : kind);

/** What a new variable starts with (as InDesign makes one). */
const DEFAULTS: Values = {
  TextBefore: '', TextAfter: '', Contents: '', SearchStrategy: 'FirstOnPage', ChangeCase: 'None', DeleteEndPunctuation: 'false', Scope: 'SectionScope',
  Format: 'MM/dd/yy', IncludePath: 'false', IncludeExtension: 'false',
  AppliedParagraphStyle: 'ParagraphStyle/$ID/NormalParagraphStyle', AppliedCharacterStyle: 'CharacterStyle/$ID/[No character style]',
};

const CASES = () => [['None', tr('변경 안 함')], ['Uppercase', tr('대문자')], ['Lowercase', tr('소문자')], ['Titlecase', tr('제목 대소문자')], ['Sentencecase', tr('문장 대소문자')]] as const;
const SEARCHES = () => [['FirstOnPage', tr('페이지의 첫 번째')], ['LastOnPage', tr('페이지의 마지막')]] as const;
const SCOPES = () => [['SectionScope', tr('구역')], ['DocumentScope', tr('문서')]] as const;

function Modal({ title, close, children }: { title: string; close: () => void; children: React.ReactNode }) {
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={title} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <div className="card dialog style-options variables-dialog">
        <h2>{title}</h2>
        {children}
      </div>
    </div>,
    document.body,
  );
}

type StyleRow = DocumentInfo['paragraphStyles'][number];
const styleLabel = (s: StyleRow) => [...s.group, s.name].join(' / ');

/** One variable's settings: a new one of a kind, or the settings of an existing one (its kind stays). */
function VariableForm({ info, variable, others, done, back }: {
  info: DocumentInfo | null; variable: Variable | null; others: Variable[]; done: (name: string, kind: string, values: Values) => void; back: () => void;
}) {
  const [name, setName] = useState(variable?.name ?? '');
  const [kind, setKind] = useState(variable?.kind ?? 'CustomTextType');
  const spec = kindOf(kind)!;
  const [values, setValues] = useState<Values>(() => ({ ...DEFAULTS, ...(variable?.settings ?? {}) }));
  const set = (key: string, value: string) => setValues((v) => ({ ...v, [key]: value }));
  const paragraphs = info?.paragraphStyles ?? [], characters = info?.characterStyles ?? [];
  // a style the document does not have anymore stays as it is in the list
  const styleOptions = (rows: StyleRow[], value: string) => (rows.some((s) => s.id === value) ? rows : [...rows, { id: value, name: value, group: [] } as StyleRow]);
  const taken = others.some((o) => o.name === name);
  const nameOk = name.trim() !== '' && !taken && !name.startsWith('<?AID');
  const text = (key: string, label: string) => (
    <label key={key}>{label}<input aria-label={label} value={values[key] ?? ''} onChange={(e) => set(key, e.target.value)} /></label>
  );
  const select = (key: string, label: string, options: readonly (readonly [string, string])[]) => (
    <label key={key}>{label}<select aria-label={label} value={values[key]} onChange={(e) => set(key, e.target.value)}>
      {options.map(([v, l]) => <option key={v} value={v}>{l}</option>)}</select></label>
  );
  const check = (key: string, label: string) => (
    <label key={key} className="check"><input type="checkbox" checked={values[key] === 'true'} onChange={(e) => set(key, e.target.checked ? 'true' : 'false')} /> {label}</label>
  );
  const field = (key: string) => {
    switch (key) {
      case 'TextBefore': return text(key, tr('앞 텍스트:'));
      case 'TextAfter': return text(key, tr('뒤 텍스트:'));
      case 'Contents': return text(key, tr('텍스트:'));
      case 'Format': return text(key, tr('날짜 형식:'));
      case 'AppliedParagraphStyle': return select(key, tr('스타일:'), styleOptions(paragraphs, values[key]).map((s) => [s.id, styleLabel(s)] as const));
      case 'AppliedCharacterStyle': return select(key, tr('스타일:'), styleOptions(characters, values[key]).map((s) => [s.id, styleLabel(s)] as const));
      case 'SearchStrategy': return select(key, tr('사용:'), SEARCHES());
      case 'ChangeCase': return select(key, tr('대소문자 변경:'), CASES());
      case 'Scope': return select(key, tr('범위:'), SCOPES());
      case 'DeleteEndPunctuation': return check(key, tr('끝 문장 부호 삭제'));
      case 'IncludePath': return check(key, tr('전체 경로 포함'));
      default: return check(key, tr('확장명 포함'));
    }
  };
  const submit = (e: React.FormEvent) => { e.preventDefault(); if (nameOk) done(name, kind, values); };
  return (
    <form onSubmit={submit}>
      <fieldset><legend>{variable ? tr('텍스트 변수 편집') : tr('새 텍스트 변수')}</legend>
        <label>{tr('이름:')}<input aria-label={tr('이름')} value={name} onChange={(e) => setName(e.target.value)} autoFocus /></label>
        {taken && <p className="field-error" role="alert">{tr('같은 이름의 변수가 이미 있습니다.')}</p>}
        <label>{tr('유형:')}<select aria-label={tr('유형')} value={kind} disabled={!!variable} onChange={(e) => setKind(e.target.value)}>
          {KINDS.map((k) => <option key={k.kind} value={k.kind}>{k.label()}</option>)}</select></label>
        {spec.fields.map(field)}
      </fieldset>
      <div className="dialog-actions"><button type="button" onClick={back}>{tr('취소')}</button><button type="submit" disabled={!nameOk}>{tr('확인')}</button></div>
    </form>
  );
}

/** Type ▸ Text Variables ▸ Define…. `insert` puts an instance of the variable at the cursor (only when the text is being edited). */
export function VariablesDialog({ info, edit, insert, close }: { info: DocumentInfo | null; edit: Edit; insert: ((id: string) => void) | null; close: () => void }) {
  const variables = useMemo(() => info?.textVariables ?? [], [info]);
  const [picked, setPicked] = useState<string | null>(null);
  const [form, setForm] = useState<'new' | 'edit' | null>(null);
  const current = variables.find((v) => v.id === picked) ?? null;
  const spec = current ? kindOf(current.kind) : undefined;

  if (form) {
    const target = form === 'edit' ? current : null;
    const done = (name: string, kind: string, values: Values) => {
      const keys = kindOf(kind)!.fields;
      if (!target) {
        void edit(tr('텍스트 변수 정의'), [{ op: 'defineTextVariable', name, kind, options: Object.fromEntries(keys.map((k) => [k, values[k]])) }]);
      } else {
        const changed = Object.fromEntries(keys.filter((k) => values[k] !== (target.settings[k] ?? DEFAULTS[k])).map((k) => [k, values[k]]));
        if (name !== target.name || Object.keys(changed).length) {
          void edit(tr('텍스트 변수 편집'), [{ op: 'editTextVariable', id: target.id, ...(name !== target.name ? { rename: name } : {}), options: changed }]);
        }
      }
      setForm(null);
    };
    return (
      <Modal title={tr('텍스트 변수')} close={() => setForm(null)}>
        <VariableForm info={info} variable={target} others={variables.filter((v) => v.id !== target?.id)} done={done} back={() => setForm(null)} />
      </Modal>
    );
  }
  return (
    <Modal title={tr('텍스트 변수')} close={close}>
      <fieldset><legend>{tr('변수')}</legend>
        <ul className="variables-list" role="listbox" aria-label={tr('변수')}>
          {variables.map((v) => (
            <li key={v.id} role="option" aria-selected={v.id === picked} className={v.id === picked ? 'on' : ''} onClick={() => setPicked(v.id)} onDoubleClick={() => { setPicked(v.id); setForm('edit'); }}>
              <span>{v.name}</span><span className="kind">{kindName(v.kind)}</span>
            </li>
          ))}
        </ul>
      </fieldset>
      <div className="dialog-actions variables-actions">
        <button type="button" onClick={() => setForm('new')}>{tr('새로 만들기…')}</button>
        <button type="button" disabled={!current || !spec} onClick={() => setForm('edit')}>{tr('편집…')}</button>
        <button type="button" disabled={!current} title={tr('변수를 지웁니다. 쓰인 곳은 보이던 텍스트가 됩니다.')}
          onClick={() => { if (current) { void edit(tr('텍스트 변수 삭제'), [{ op: 'deleteTextVariable', id: current.id }]); setPicked(null); } }}>{tr('삭제')}</button>
        <button type="button" className="muted" disabled title={tr('텍스트로 변환')}>{tr('텍스트로 변환')}</button>
        <button type="button" disabled={!current || !spec?.computed || !insert} title={current && !spec?.computed ? tr('이 변수의 값은 아직 계산하지 않습니다') : undefined}
          onClick={() => { if (current && insert) { insert(current.id); close(); } }}>{tr('삽입')}</button>
        <button type="button" onClick={close}>{tr('완료')}</button>
      </div>
    </Modal>
  );
}
