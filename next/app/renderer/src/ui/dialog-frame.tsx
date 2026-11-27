import { useEffect, useRef, useState } from 'react';
import { AiSettings } from '../ai/Settings';
import { tr } from '../i18n';
import type { EditResult } from '../engine';
import { parseLength, unitOf } from '../inspect';
import { documentPath } from '../project';
import { NumberField } from './controls/numberfield';
import { formatInteger, formatNumeric, lengthSpec, parseInteger, parseNumeric } from './controls/lengthfield';
import { PREVIEW_PANELS, ROW_EDITORS, STATUS_PANELS } from './dialog-editors';
import { keyLabel, shortcutSpec } from './keys';

type Row = Record<string, number | boolean | string>;
type Value = number | boolean | string | Row[];
type Field = {
  name: string; label?: string; type: 'number' | 'boolean' | 'enum' | 'text' | 'choice' | 'array' | 'color' | 'json' | 'key' | 'writeOnly'; group?: string; min: number; max: number; integer?: boolean;
  unit?: 'length-h' | 'length-v' | 'length-pt' | 'angle' | 'percent'; suffix?: string; hidden?: boolean; radio?: [string, string]; rangeMessage?: string; required?: boolean; maxLength?: number; multiline?: boolean;
  /** text: a hint box shows (code, not translated), the box is monospaced and `rows` high */
  placeholder?: string; mono?: boolean; rows?: number; format?: string;
  /** choice: the value of the empty choice (`none`); text: a prefix of the stored text the box does not show (the IDML placeholder `$ID/`) */
  noneValue?: string; hidePrefix?: string;
  /** shown while (or off while) every field named holds its value, or one of the values listed */
  labelFacing?: string; labelWhen?: { label: string; [field: string]: unknown }; when?: Record<string, unknown>; disabledWhen?: Record<string, unknown>;
  /** enum: the options as a grid of that many buttons a row (a reference point) */
  grid?: number;
  /** enum: only these or the value the dialog opened with (the others are shown off; the engine refuses them) */
  fixedExcept?: string[];
  /** shown but not editable (the engine refuses it too); `disabledNote` says why */
  disabled?: boolean; disabledNote?: string;
  /** an option `native` is shown in its own words (a language's name), not translated */
  options?: (string | { value: string; label: string; native?: boolean; current?: boolean })[];
  /** the label's arguments (`{name}`), each a text of its own */
  labelArgs?: Record<string, string>;
  /** what the engine's refusal of the field means, by its reason (a quotation pair, a word already listed) */
  messages?: Record<string, string>;
  /** a setting of the document: greyed without one (scope app), or not shown then (`hideWithoutDocument`) */
  document?: boolean; hideWithoutDocument?: boolean;
  /** text: a path the engine checks (`folder`: a whole folder that exists; `name`); `choose` labels the button that fills it from a folder window */
  path?: 'folder' | 'name'; choose?: string; chooseTitle?: string;
  /** choice: the list the engine gives on opening (`state.choices[source]`), `none` the label of the empty choice, `custom` the label
   *  of `__custom`, which the engine sets while the values are none of the list's (New Document's [Custom]) */
  source?: string; none?: string; custom?: string;
  /** array: the sub-fields of a row and how many rows it takes */
  fields?: Field[]; minRows?: number; maxRows?: number; addLabel?: string; removeLabel?: string; newRow?: Row;
  /** array: a row editor of ui/dialog-editors.tsx instead of the table of rows */
  editor?: string;
  /** text: not a name the list `unique` names already has (but the dialog's own item's); `uniqueMessage` says so.
   *  array: no two rows alike in the sub-fields `unique` lists (the engine refuses it too, `duplicate`) */
  unique?: string | string[]; uniqueMessage?: string;
  tab?: string;
  /** shown above the fields of every tab (the preset a PDF dialog shows, its name) */
  everyTab?: boolean;
  /** choice: a list box this many rows high instead of a drop-down (Keyboard Shortcuts' commands) */
  size?: number;
  /** array: one row for each item the engine lists (the document's languages, its inks: no rows added, removed or moved, only their open
   *  cells set); a row's sub-field labelled by a cell (`labelFrom`) or with it (`labelArg`, the label's {0}) */
  fixedRows?: boolean; labelFrom?: string; labelArg?: string;
  /** text `"path": "saveFile"`: before a button that needs it (`needs`) the system's save window fills it, starting from this file name */
  saveAs?: string;
  /** text `"path": "openFile"`: before a button that needs it the system's open window of this kind fills it (a file of print presets) */
  openAs?: 'pdfPreset' | 'printPreset';
  /** choice: the empty choice (`none`) is listed only while the engine's list is empty (no printers) */
  noneWhenEmpty?: boolean;
  /** array: `remove`: a row has only its remove button and no row is added here (a list the engine keeps sorted: a user dictionary's words) */
  rowButtons?: 'remove';
};
type Definition = {
  title: string; /** the undo step's name when it is not the title */ label?: string; style?: 'transform' | 'wide' | 'preferences'; className?: string; fields: Field[];
  note?: string | (string | { text: string; when?: Record<string, unknown>; differs?: string })[]; pagesNote?: string;
  /** the dialog opens again (empty) after a button has applied: a manager of a list of items */
  /** with `keepOpen`: the fields that keep their values when it opens again, and what it says once a button has applied */
  keepValues?: string[]; appliedNote?: string;
  /** the names of the OK and Cancel buttons when they are not OK and Cancel (Convert, Done; "만들기" when the dialog makes something) */
  okLabel?: string; cancelLabel?: string;
  /** OK is off while these fields hold these values, and while the two fields of a pair are equal (the engine refuses both) */
  okDisabledWhen?: Record<string, unknown>; distinct?: [string, string][];
  /** rows a list must have: at least `min` that hold `where`, one of them holding `some` (the engine refuses it too, `tooFew`) */
  rowsCheck?: { field: string; where?: Record<string, unknown>; min?: number; some?: Record<string, unknown>; message?: string }[];
  /** (or only after the buttons listed: TOC Styles' Delete and Load, not OK) */
  keepOpen?: boolean | string[]; buttons?: { label: string; field: string; action?: string; set?: { field: string; value: Value }; disabledWhen?: Record<string, unknown> | Record<string, unknown>[];
    /** a button that closes the dialog and hands the screen something to do instead (New Book's "open an existing book") */
    screen?: string;
    /** off until the engine's preview has something (Print Booklet's Print waits for the plan's sheets, as OK does) */
    needsPreview?: boolean }[];
  /** the text fields a button needs (the engine refuses it without them); one the system's save window fills (`saveAs`) is asked for first */
  needs?: Record<string, string[]>;
  /** a panel of ui/dialog-editors.tsx under the fields that shows what a button that keeps the dialog open answered (a print job sent) */
  status?: string;
  /** the dialog whose field labels the status panel names settings by when this one has no such field (Print Booklet: File ▸ Print's) */
  labelsFrom?: string;
  /** a tab of `summary`: the values of the other tabs' fields, a table each (InDesign's Print Summary; what is off reads "N/A") */
  tabs?: { id: string; label: string; document?: boolean; note?: string; differs?: string; className?: string; summary?: boolean }[];
  /** lines a tab shows under its fields, each while `when` holds (the {names} are the dialog's values): a PDF preset's summary */
  tabNotes?: { tab: string; text: string; when?: Record<string, unknown> }[];
  /** a picture of the values the dialog holds now, drawn by the engine (`dialogPreview`); OK waits until there is one */
  preview?: { title: string; empty: string; pending: string; failed: string; alt: string; notices?: Record<string, string>;
    /** `list`: the engine answers rows (`swatches`: name and percentages) the dialog lists under `count` ("{0} swatches");
     *  `sheets`: the sheets of a print plan (`sheets`: the pages on each and its size), a place without a page named `blank`;
     *  `text`: a line the engine computes (`values` of its answer are the {names} of `text`, in the box `field`); OK does not wait for it;
     *  `panel`: the answer drawn by a panel of ui/dialog-editors.tsx (`panel`: File ▸ Package's plan); OK waits for it and is off while
     *  the answer says so (`okAllowed: false`: the folder cannot be made) */
    kind?: 'list' | 'sheets' | 'text' | 'panel'; count?: string; blank?: string; text?: string; field?: string; panel?: string };
  /** a dialog in stages (File ▸ Package: choose → running → done / failed): the field that holds the stage, the stage that is the form;
   *  in another stage only the status panel shows, with the close button (`closeLabel`) */
  stages?: { field: string; form: string; closeLabel?: string };
  /** `app`: opens without a document (held by the engine process, not a document session); `either`: the document's when one is open */
  scope?: 'app' | 'document' | 'either';
  /** the fields whose values are the units the dialog's lengths are shown in, instead of the ruler units: one field for both directions
   *  (New Document's `unit`) or one each (Preferences: the ruler units it is choosing). A field of the document's is not used without one (scope app). */
  unitFields?: string | { horizontal: string; vertical: string };
  /** the fields whose values make the path the dialog will make (a folder, then a name in it), shown under them */
  pathPreview?: string[];
  /** the groups in this many columns */
  columns?: number;
  /** the question the engine asks when an app request refuses with `code` (the file exists): Yes applies `action` */
  confirm?: { code: string; action: string; title: string; message: string; yes: string; no: string };
  /** what an app request's refusal means, by its code (`bad_name: a:b` → the words of bad_name with the rest as {0}) */
  errors?: Record<string, string>;
};
type Preview = { ok?: boolean; svg?: string; error?: string; notice?: string | null; font?: string; state?: string; swatches?: { name: string; percentages: number[] }[];
  sheets?: { face: number; size: [number, number]; cells: { name: string | null }[] }[]; values?: Record<string, string | number>;
  /** (a panel's answer: what it draws, and whether OK can go) */
  okAllowed?: boolean; [more: string]: unknown };
/** An SVG string that is safe to put into the page: the engine's answer, and nothing that runs. */
const safeSvg = (svg: string | undefined) => svg && svg.trimStart().startsWith('<svg') && !/<script|\son[a-z]+\s*=|javascript:/i.test(svg) ? svg : null;
type Choice = { value: string; label: string; [more: string]: unknown };
type State = {
  dialog: string; values: Record<string, Value>; initial?: Record<string, Value>; drafts: Record<string, string | null>; ids: string[]; scope?: 'app' | 'document';
  context: { facing?: boolean; pages?: number; baseName?: string; tab?: string; [arg: string]: unknown }; choices?: Record<string, Choice[]>;
  /** the question asked (`confirm` of the definition): the name of the file that is there */
  confirm?: { name: string };
  /** what the engine works out of the values now (`computed`): the fields that are off and why (a text of the screen's), and the
   *  choices a field cannot take; the engine refuses both */
  off?: Record<string, string>; offOptions?: Record<string, string[]>;
} | null;
type Units = { horizontal: string; vertical: string };

/** A value set at a field path (`name`, `name/row/sub`), as the engine sets it. */
function setAt(values: Record<string, Value>, path: string, value: Value): Record<string, Value> {
  const [name, row, sub] = path.split('/');
  if (row === undefined) return { ...values, [name]: value };
  const rows = [...(values[name] as Row[])], i = Number(row);
  rows[i] = sub === undefined ? value as unknown as Row : { ...rows[i], [sub]: value as number | boolean | string };
  return { ...values, [name]: rows };
}

/** One form renderer; fields, tabs, buttons and the edit commands belong to the shared registry (next/shared/dialogs/<id>.json).
 *  With `scope="app"` it shows the engine's app dialog (no document needed); `done` gets what the applied dialog's requests answered. */
export function DialogFrame({ path = '', units, applied, scope = 'document', done }: {
  path?: string; units: Units; applied?: (edit: EditResult) => void; scope?: 'app' | 'document'; done?: (dialog: string, result: unknown) => void | Promise<unknown>;
}) {
  const [state, setState] = useState<State>(null), [definitions, setDefinitions] = useState<Record<string, Definition>>({});
  const [error, setError] = useState(''), [busy, setBusy] = useState(false), [tab, setTab] = useState('');
  // the button whose requests are on their way (a status panel says a print is being sent)
  const [running, setRunning] = useState('');
  // what a dialog that stays open says once a button has applied (until a value changes or it closes)
  const [notice, setNotice] = useState<{ dialog: string; text: string } | null>(null);
  const callback = useRef(applied); callback.current = applied;
  const finished = useRef(done); finished.current = done;
  const pending = useRef(Promise.resolve());
  const aiFieldErrors = useRef(new Map<string,string>());
  const inflight = useRef(0);
  const known = useRef(definitions); known.current = definitions;
  const current = useRef<State>(null); current.current = state;
  const box = useRef<HTMLDivElement>(null);
  // the first box is ready to type in, as the dialogs of the screen always were; a dialog opens on its first tab
  useEffect(() => {
    aiFieldErrors.current.clear();
    setTab(current.current?.context.tab ?? '');
    const first = box.current?.querySelector<HTMLInputElement>('input:not([type=checkbox]):not([type=radio])');
    first?.focus(); first?.select();
  }, [state?.dialog]);
  useEffect(() => {
    let gone = false;
    setState(null);
    const off = window.folio.onEvent(event => {
      // the app dialog's events say so (`scope`), a document dialog's name the document
      const ours = scope === 'app' ? (event as { scope?: string }).scope === 'app' : (event as { scope?: string }).scope !== 'app' && event.path === path;
      if (event.event !== 'dialog-state' || !ours) return;
      const detail = event.detail as { dialogState: State; edit: EditResult | null; dialog?: string; result?: unknown; progress?: boolean };
      // (a long request's progress, kept in the dialog's values by the engine: shown while that request is still out)
      if (detail.progress) { setState(detail.dialogState); return; }
      // an echo of what this screen just sent is replaced by the engine's answer when the last request is through;
      // a closing (applied or cancelled) always shows at once
      if (detail.edit) callback.current?.(detail.edit);
      // what an applied dialog made is taken in first (the settings read again), then it closes
      const taking = detail.result != null && detail.dialog ? finished.current?.(detail.dialog, detail.result) : undefined;
      const show = () => { if (!inflight.current || detail.dialogState === null) { setState(detail.dialogState); setError(''); } };
      if (taking instanceof Promise) void taking.catch(() => undefined).finally(show); else show();
    });
    void window.folio.request({ cmd: 'dialogState', scope }).then(({ header }) => {
      if (!gone && header.ok) { setDefinitions(header.dialogs as Record<string, Definition>); setState(header.dialogState as State); }
    });
    return () => { gone = true; off(); };
  }, [path, scope]);
  const explain = (header: Record<string, unknown>) => {
    const dialog = current.current && known.current[current.current.dialog];
    const [name, , sub] = String(header.field ?? '').split('/');
    const top = dialog?.fields.find(x => x.name === name), f = sub ? top?.fields?.find(x => x.name === sub) : top;
    if (f && header.reason === 'outOfRange' && f.rangeMessage) return tr(f.rangeMessage);
    const said = f?.messages?.[String(header.reason)] ?? top?.messages?.[String(header.reason)];
    if (said) return tr(said);
    if (f) return tr('올바른 값을 넣으세요: {name}', { name: tr(f.label ?? f.name) });
    const meant = header.errorKind === 'appCommand' && dialog?.errors?.[String(header.error).split(': ')[0]];
    if (meant) return tr(meant, { 0: String(header.error).split(': ').slice(1).join(': ') });
    // a document request a button sent was refused (a print): what its kind means, then the engine's own words
    const kind = header.errorKind === 'dialogRequest' && dialog?.errors?.[String(header.code ?? '')];
    if (kind) return tr(kind, { 0: String(header.error ?? '') });
    return String(header.error);
  };
  const send = (cmd: string, args: Record<string, unknown> = {}) => {
    const field = current.current?.dialog==='aiSettings' && cmd==='setDialogField' ? String(args.field) : null;
    inflight.current++;
    const task = pending.current.then(async () => {
      try {
        const { header } = await window.folio.request({ cmd, scope, ...args });
        if (!header.ok) throw new Error(explain(header));
        if(field) aiFieldErrors.current.delete(field);
        return header;
      } finally { inflight.current--; }
    // (an applied dialog with a result is closed by its event, once the result is taken in; one a button keeps open (Save Preset) shows
    // the values its answer has)
    }).then(header => { if (!inflight.current && header.dialogState !== undefined && (header.result === undefined || header.dialogState !== null)) { setState(header.dialogState as State); setError(''); } });
    pending.current = task.catch(e => {
      const message=String(e.message ?? e);
      if(field) aiFieldErrors.current.set(field,message);
      setError(message);
    });
    return pending.current;
  };
  // a preview follows the values a moment after the last change; an answer that is no longer the newest question is dropped
  const [preview, setPreview] = useState<Preview | null>(null), [previewing, setPreviewing] = useState(false);
  const asked = useRef(0);
  // (a dialog in stages asks only while it shows its form: File ▸ Package's plan is not asked for again while it runs)
  const stagesOf = state ? definitions[state.dialog]?.stages : undefined;
  const wantsPreview = !!(state && definitions[state.dialog]?.preview) && (!stagesOf || state?.values[stagesOf.field] === stagesOf.form);
  const previewKey = wantsPreview ? JSON.stringify(state?.values) : '';
  useEffect(() => {
    if (!wantsPreview) { setPreview(null); setPreviewing(false); return; }
    const mine = ++asked.current;
    setPreviewing(true);
    const timer = window.setTimeout(() => {
      void pending.current.then(() => window.folio.request({ cmd: 'dialogPreview', scope })).then(({ header }) => {
        if (asked.current !== mine) return;
        setPreview(header.ok ? header.preview as Preview : { ok: false, error: String(header.error ?? '') });
        setPreviewing(false);
      }, () => { if (asked.current === mine) { setPreview({ ok: false }); setPreviewing(false); } });
    }, 200);
    return () => window.clearTimeout(timer);
  }, [wantsPreview, previewKey, state?.dialog]);
  if (!state || !definitions[state.dialog]) return null;
  const def = definitions[state.dialog];
  const change = (field: string, value: Value) => {
    setNotice(null);
    setState(s => s && { ...s, values: setAt(s.values, field, value), drafts: { ...s.drafts, [field]: null } });
    void send('setDialogField', { field, value });
  };
  const draft = (field: string, text: string | null) => {
    setState(s => s && { ...s, drafts: { ...s.drafts, [field]: text } });
    void send('setDialogField', { field, draft: text });
  };
  // rows are added, removed and moved by the engine; the screen shows its answer
  const rowOp = (f: Field, row: 'add' | 'remove' | 'move', at?: number, to?: number) =>
    void send('setDialogField', { field: at === undefined ? f.name : `${f.name}/${at}`, row, ...(to === undefined ? {} : { to }), ...(row === 'add' && f.newRow ? { value: f.newRow } : {}) });
  const close = () => { setNotice(null); void send('cancelDialog'); };
  const run = async (action: string) => {
    if (busy) return;
    // a file the button writes is chosen in the system's save window first (the engine checks the path it is given); no file, nothing done
    for (const f of def.fields.filter(x => (x.saveAs || x.openAs) && def.needs?.[action]?.includes(x.name))) {
      // (the name it starts from may be the document's: `{fileName}`; a file to read comes from the system's open window)
      const path = f.openAs ? await window.folio.openDialog(f.openAs) : await window.folio.saveDialog(f.saveAs!.replace('{fileName}', String(state.context.fileName ?? 'document')));
      if (!path) return;
      await send('setDialogField', { field: f.name, value: path });
    }
    setBusy(true); setRunning(action);
    await send('applyDialog', { action, label: tr(def.label ?? def.title) });
    setRunning('');
    // a manager of a list opens again, empty, once what the button did is in the document (a refused button leaves the dialog as it was)
    if (def.keepOpen === true || (Array.isArray(def.keepOpen) && def.keepOpen.includes(action))) {
      const { header } = await window.folio.request({ cmd: 'dialogState', scope });
      if (header.ok && header.dialogState === null) {
        const kept = Object.fromEntries((def.keepValues ?? []).map(k => [k, state.values[k]]));
        await send('openDialog', { dialog: state.dialog, ...(state.ids.length ? { ids: state.ids } : {}), ...(def.keepValues ? { values: kept } : {}) });
        if (def.appliedNote) setNotice({ dialog: state.dialog, text: def.appliedNote });
      }
    }
    setBusy(false);
  };
  if (state.dialog === 'aiSettings') return <AiSettings values={state.values} change={change} flush={async()=>{
    await pending.current;
    const message=aiFieldErrors.current.values().next().value;
    if(message)throw Error(message);
  }} close={close} error={error}/>;
  const transform = def.style === 'transform', prefs = def.style === 'preferences';
  // without a document (an app scope) a document's tab is greyed, its fields greyed or not shown
  const alone = state.scope === 'app';
  const activeTab = tab || def.tabs?.[0]?.id || '';
  const holds = (k: string, v: unknown) => Array.isArray(v) ? v.includes(state.values[k]) : state.values[k] === v;
  const matches = (when: Record<string, unknown> | undefined) => Object.entries(when ?? {}).every(([k, v]) => holds(k, v));
  const shown = (f: Field) => !f.hidden && !(alone && f.hideWithoutDocument) && matches(f.when) && (!def.tabs || !!f.everyTab || f.tab === activeTab);
  const labelOf = (f: Field) => {
    if (f.labelWhen) { const { label, ...when } = f.labelWhen; if (matches(when)) return tr(label); }
    const args = f.labelArgs && Object.fromEntries(Object.entries(f.labelArgs).map(([k, v]) => [k, tr(v)]));
    return tr(state.context.facing && f.labelFacing ? f.labelFacing : (f.label ?? f.name), args);
  };
  // lengths are kept in points and shown in the ruler units, or in the units the dialog holds (`unitFields`: New Document's unit, the
  // ruler units Preferences > Units & Increments is choosing); a unit field of the document's is not used without one
  const unitFields = typeof def.unitFields === 'string' ? { horizontal: def.unitFields, vertical: def.unitFields } : def.unitFields;
  const unitIn = (name: string, ruler: string) => alone && def.fields.find(f => f.name === name)?.document ? ruler : String(state.values[name] ?? ruler);
  const u = unitFields ? { horizontal: unitIn(unitFields.horizontal, units.horizontal), vertical: unitIn(unitFields.vertical, units.vertical) } : units;
  const unitName = (f: Field) => f.unit === 'length-h' ? u.horizontal : f.unit === 'length-v' ? u.vertical : f.unit === 'length-pt' ? 'Points' : null;
  const suffixOf = (f: Field) => f.suffix ?? (unitName(f) ? unitOf(unitName(f)!).suffix : f.unit === 'angle' ? '°' : f.unit === 'percent' ? '%' : '');
  /** The box of a field at a path: a top field (with the typed text kept in the engine) or a row's sub-field. */
  const numeric = (f: Field, at: string, value: Value, disabled: boolean, label = labelOf(f)) => {
    const length = unitName(f) ? unitOf(unitName(f)!) : null;
    // Preferences: a length shows and takes its unit in the box, as the dialog always did ("2 pt", "0p6", "3 mm"); the box is the field
    if (prefs && length) {
      const spec = lengthSpec(unitName(f)!);
      return <NumberField value={Number(value)} onCommit={v => change(at, v)} disabled={disabled} label={label} inputData={{ 'data-field': at }}
        min={f.min} max={f.max} step={spec.step} parse={spec.parse} format={spec.format} decimals={3} width={190} inDialog />;
    }
    const k = length ? length.perPoint : 1, digits = length ? Math.max(length.decimals, 3) : 4;
    // a length is kept in points and shown in the ruler unit, as a number with the unit written beside the box
    // a length may be typed with its own unit ("18 pt", "1p6", "5mm"), as the screen's length boxes take it
    // (a bare number is in the unit the box shows: the ruler's, or the dialog's own `unitFields`)
    const shownIn = unitName(f) ?? units.horizontal;
    const common = f.integer ? { parse: parseInteger, format: formatInteger, decimals: 0 }
      : length ? { parse: (text: string) => { const pt = parseLength(text, shownIn); return pt === null ? null : pt * k; }, format: (n: number) => String(Number(n.toFixed(digits))), decimals: digits }
        : f.unit ? { parse: parseNumeric, format: formatNumeric, decimals: 4 } : { decimals: 4 };
    // the text a box shows when it takes focus is the value, not an entry: sending it would put an older value over one the engine set since
    const typed = (text: string | null) => {
      const now = current.current;
      if (text !== null && now?.drafts[f.name] == null && Number.parseFloat(text) === Number((Number(now?.values[f.name]) * k).toFixed(digits))) return;
      draft(f.name, text);
    };
    const drafts = at === f.name ? { textValue: state.drafts[f.name] ?? null, onTextChange: typed } : {};
    const mark = prefs ? { inputData: { 'data-field': at } as Record<`data-${string}`, string> } : { fieldId: at };
    return <NumberField value={Number(value) * k} onCommit={v => change(at, f.integer ? Math.round(v) : v / k)} {...drafts} disabled={disabled}
      label={label} {...mark} min={f.min * k} max={f.max * k} step={1} width={transform ? 120 : prefs ? 190 : at === f.name ? 180 : '100%'} inDialog {...common} />;
  };
  const control = (f: Field, at: string, value: Value, disabled: boolean, aria = labelOf(f)) => {
    if (f.type === 'boolean') return <input type="checkbox" data-field={at} aria-label={aria} disabled={disabled} checked={Boolean(value)} onChange={e => change(at, e.target.checked)} />;
    if (f.type === 'enum' && f.grid) return <div role="radiogroup" className="dialog-grid" data-field={at} aria-label={aria} style={{ gridTemplateColumns: `repeat(${f.grid},14px)` }}>
      {(f.options ?? []).map(o => { const v = typeof o === 'string' ? o : o.value, label = typeof o === 'string' ? o : tr(o.label);
        return <button type="button" role="radio" key={v} data-value={v} aria-checked={value === v} aria-label={label} title={label} disabled={disabled}
          className={value === v ? 'on' : undefined} onClick={() => change(at, v)} />; })}</div>;
    // (an option `current` is listed only while it is the value; one `fixedExcept` does not take is greyed)
    if (f.type === 'enum') return <select data-field={at} aria-label={aria} disabled={disabled} value={String(value)} onChange={e => change(at, e.target.value)}>
      {(f.options ?? []).filter(o => typeof o === 'string' || !o.current || o.value === value).map(o => { const v = typeof o === 'string' ? o : o.value,
        off = (!!f.fixedExcept && !f.fixedExcept.includes(v) && state.initial?.[f.name] !== v) || (state.offOptions?.[f.name] ?? []).includes(v);
        return <option key={v} value={v} disabled={off}>{typeof o === 'string' ? o : o.native ? o.label : tr(o.label)}</option>; })}</select>;
    // a choice lists the document's own items (their names are not translated); an `only` item is offered only while it is the value ([Custom])
    if (f.type === 'choice') return <select data-field={at} aria-label={aria} disabled={disabled} size={f.size} value={String(value)} onChange={e => change(at, e.target.value)}>
      {f.none !== undefined && (!f.noneWhenEmpty || !(state.choices?.[f.source ?? ''] ?? []).length) && <option value={f.noneValue ?? ''}>{tr(f.none)}</option>}
      {/* (a screen text of the engine's list may name the item: "{0} (default)") */}
      {(state.choices?.[f.source ?? ''] ?? []).filter(o => !o.only || o.value === value).map(o => <option key={o.value} value={o.value}>{o.translate ? tr(o.label, o.args as Record<string, string> | undefined) : o.label}</option>)}
      {f.custom !== undefined && value === '__custom' && <option value="__custom">{tr(f.custom)}</option>}</select>;
    const shown = (text: string) => f.hidePrefix && text.startsWith(f.hidePrefix) ? text.slice(f.hidePrefix.length) : text;
    // a key pressed (Keyboard Shortcuts): the engine takes it normalized and works out its conflict; the box shows what the engine holds
    // (no early echo, so a conflict and the key show together)
    if (f.type === 'key') return <input type="text" readOnly data-field={at} aria-label={aria} disabled={disabled} value={keyLabel(String(value || ''))}
      onKeyDown={e => { e.preventDefault(); e.stopPropagation(); setNotice(null); void send('setDialogField', { field: at, value: shortcutSpec(e) ?? '' }); }} />;
    if (f.type === 'color') return <input type="color" data-field={at} aria-label={aria} disabled={disabled} value={String(value)} onChange={e => change(at, e.target.value)} />;
    if (f.type === 'text' && f.multiline) return <textarea data-field={at} aria-label={aria} disabled={disabled} rows={f.rows ?? 5} maxLength={f.maxLength} spellCheck={f.mono ? false : undefined}
      placeholder={f.placeholder} style={f.mono ? { width: '100%', fontFamily: 'monospace', boxSizing: 'border-box' } : undefined} value={shown(String(value))} onChange={e => change(at, e.target.value)} />;
    // a path the system's folder window fills (the engine checks what it is given, from there or typed)
    if (f.type === 'text' && f.choose) return <span className="pkg-dest">
      <input type="text" data-field={at} aria-label={aria} disabled={disabled} maxLength={f.maxLength} value={String(value)} onChange={e => change(at, e.target.value)} />
      <button type="button" data-field={`${at}/choose`} disabled={disabled || busy} onClick={() => void window.folio.chooseFolder(f.chooseTitle ? tr(f.chooseTitle) : undefined, String(value) || undefined)
        .then(folder => { if (folder) change(at, folder); })}>{tr(f.choose)}</button></span>;
    if (f.type === 'text') return <input type="text" data-field={at} aria-label={aria} disabled={disabled} maxLength={f.maxLength} placeholder={f.placeholder} value={shown(String(value))} onChange={e => change(at, e.target.value)} />;
    return numeric(f, at, value, disabled, aria);
  };
  const rows = (f: Field) => {
    const list = (state.values[f.name] as Row[]) ?? [], subs = (f.fields ?? []).filter(sub => !sub.hidden), gone = alone && !!f.document;
    // a fixed list (the document's languages, its inks) has no add, remove and move buttons
    const fixed = !!f.fixedRows, removeOnly = f.rowButtons === 'remove';
    return <fieldset key={f.name} data-field={f.name} disabled={gone}><legend>{labelOf(f)}</legend>
      {list.map((r, i) => {
        const holdsRow = (when: Record<string, unknown> | undefined) => Object.entries(when ?? {}).every(([k, v]) => Array.isArray(v) ? v.includes(r[k]) : r[k] === v);
        const here = subs.filter(sub => holdsRow(sub.when));
        // (a row's own cell names it: the ink of a mixed ink row, which is the document's name and not translated)
        const label = (sub: Field) => sub.labelFrom ? String(r[sub.labelFrom] ?? '') : sub.labelArg ? tr(sub.label ?? sub.name, { 0: String(r[sub.labelArg] ?? '') }) : labelOf(sub);
        // (in a fixed list a cell named by its column alone is named once, above the first row: Preferences > Dictionary's quotation marks)
        const shownLabel = (sub: Field) => fixed && i > 0 && !sub.labelFrom && !sub.labelArg ? '' : label(sub);
        const off = (sub: Field) => !!sub.disabled || (!!sub.disabledWhen && holdsRow(sub.disabledWhen));
        return <div key={i} data-row={i} style={{ display: 'grid', gridTemplateColumns: `repeat(${here.length},minmax(0,1fr))${fixed ? '' : removeOnly ? ' auto' : ' auto auto auto'}`, gap: 8, alignItems: 'end' }}>
        {here.map(sub => sub.type === 'boolean'
          ? <label className="page-check" key={sub.name}>{control(sub, `${f.name}/${i}/${sub.name}`, r[sub.name], off(sub), label(sub))}{shownLabel(sub)}</label>
          : <label key={sub.name} style={{ display: 'grid', gap: 4, minWidth: 0 }}>{shownLabel(sub)}{control(sub, `${f.name}/${i}/${sub.name}`, r[sub.name], off(sub), label(sub))}</label>)}
        {!fixed && <>
        <button type="button" aria-label={f.removeLabel ? tr(f.removeLabel) : tr('행 삭제')} data-field={`${f.name}/${i}/remove`} disabled={busy || list.length <= (f.minRows ?? 0)} onClick={() => rowOp(f, 'remove', i)}>−</button>
        {!removeOnly && <><button type="button" aria-label={tr('위로 이동')} data-field={`${f.name}/${i}/up`} disabled={busy || i === 0} onClick={() => rowOp(f, 'move', i, i - 1)}>↑</button>
        <button type="button" aria-label={tr('아래로 이동')} data-field={`${f.name}/${i}/down`} disabled={busy || i === list.length - 1} onClick={() => rowOp(f, 'move', i, i + 1)}>↓</button></>}
        </>}
      </div>; })}
      {!fixed && !removeOnly && <button type="button" data-field={`${f.name}/add`} disabled={busy || list.length >= (f.maxRows ?? 1000)} onClick={() => rowOp(f, 'add')}>{f.addLabel ? tr(f.addLabel) : tr('행 추가')}</button>}
    </fieldset>;
  };
  const row = (f: Field) => {
    // (a field the engine has put off for the values now says why: a print setting that does not apply)
    const offWhy = state.off?.[f.name];
    const disabled = !!f.disabled || !!offWhy || (!!f.disabledWhen && matches(f.disabledWhen)) || (alone && !!f.document), value = state.values[f.name];
    const why = f.disabled ? { 'aria-disabled': true, title: f.disabledNote ? tr(f.disabledNote) : undefined } : offWhy ? { title: tr(offWhy), 'data-off': f.name } : {};
    if (f.type === 'array' && f.editor && ROW_EDITORS[f.editor]) return <div key={f.name} data-field={f.name}>
      {ROW_EDITORS[f.editor]({ rows: (state.values[f.name] as Row[]) ?? [], change: list => change(f.name, list), choices: state.choices ?? {}, disabled: busy })}</div>;
    if (f.type === 'array') return rows(f);
    if (f.type === 'boolean' && f.radio) return <div key={f.name} role="radiogroup">{[true, false].map((on, i) => <label className="page-check" key={String(on)}>
      <input type="radio" name={f.name} data-field={f.name} disabled={disabled} checked={value === on} onChange={() => change(f.name, on)} />{tr(f.radio![i])}</label>)}</div>;
    if (f.type === 'boolean') return <label className="page-check" key={f.name} {...why}>{control(f, f.name, value, disabled)}{labelOf(f)}</label>;
    // (a grid of buttons is not one control a label can point at)
    if (f.type === 'enum' && f.grid) return <div className="dialog-grid-row" key={f.name}>{labelOf(f)}{control(f, f.name, value, disabled)}</div>;
    if (f.type !== 'number') return prefs ? <label key={f.name} {...why}>{labelOf(f)}<span className="td-field">{control(f, f.name, value, disabled)}</span></label>
      : <label key={f.name} {...why}>{labelOf(f)}{control(f, f.name, value, disabled)}</label>;
    const suffix = prefs && unitName(f) ? '' : suffixOf(f);
    return transform || prefs
      ? <label key={f.name} {...why}>{labelOf(f)}<span className="td-field">{numeric(f, f.name, value, disabled)}{suffix && <em>{suffix}</em>}</span></label>
      : <label key={f.name} {...why}>{labelOf(f)}{numeric(f, f.name, value, disabled)}{suffix}</label>;
  };
  const groups = [...new Set(def.fields.filter(shown).map(f => f.group ?? ''))];
  // a text the dialog cannot go without is asked for before OK (the engine refuses it too)
  const incomplete = def.fields.some(f => (f.type === 'text' || f.type === 'choice') && f.required && matches(f.when) && !String(state.values[f.name] ?? '').trim())
    // (and a row's box it cannot go without: a TOC entry's paragraph style)
    || def.fields.some(f => f.type === 'array' && ((state.values[f.name] as Row[]) ?? []).some(r => (f.fields ?? []).some(sub => sub.required && Object.entries(sub.when ?? {}).every(([k, v]) => r[k] === v) && r[sub.name] === '')));
  // a number being typed that its box cannot take (not a number, out of range, not whole) holds OK back until it is fixed
  const untakable = (f: Field) => {
    const text = state.drafts[f.name];
    if (f.type !== 'number' || text == null || f.unit?.startsWith('length') || f.hidden) return false;
    // read as the engine reads it (a decimal comma, a trailing % or °)
    const t = text.trim().replace(',', '.').replace(/[%°]$/, '').trim(), n = t === '' ? NaN : Number(t);
    return !Number.isFinite(n) || n < f.min || n > f.max || (!!f.integer && !Number.isInteger(n));
  };
  const listed = def.preview?.kind === 'list' ? (preview?.swatches ?? []) : null, textual = def.preview?.kind === 'text';
  // (a print plan's sheets: what OK and the buttons that wait for the preview act on)
  const sheets = def.preview?.kind === 'sheets' ? (preview?.sheets ?? []) : null;
  // (a panel draws the engine's answer its own way: File ▸ Package's plan)
  const panel = def.preview?.kind === 'panel' ? PREVIEW_PANELS[def.preview.panel ?? ''] : undefined;
  // (a computed line does not hold OK back; a picture, a list, sheets and a panel do until there is one; a panel's answer may say no)
  const svg = def.preview && !listed && !sheets && !textual && !panel ? safeSvg(preview?.svg) : null;
  const drawn = textual || ((panel ? !!preview && preview.ok !== false && preview.okAllowed !== false : listed ? listed.length > 0 : sheets ? sheets.length > 0 : !!svg) && !previewing);
  // a name another item of the document has (the item the dialog works on keeps its own)
  const clash = (f: Field) => f.type === 'text' && typeof f.unique === 'string' && (state.choices?.[f.unique] ?? []).some(c => c.label === String(state.values[f.name] ?? '').trim() && c.value !== state.ids[0]);
  const clashes = def.fields.filter(clash);
  const holdsIn = (when: Record<string, unknown> | undefined, r: Row) => Object.entries(when ?? {}).every(([k, v]) => Array.isArray(v) ? v.includes(r[k]) : r[k] === v);
  const short = (def.rowsCheck ?? []).filter(c => { const picked = ((state.values[c.field] as Row[]) ?? []).filter(r => holdsIn(c.where, r));
    return picked.length < (c.min ?? 0) || (!!c.some && !picked.some(r => holdsIn(c.some, r))); });
  const same = (def.distinct ?? []).some(([a, b]) => typeof state.values[a] === 'number' && state.values[a] === state.values[b]);
  const blocked = incomplete || def.fields.some(untakable) || clashes.length > 0 || (!!def.preview && !drawn) || same || short.length > 0 || (!!def.okDisabledWhen && matches(def.okDisabledWhen));
  // a note may name a field ({conflictWith}): the value the engine holds
  const noteArgs = Object.fromEntries(Object.entries(state.values).filter(([, v]) => typeof v === 'string' || typeof v === 'number')) as Record<string, string | number>;
  const grouped = groups.map(group => {
    const rows = def.fields.filter(f => shown(f) && (f.group ?? '') === group).map(row);
    return group ? <fieldset key={group} data-group={group} className={prefs ? 'preference-group' : undefined}><legend>{tr(group)}</legend>{rows}</fieldset> : <div key="-">{rows}</div>;
  });
  // (the groups in `columns` columns: New Document)
  const body = def.columns ? <div style={{ display: 'grid', gridTemplateColumns: `repeat(${def.columns},minmax(0,1fr))`, gap: '0 16px', alignItems: 'start' }}>{grouped}</div> : grouped;
  const activeDef = def.tabs?.find(t => t.id === activeTab), tabLabel = activeDef?.label;
  // a summary tab: each other tab's fields with their values as words (off: N/A, as InDesign's Print Summary)
  const summaryText = (f: Field) => {
    if (state.off?.[f.name]) return tr('해당 없음');
    const v = state.values[f.name];
    if (f.type === 'boolean') return v === true ? tr('켬') : tr('끔');
    if (f.type === 'enum') {
      const o = (f.options ?? []).find(x => (typeof x === 'string' ? x : x.value) === v);
      return o === undefined ? String(v) : typeof o === 'string' ? o : o.native ? o.label : tr(o.label);
    }
    return v === '' || v === undefined ? '—' : String(v);
  };
  const summary = activeDef?.summary ? <div data-dialog-summary style={{ overflow: 'auto', maxHeight: 360 }}>
    {(def.tabs ?? []).filter(t => !t.summary).map(t => <section key={t.id} data-summary-tab={t.id}><h3>{tr(t.label)}</h3>
      <table><tbody>{def.fields.filter(f => f.tab === t.id && !f.hidden).map(f => <tr key={f.name} data-summary-key={f.name}><th scope="row">{labelOf(f)}</th><td>{summaryText(f)}</td></tr>)}</tbody></table>
    </section>)}</div> : null;
  // the folder and name joined as the engine joins them (the name without the spaces around it); shown once all are there
  const parts = (def.pathPreview ?? []).map(k => String(state.values[k] ?? '').trim());
  const pathLine = parts.length && parts.every(Boolean) ? parts.reduce(documentPath) : '';
  // what a status panel is given: the values and the button running, a button's action (`run`), an action that goes at once while
  // another request is out (`now`: File ▸ Package's Cancel while it runs), a value set (`set`: back to the choices), the fields' labels
  const panelProps = {
    values: state.values, running, busy, error, run: (action: string) => void run(action),
    now: (action: string) => void window.folio.request({ cmd: 'applyDialog', scope, action }).then(({ header }) => { if (!header.ok) setError(explain(header)); }),
    set: (field: string, value: Value) => change(field, value),
    // (a field of this dialog, else of the dialog `labelsFrom` names: Print Booklet names a print setting as File ▸ Print does)
    labelOf: (name: string) => { const f = def.fields.find(x => x.name === name) ?? (def.labelsFrom ? definitions[def.labelsFrom]?.fields.find(x => x.name === name) : undefined); return f ? labelOf(f) : name; },
  };
  // a dialog in stages that is past its form (a package being made, made or failed): the status panel and the close button only
  if (def.stages && state.values[def.stages.field] !== def.stages.form) {
    return <div className="overlay page-dialog-scrim" onKeyDown={e => { e.stopPropagation(); if (e.key === 'Escape' && !busy) close(); }}>
      <div className={`modal page-dialog${def.className ? ' ' + def.className : ''}`} ref={box} role="dialog" aria-modal="true" aria-label={tr(def.title)} data-engine-dialog={state.dialog}
        data-stage={String(state.values[def.stages.field])}>
        <h2>{tr(def.title)}</h2>
        {def.status && STATUS_PANELS[def.status]?.(panelProps)}
        {error && <p role="alert" className="error">{error}</p>}
        <div className="actions"><button type="button" data-field="close" onClick={close} disabled={busy}>{def.stages.closeLabel ? tr(def.stages.closeLabel) : def.cancelLabel ? tr(def.cancelLabel) : tr('취소')}</button></div>
      </div>
    </div>;
  }
  return <div className="overlay page-dialog-scrim" onKeyDown={e => { e.stopPropagation(); if (e.key === 'Escape' && !busy) close(); }}>
    <div className={`modal page-dialog${transform ? ' transform-dialog' : ''}${prefs ? ' preferences-dialog' : ''}${def.className ? ' ' + def.className : ''}`} ref={box} role="dialog" aria-modal="true" aria-label={tr(def.title)} data-engine-dialog={state.dialog}
      style={def.style === 'wide' ? { width: 720, maxWidth: '95vw' } : undefined}>
      <h2>{tr(def.title)}</h2>
      {state.context.baseName && <p>{state.context.baseName}</p>}
      {def.tabs && !prefs && <div role="tablist" className="dialog-tabs">{def.tabs.map(t => <button type="button" role="tab" key={t.id} data-tab={t.id} aria-selected={t.id === activeTab}
        className={t.id === activeTab ? 'active' : undefined} onClick={() => setTab(t.id)}>{tr(t.label)}</button>)}</div>}
      <form onSubmit={e => { e.preventDefault(); void run('ok'); }}>
        {/* Preferences: the categories down the side, one at a time beside them */}
        {prefs && def.tabs ? <div className="preferences-body">
          <nav aria-label={tr('환경 설정 범주')} role="tablist">{def.tabs.map(t => <button type="button" role="tab" key={t.id} data-tab={t.id} data-category={t.id}
            aria-selected={t.id === activeTab} aria-current={t.id === activeTab ? 'page' : undefined} disabled={busy || (alone && !!t.document)}
            onClick={() => { setTab(t.id); setError(''); }}>{tr(t.label)}</button>)}</nav>
          <div className="preferences-panels"><section className={alone ? undefined : activeDef?.className} aria-label={tabLabel ? tr(tabLabel) : undefined}>
            {!def.fields.some(f => shown(f) && f.group) && tabLabel && <h3>{tr(tabLabel)}</h3>}{body}
            {activeDef?.note && !alone && <p className="differs-note" data-differs={activeDef.differs}>{tr(activeDef.note)}</p>}</section></div>
        </div> : <>{body}{summary}{(def.tabNotes ?? []).filter(n => n.tab === activeTab && matches(n.when))
          .map(n => <p key={n.text} className="muted small" data-tab-note={n.tab}>{tr(n.text, noteArgs)}</p>)}</>}
        {def.pagesNote && (state.context.pages ?? 0) > 1 && <p>{tr(def.pagesNote, { 0: state.context.pages })}</p>}
        {pathLine && <p className="muted small path-line" data-path-preview>{pathLine}</p>}
        {(Array.isArray(def.note) ? def.note : def.note ? [def.note] : []).map(n => typeof n === 'string' ? { text: n, when: undefined, differs: undefined } : n).filter(n => matches(n.when))
          .map(n => <p key={n.text} className={transform ? 'muted small' : n.differs ? 'p-note' : undefined} data-differs={n.differs}>{tr(n.text, { ...noteArgs, ...(state.context as Record<string, string | number>) })}</p>)}
        {short.filter(c => c.message).map(c => <p key={c.field} data-rows-check={c.field}>{tr(c.message!)}</p>)}
        {clashes.map(f => <p role="alert" key={f.name}>{f.uniqueMessage ? tr(f.uniqueMessage) : tr('올바른 값을 넣으세요: {name}', { name: labelOf(f) })}</p>)}
        {def.preview && listed && <div data-engine-preview={previewing ? 'pending' : listed.length ? 'ready' : preview?.ok === false ? 'error' : 'empty'}>
          {preview?.ok === false && <p role="alert">{tr(def.preview.failed)}</p>}
          <p aria-live="polite">{def.preview.count ? tr(def.preview.count, { 0: listed.length }) : listed.length}</p>
          <div style={{ maxHeight: 160, overflow: 'auto' }}>{listed.map(row => <div key={row.name}>{row.name}: {row.percentages.map(v => `${v}%`).join(' / ')}</div>)}</div>
        </div>}
        {def.preview && sheets && <div data-engine-preview={previewing ? 'pending' : sheets.length ? 'ready' : preview?.ok === false ? 'error' : 'empty'}>
          {preview?.ok === false && <p role="alert">{tr(def.preview.failed)}<br />{preview.error}</p>}
          <ol data-engine-plan>{sheets.map(s => <li key={s.face}>{s.cells.map(c => c.name ?? (def.preview?.blank ? tr(def.preview.blank) : '')).join(' · ')} — {s.size[0]} × {s.size[1]} pt</li>)}</ol>
        </div>}
        {/* a line the engine works out from the values (the swatches Convert to Profile would change); the last one stays while it asks again */}
        {def.preview && textual && <p data-field={def.preview.field ?? 'preview'} data-engine-preview={previewing ? 'pending' : preview?.ok === false ? 'error' : preview?.values ? 'ready' : 'empty'}
          className={preview?.ok === false ? 'error' : undefined} aria-live="polite">
          {preview?.ok === false ? tr(def.preview.failed, { 0: preview.error ?? '' }) : preview?.values ? tr(def.preview.text ?? '', preview.values) : ''}</p>}
        {def.preview && panel && <div data-engine-preview={previewing && !preview ? 'pending' : preview?.ok === false ? 'error' : preview ? 'ready' : 'empty'}>
          {preview?.ok === false ? <p role="alert">{tr(def.preview.failed, { 0: preview.error ?? '' })}</p>
            : preview ? panel({ preview, values: state.values, set: (field, value) => change(field, value) }) : <p className="muted">{tr(def.preview.pending)}</p>}
        </div>}
        {def.preview && !listed && !sheets && !textual && !panel && <div>
          <div>{tr(def.preview.title)}</div>
          <div data-engine-preview={previewing ? 'pending' : svg ? 'ready' : preview?.ok === false ? 'error' : 'empty'}
            style={{ background: '#fff', color: String(state.values.color ?? '#000'), border: '1px solid #8886', borderRadius: 4, padding: 8, minHeight: 64, overflow: 'auto', display: 'flex', alignItems: 'center' }}>
            {svg ? <img alt={tr(def.preview.alt)} data-engine-preview-svg src={`data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`} style={{ maxWidth: '100%' }} />
              : <span style={{ color: '#666' }}>{previewing ? tr(def.preview.pending) : preview?.ok === false ? tr(def.preview.failed) : tr(def.preview.empty)}</span>}
          </div>
          {preview?.ok === false && preview.error && <p role="alert" data-engine-preview-error style={{ margin: '6px 0 0' }}>{preview.error}</p>}
          {preview?.notice && def.preview.notices?.[preview.notice] && <p role="status" data-engine-preview-notice style={{ margin: '6px 0 0' }}>{tr(def.preview.notices[preview.notice], { 0: preview.font ?? '' })}</p>}
        </div>}
        {error && <p role="alert" className="error">{error}</p>}
        {/* the engine's question (the file exists: replace it?); No goes back to the dialog with its values */}
        {state.confirm && def.confirm && <div role="alertdialog" aria-label={tr(def.confirm.title)} data-confirm>
          <p>{tr(def.confirm.message, { 0: state.confirm.name })}</p>
          <button type="button" disabled={busy} onClick={() => void run(def.confirm!.action)}>{tr(def.confirm.yes)}</button>
          <button type="button" disabled={busy} onClick={() => change(def.fields.find(f => !f.hidden && f.type === 'text')?.name ?? '', state.values[def.fields.find(f => !f.hidden && f.type === 'text')?.name ?? ''])}>{tr(def.confirm.no)}</button>
        </div>}
        {!error && notice?.dialog === state.dialog && <p role="status">{tr(notice.text)}</p>}
        {/* what a button that keeps the dialog open answered (the print job sent), with the buttons that act on it */}
        {def.status && STATUS_PANELS[def.status]?.(panelProps)}
        <div className="actions"><button type="button" onClick={close} disabled={busy}>{def.cancelLabel ? tr(def.cancelLabel) : tr('취소')}</button>
          {def.buttons?.map(b => <button type="button" key={b.field} data-field={b.field} disabled={busy || !!state.confirm || (!!b.needsPreview && !drawn) || [b.disabledWhen ?? []].flat().some(c => matches(c))}
            onClick={() => b.screen ? void send('cancelDialog').then(() => finished.current?.(state.dialog, { screen: b.screen })) : b.set ? change(b.set.field, b.set.value) : void run(b.action ?? 'ok')}>{tr(b.label)}</button>)}
          <button className="primary" type="submit" data-field="ok" disabled={busy || blocked || !!state.confirm}>{busy ? tr('적용 중…') : def.okLabel ? tr(def.okLabel) : tr('확인')}</button></div>
      </form>
    </div>
  </div>;
}
