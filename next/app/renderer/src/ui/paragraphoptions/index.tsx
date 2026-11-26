import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { Command } from '../commands';
import type { TextFormatting } from '../textformat';
import { fieldKey } from '../keys';
import { TextNumberField, formatNumeric, parseNumeric } from '../controls/lengthfield';
import { paragraphPages } from './fields';

type Context = { documentPath?: string; edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null> };
type Selection = NonNullable<TextFormatting['selection']>;

export function useParagraphOptions(control: TextFormatting, context: Context) {
  const [page, setPage] = useState('justification');
  const pages = paragraphPages();
  const current = pages.find(p => p.id === page)!;
  const [selection, setSelection] = useState<Selection | null>(null);
  useEffect(() => setSelection(null), [context.documentPath]);
  const handlers: Record<string, Partial<Command>> = Object.fromEntries(pages.map(p => [
    `id.panel.paragraph.${p.id}`, {
      run: control.selection ? () => { setPage(p.id); setSelection(control.selection); } : undefined },
  ]));
  return { handlers, dialog: selection && <ParagraphOptionsDialog key={page} page={current} selection={selection} context={context} close={() => setSelection(null)} /> };
}

function ParagraphOptionsDialog({ page, selection, context, close }: { page: ReturnType<typeof paragraphPages>[number]; selection: Selection; context: Context; close: () => void }) {
  const [values, setValues] = useState<Record<string, string>>({});
  const [error, setError] = useState(''), [busy, setBusy] = useState(false);
  const fields = page.fields;
  const submit = async () => {
    const props: Record<string, string | number | boolean> = {};
    for (const [key, raw] of Object.entries(values)) {
      const field = fields.find(f => f.key === key);
      if (field?.choices) { if (!raw) continue; props[key] = field.boolean ? raw === 'true' : field.numericChoice && Number.isFinite(Number(raw)) ? Number(raw) : raw; continue; }
      if (!raw.trim() || !Number.isFinite(Number(raw))) { setError(tr('수치를 확인하세요.')); return; }
      props[key] = Number(raw);
    }
    if (!Object.keys(props).length) { close(); return; }
    setBusy(true); setError('');
    try {
      const result = await context.edit(page.label, [{ op: 'textFormat', story: selection.story, start: selection.start, end: selection.end, props }]);
      if (result) close(); else setError(tr('입력 범위와 최소·원하는·최대 순서를 확인하세요.'));
    } catch { setError(tr('입력 범위와 최소·원하는·최대 순서를 확인하세요.')); }
    finally { setBusy(false); }
  };
  return createPortal(<div className="style-modal" role="dialog" aria-label={page.label} onPointerDown={e => e.stopPropagation()} onKeyDown={e => {
    e.stopPropagation(); if (fieldKey(e) === 'cancel' && !busy) close();
  }}><form className="card dialog style-options" onSubmit={e => { e.preventDefault(); if (!busy) void submit(); }}>
    <h2>{page.label}</h2>
    {fields.map((f, i) => <label key={f.key}>{f.label}{f.choices ? <select aria-label={f.label} disabled={busy} value={values[f.key] ?? String(selection.format[f.key] ?? '')} onChange={e => setValues(v => ({ ...v, [f.key]: e.target.value }))}><option value="">{tr('혼합')}</option>{f.choices.map(c => <option key={c.value} value={c.value}>{c.label}</option>)}</select> : <TextNumberField autoFocus={i === 0} label={f.label} disabled={busy}
      text={values[f.key] ?? String(selection.format[f.key] ?? '')} placeholder={selection.format[f.key] == null ? tr('혼합') : ''}
      onText={t => { setError(''); setValues(v => ({ ...v, [f.key]: t })); }} format={formatNumeric} parse={parseNumeric} />}</label>)}
    {page.note && <p className="differs-note" data-note={page.id}>{page.note}</p>}
    {error && <p role="alert">{error}</p>}
    <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy}>{tr('확인')}</button></div>
  </form></div>, document.body);
}
