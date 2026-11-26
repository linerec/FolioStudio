import { createPortal } from 'react-dom';
import { useState } from 'react';
import { tr } from '../../../i18n';
import { fieldKey } from '../../../ui/keys';
import type { Param, ParamType } from './api';
import { PARAM_TYPES, RELATIVE, defaultFor, isRelative, newParam, paramTypeLabel, validParamName } from './model';

const CUSTOM = '';
/** The value of one parameter, by its type: a text, a number, a list (a line each), or a date or month that is typed or relative to today ("this month"). */
export function ParamValue({ type, value, label, onChange }: { type: ParamType; value: unknown; label: string; onChange: (v: unknown) => void }) {
  if (type === 'date' || type === 'month') {
    const relative = isRelative(type, value);
    const options = RELATIVE[type];
    const known = options.some((o) => o.token === value);
    return <span className="db-value">
      <select aria-label={tr('{0}: 날짜 기준', { 0: label })} data-db-relative value={relative ? (known ? String(value) : 'offset') : CUSTOM}
        onChange={(e) => onChange(e.target.value === CUSTOM ? '' : e.target.value === 'offset' ? 'monthOffset:0' : e.target.value)}>
        <option value={CUSTOM}>{type === 'month' ? tr('직접 입력한 월') : tr('직접 입력한 날짜')}</option>
        {options.map((o) => <option key={o.token} value={o.token}>{o.label()}</option>)}
        {relative && !known && <option value="offset">{tr('이번 달 기준 {0}개월', { 0: String(value).replace('monthOffset:', '') })}</option>}
      </select>
      {!relative && <input aria-label={label} data-db-param-value type={type} value={typeof value === 'string' ? value : ''} onChange={(e) => onChange(e.target.value)} />}
    </span>;
  }
  if (type === 'list') {
    const lines = Array.isArray(value) ? value.map(String).join('\n') : typeof value === 'string' ? value : '';
    return <textarea aria-label={label} data-db-param-value rows={2} placeholder={tr('한 줄에 하나')} value={lines} onChange={(e) => onChange(e.target.value.split('\n'))} />;
  }
  return <input aria-label={label} data-db-param-value inputMode={type === 'number' ? 'decimal' : undefined} value={typeof value === 'string' || typeof value === 'number' ? String(value) : ''} onChange={(e) => onChange(e.target.value)} />;
}

/** The parameters of a query: a name, a type and the default (for a month or a date: "this month", "last month", "next month", "today" or a fixed one). */
export function ParamsEditor({ params, onChange, used }: { params: Param[]; onChange: (p: Param[]) => void; used: (name: string) => boolean }) {
  const set = (i: number, patch: Partial<Param>) => onChange(params.map((p, j) => j === i ? { ...p, ...patch } : p));
  return <div className="db-params" data-db-params>
    <p className="db-hint">{tr('매개변수는 조건에서 값 대신 고르는 이름입니다. 가져올 때마다 값을 물어보므로 "이번 달" 같은 기본값으로 매달 같은 조건을 다시 쓸 수 있습니다. 고급 SQL 에서는 :이름 으로 씁니다.')}</p>
    {params.map((p, i) => {
      const dup = params.some((o, j) => j !== i && o.name === p.name);
      const bad = !validParamName(p.name) || dup;
      return <div key={i} className="db-param" data-db-param={p.name}>
        <input aria-label={tr('매개변수 이름')} data-db-param-name value={p.name} aria-invalid={bad} onChange={(e) => set(i, { name: e.target.value })} />
        <select aria-label={tr('매개변수 형식')} data-db-param-type value={p.type} onChange={(e) => set(i, { type: e.target.value as ParamType, default: defaultFor(e.target.value as ParamType) })}>
          {PARAM_TYPES.map((t) => <option key={t} value={t}>{paramTypeLabel(t)}</option>)}
        </select>
        <ParamValue type={p.type} label={tr('{0}의 기본값', { 0: p.name })} value={p.default} onChange={(v) => set(i, { default: v })} />
        <button type="button" aria-label={tr('매개변수 삭제')} data-db-remove-param disabled={used(p.name)} title={used(p.name) ? tr('조건이나 SQL 에서 쓰고 있어 지울 수 없습니다.') : undefined} onClick={() => onChange(params.filter((_, j) => j !== i))}>×</button>
        {bad && <small role="alert" className="db-warn">{dup ? tr('같은 이름의 매개변수가 있습니다.') : tr('이름은 글자·숫자·밑줄만 쓸 수 있습니다.')}</small>}
      </div>;
    })}
    <button type="button" data-db-add-param onClick={() => onChange([...params, newParam(params)])}>{tr('매개변수 추가')}</button>
  </div>;
}

/** What is asked when the data is fetched: a value for every parameter (the last ones used are the starting point). */
export function AskParams({ params, initial, title, message, ok, busy, error, onOk, onCancel }: {
  params: Param[];
  /** What is being asked (default: the values of the parameters). */
  message?: string;
  initial: Record<string, unknown>;
  title: string;
  ok: string;
  busy?: boolean;
  error?: string;
  onOk: (values: Record<string, unknown>) => void;
  onCancel: () => void;
}) {
  const [values, setValues] = useState<Record<string, unknown>>(() => Object.fromEntries(params.map((p) => [p.name, initial[p.name] ?? p.default ?? defaultFor(p.type)])));
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={title} data-db-ask-params onPointerDown={(e) => e.stopPropagation()}
      onKeyDown={(e) => { e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) onCancel(); }}>
      <form className="card dialog db-dialog db-ask" onSubmit={(e) => { e.preventDefault(); if (!busy) onOk(values); }}>
        <h2>{title}</h2>
        <p className="db-explain" data-db-ask-message>{message ?? tr('이번에 가져올 값을 정하세요. 마지막으로 쓴 값이 처음에 들어 있습니다.')}</p>
        {params.length > 0 && <fieldset disabled={busy} className="db-ask-fields">
          {params.map((p) => <label key={p.name} data-db-ask={p.name}>
            <span>{p.name} <small>({paramTypeLabel(p.type)})</small></span>
            <ParamValue type={p.type} label={p.name} value={values[p.name]} onChange={(v) => setValues((o) => ({ ...o, [p.name]: v }))} />
          </label>)}
        </fieldset>}
        {error && <p role="alert" data-db-ask-error>{error}</p>}
        <div className="dialog-actions">
          <button type="button" disabled={busy} onClick={onCancel}>{tr('취소')}</button>
          <button type="submit" data-db-ask-ok disabled={busy}>{busy ? tr('가져오는 중…') : ok}</button>
        </div>
      </form>
    </div>,
    document.body,
  );
}
