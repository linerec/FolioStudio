// The parts the small settings dialogs share (Pencil, Proof Setup, Guides, the user dictionary): the dialog shell with its OK / Cancel
// buttons and error line, and the labelled fields. Styled by pages.css (.transform-dialog). Edit ▸ Preferences itself is a dialog of the
// shared frame (next/shared/dialogs/preferences.json, ui/dialog-frame.tsx).
import { useEffect, useRef } from 'react';
import { tr } from '../i18n';
import { UI_COLOR_NAMES } from './uicolors';
import { TextNumberField, formatInteger, formatNumeric, lengthSpec, parseInteger, parseNumeric } from './controls/lengthfield';

export const CUSTOM = '*custom';

/** A shown value that is the name of an interface colour, or a custom one (shown as "Custom"). */
export const colorValue = (name: string | null | undefined) => name ?? CUSTOM;

export function PrefsDialog({ title, className, busy, error, onClose, onSubmit, children }: {
  title: string; className: string; busy: boolean; error: string; onClose: () => void; onSubmit: () => void; children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => { ref.current?.querySelector<HTMLInputElement>('input')?.select(); }, []);
  return <div className="overlay page-dialog-scrim" onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape' && !busy) onClose(); }}>
    <div className={`modal page-dialog transform-dialog ${className}`} ref={ref} role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2>
      <form onSubmit={(e) => { e.preventDefault(); onSubmit(); }}>
        {children}
        {error && <p role="alert" className="error">{error}</p>}
        <div className="actions">
          <button type="button" onClick={onClose} disabled={busy}>{tr('취소')}</button>
          <button className="primary" type="submit" disabled={busy} data-field="ok">{busy ? tr('적용 중…') : tr('확인')}</button>
        </div>
      </form>
    </div>
  </div>;
}

export function TextField({ label, value, set, name, suffix }: { label: string; value: string; set: (v: string) => void; name: string; suffix?: string }) {
  return <label>{label}<span className="td-field"><input type="text" value={value} data-field={name} onChange={(e) => set(e.target.value)} />{suffix && <em>{suffix}</em>}</span></label>;
}

/**
 * A number box of a preferences dialog. The dialog keeps the entry as text (it reads it with its own checks and ranges when OK is
 * pressed); the box shows the number the text means, steps it (arrows, ▲▼, wheel) and gives text back when a value is chosen:
 * `{ unit }` a length in the document's unit (a typed value with its own unit is read as such), `'integer'` a whole number,
 * `'number'` a plain number (a decimal comma or a % allowed). A range of its own is for the entry only when `min`/`max` are given.
 */
export function NumField({ label, value, set, name, suffix, spec, min, max }: {
  label: string; value: string; set: (v: string) => void; name: string; suffix?: string; spec: { unit: string } | 'integer' | 'number'; min?: number; max?: number;
}) {
  const length = typeof spec === 'object' ? lengthSpec(spec.unit) : null;
  return <label>{label}<span className="td-field">
    <TextNumberField label={label} text={value} onText={set} inputData={{ 'data-field': name }} min={min} max={max} width={110}
      format={length ? length.format : spec === 'integer' ? formatInteger : formatNumeric} parse={length ? length.parse : spec === 'integer' ? parseInteger : parseNumeric}
      step={length ? length.step : 1} />
    {suffix && <em>{suffix}</em>}</span></label>;
}

export function PickField({ label, value, set, name, options }: { label: string; value: string; set: (v: string) => void; name: string; options: [string, string][] }) {
  return <label>{label}<span className="td-field">
    <select value={value} data-field={name} onChange={(e) => set(e.target.value)}>
      {options.map(([v, n]) => <option key={v} value={v}>{n}</option>)}
    </select>
  </span></label>;
}

export function ColorField({ label, value, set, name }: { label: string; value: string; set: (v: string) => void; name: string }) {
  const options = UI_COLOR_NAMES();
  return <PickField label={label} value={value} set={set} name={name} options={value === CUSTOM ? [[CUSTOM, tr('사용자 지정')], ...options] : options} />;
}

export function CheckField({ label, value, set, name }: { label: string; value: boolean; set: (v: boolean) => void; name: string }) {
  return <label className="page-check"><input type="checkbox" checked={value} data-field={name} onChange={(e) => set(e.target.checked)} />{label}</label>;
}
