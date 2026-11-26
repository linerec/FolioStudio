// Table menu: Create Table (Ctrl+Alt+Shift+T), Convert Text to Table, Table Options (Table
// Setup, Headers and Footers), Cell Options (Text, Strokes and Fills, Rows and Columns), Insert
// and Delete rows and columns, Delete Table, Select, Merge / Unmerge, Split Cell. The type tool
// selects cells in a table (textedit.tsx); the engine changes the table as InDesign does
// (edit/src/ops/tables.rs, oracle tables.json) and composes the rows again.
import { t as tr } from '../i18n';
import { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import type { CellChange, DiagonalLines, EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';
import type { Command } from './commands';
import { STROKE_TYPES } from './stroke';
import { TextNumberField, formatInteger, formatNumeric, parseInteger, parseNumeric, stepOfLength } from './controls/lengthfield';
import { FillPatterns, type PatternTab, type PatternValues, StrokePatterns, patternChanges, patternsValid } from './tablepatterns';
import type { TableActions, TableSelection, TextSelectionState } from './textedit';
import { cellRotationCommands } from './tablecellrotation';
import { cellDirectionCommands } from './tablecelldirection';
import { tableKeyCommands } from './tablekeys';
import { tableSizeKeyCommands } from './tablekeysize';
import { tableClearCommands } from './tableclear';
import { ALL_EDGES, EdgeProxy, type EdgeSet } from './edgeproxy';

export type TableDialog =
  | { kind: 'create' }
  | { kind: 'insert'; what: 'rows' | 'columns' }
  | { kind: 'cell'; tab: CellTab }
  | { kind: 'table'; tab?: TableTab }
  | { kind: 'goToRow' };
type CellTab = 'text' | 'graphic' | 'strokes' | 'rows' | 'diagonal';
/** Cell Options ▸ Rows and Columns ▸ Start Row (IDML Row/@StartRow), and the keys' commands for them. */
export const ROW_STARTS: [string, () => string][] = [['Anywhere', () => tr('아무 곳')], ['NextColumn', () => tr('다음 단')], ['NextFrame', () => tr('다음 프레임')],
  ['NextPage', () => tr('다음 페이지')], ['NextOddPage', () => tr('다음 홀수 페이지')], ['NextEvenPage', () => tr('다음 짝수 페이지')]];
type TableTab = 'setup' | PatternTab;
type Edit = (label: string, commands: EditCommand[]) => unknown;
type Cell = [number, number];
const SIDES = ['Top', 'Left', 'Bottom', 'Right'] as const;
const SOLID = 'StrokeStyle/$ID/Solid';
export const SIDE_NAMES = () => [tr('위쪽'), tr('왼쪽'), tr('아래쪽'), tr('오른쪽')];
export const VERTICAL = () => [['TopAlign', tr('위쪽')], ['CenterAlign', tr('가운데')], ['BottomAlign', tr('아래쪽')], ['JustifyAlign', tr('양쪽 정렬')]] as const;

/** The cells (their [column, row]) whose first row and column lie in the selection. */
export function selectedCells(sel: TableSelection): Cell[] {
  const [c0, r0] = sel.from, [c1, r1] = sel.to;
  return sel.info.cells.filter((c) => c.cell[0] >= c0 && c.cell[0] <= c1 && c.cell[1] >= r0 && c.cell[1] <= r1).map((c) => c.cell);
}
export const range = (a: number, b: number) => Array.from({ length: b - a + 1 }, (_, k) => a + k);
export const anchorCell = (sel: TableSelection) => sel.info.cells.find((c) => c.cell[0] === sel.from[0] && c.cell[1] === sel.from[1]) ?? sel.info.cells[0];

/** Rows or columns copied with Edit ▸ Copy while whole rows or whole columns of a table are selected (Table ▸ Paste Before / Paste After puts them next to another row or column). */
export type TableClip = { table: string; kind: 'rows' | 'columns'; from: number; to: number; rows: number; columns: number };
/** What Edit ▸ Copy takes from a selection of cells: whole rows, else whole columns; a part of a row or column (one cell) copies nothing here. */
export function tableClipOf(sel: TableSelection | null): TableClip | null {
  if (!sel) return null;
  const [columns, rows] = [sel.info.columns.length, sel.info.rows.length];
  const common = { table: sel.table, rows, columns };
  if (sel.from[0] === 0 && sel.to[0] === columns - 1) return { ...common, kind: 'rows', from: sel.from[1], to: sel.to[1] };
  if (sel.from[1] === 0 && sel.to[1] === rows - 1) return { ...common, kind: 'columns', from: sel.from[0], to: sel.to[0] };
  return null;
}

/** The Table menu's commands for App's command table. */
export function tableCommands({ editable, text, cells, actions, edit, open, clip, units }: {
  units: { horizontal: string; vertical: string };
  editable: boolean; text: TextSelectionState | null; cells: TableSelection | null; clip: TableClip | null;
  actions: React.MutableRefObject<TableActions | null>; edit: Edit; open: (d: TableDialog) => void;
}): Record<string, Partial<Command>> {
  const sel = editable ? cells : null;
  const when = (ok: boolean, run: () => void) => ({ run: ok ? run : undefined });
  const table = sel?.table ?? '';
  const info = sel?.info;
  const rows = info?.rows.length ?? 0, columns = info?.columns.length ?? 0;
  const single = !!sel && sel.from[0] === sel.to[0] && sel.from[1] === sel.to[1];
  const span = sel ? anchorCell(sel).span : [1, 1];
  const select = (a: Cell, b: Cell) => actions.current?.select(a, b);
  const header = info?.header ?? 0, footer = info?.footer ?? 0;
  // Convert Rows: the selected rows, when they are next to the header or footer
  const [r0, r1] = sel ? [sel.from[1], sel.to[1]] : [0, 0];
  const bodyEnd = rows - footer;
  const convert = (to: 'header' | 'body' | 'footer', ok: boolean) => when(!!sel && ok, () => {
    const label = { header: tr('머리글로 변환'), body: tr('본문으로 변환'), footer: tr('바닥글로 변환') }[to];
    edit(label, [{ op: 'convertRows', table, at: r0, count: r1 - r0 + 1, to }]);
  });
  // Convert Cell to Graphic Cell / Text Cell: the selected cells that are not of that kind yet
  // (InDesign enables Convert Cell to Graphic Cell only in an empty cell: tables-cells.json cell-menu-graphic-empty)
  const cellAt = (c: Cell) => sel?.info.cells.find((x) => x.cell[0] === c[0] && x.cell[1] === c[1]);
  const convertCells = (graphic: boolean) => {
    const todo = sel ? selectedCells(sel).filter((c) => !!cellAt(c)?.graphic !== graphic && (!graphic || !!cellAt(c)?.empty)) : [];
    return when(todo.length > 0, () => edit(graphic ? tr('셀을 그래픽 셀로 변환') : tr('셀을 텍스트 셀로 변환'), [{ op: 'setCellType', table, cells: todo, graphic }]));
  };
  const toHeader = r0 <= header && r1 >= header && r1 < bodyEnd && r1 + 1 + footer < rows;
  const toFooter = r1 + 1 >= bodyEnd && r0 < bodyEnd && r0 >= header && header + rows - r0 < rows;
  const toBody = (r1 + 1 === header && r0 < header) || (r0 === bodyEnd && footer > 0);
  return {
    'id.table.distributeRowsEvenly': when(!!sel && sel.to[1] > sel.from[1], () =>
      edit(tr('행을 균등하게 분배'), [{ op: 'distributeRows', table, first: sel!.from[1], last: sel!.to[1] }])),
    'id.table.distributeColumnsEvenly': when(!!sel && sel.to[0] > sel.from[0], () =>
      edit(tr('열을 균등하게 분배'), [{ op: 'distributeColumns', table, first: sel!.from[0], last: sel!.to[0] }])),
    'id.table.convertTableToText': when(!!sel, () => { actions.current?.exit(); edit(tr('표를 텍스트로 변환'), [{ op: 'convertTableToText', table }]); }),
    // Go to Row selects a row; Edit Header / Edit Footer put the cursor in their first cell
    'id.table.goToRow': when(!!sel, () => open({ kind: 'goToRow' })),
    'id.table.editHeader': when(!!sel && header > 0, () => actions.current?.edit([0, 0])),
    'id.table.editFooter': when(!!sel && footer > 0, () => actions.current?.edit([0, rows - footer])),
    'id.table.convertRows.toHeader': convert('header', toHeader),
    'id.table.convertRows.toBody': convert('body', toBody),
    'id.table.convertRows.toFooter': convert('footer', toFooter),
    'id.ctx.table.convertToHeaderRows': convert('header', toHeader),
    'id.ctx.table.convertToBodyRows': convert('body', toBody),
    'id.ctx.table.convertToFooterRows': convert('footer', toFooter),
    // in a story's text (not a cell's: tables in cells are not built yet)
    'table.create': when(editable && !!text && !cells, () => open({ kind: 'create' })),
    'table.convertText': when(editable && !!text && !cells && text.start < text.end, () =>
      edit(tr('텍스트를 표로 변환'), [{ op: 'convertToTable', story: text!.story, start: text!.start, end: text!.end }])),
    'id.table.tableOptions.tableSetup': when(!!sel, () => open({ kind: 'table', tab: 'setup' })),
    'id.table.tableOptions.headersAndFooters': when(!!sel, () => open({ kind: 'table', tab: 'setup' })),
    'id.table.tableOptions.alternatingRowStrokes': when(!!sel?.info.options, () => open({ kind: 'table', tab: 'rowStrokes' })),
    'id.table.tableOptions.alternatingColumnStrokes': when(!!sel?.info.options, () => open({ kind: 'table', tab: 'columnStrokes' })),
    'id.table.tableOptions.alternatingFills': when(!!sel?.info.options, () => open({ kind: 'table', tab: 'fills' })),
    'id.table.cellOptions.text': when(!!sel, () => open({ kind: 'cell', tab: 'text' })),
    'id.table.cellOptions.graphic': when(!!sel, () => open({ kind: 'cell', tab: 'graphic' })),
    'id.table.cellOptions.strokesAndFills': when(!!sel, () => open({ kind: 'cell', tab: 'strokes' })),
    'id.table.cellOptions.rowsAndColumns': when(!!sel, () => open({ kind: 'cell', tab: 'rows' })),
    'id.table.cellOptions.diagonalLines': when(!!sel, () => open({ kind: 'cell', tab: 'diagonal' })),
    'id.table.convertCellToGraphicCell': convertCells(true),
    // Keep Options from the keys (tools-c2; InDesign's commands without default keys): Keep with Next Row turned on/off, Start Row
    'table.key.keepWithNextRow': when(!!sel, () => { const rows = range(sel!.from[1], sel!.to[1]); edit(tr('다음 행과 함께 유지'), [{ op: 'setRowKeeps', table, rows, keep_with_next: !sel!.info.rows[sel!.from[1]]?.keepWithNext, start: undefined }]); }),
    ...Object.fromEntries(ROW_STARTS.map(([v, label]) => [`table.key.startRow.${v}`, when(!!sel, () => edit(tr('행 시작: {0}', { 0: label() }), [{ op: 'setRowKeeps', table, rows: range(sel!.from[1], sel!.to[1]), keep_with_next: undefined, start: v }]))])),
    'id.table.convertCellToTextCell': convertCells(false),
    'id.table.insert.row': when(!!sel, () => open({ kind: 'insert', what: 'rows' })),
    'id.table.insert.column': when(!!sel, () => open({ kind: 'insert', what: 'columns' })),
    // Paste Before / After: the copied rows (columns) go beside the selected ones; the table has to have as many columns (rows)
    ...Object.fromEntries([true, false].map((before) => [before ? 'id.table.pasteBefore' : 'id.table.pasteAfter', when(!!sel && !!clip && (clip.kind === 'rows' ? clip.columns === columns : clip.rows === rows), () => {
      const c = clip!, at = c.kind === 'rows' ? (before ? sel!.from[1] : sel!.to[1]) : (before ? sel!.from[0] : sel!.to[0]);
      edit(before ? tr('앞에 붙이기') : tr('뒤에 붙이기'), [{ op: c.kind === 'rows' ? 'pasteRows' : 'pasteColumns', table, ...(c.table === table ? {} : { source: c.table }), from: c.from, to: c.to, at, before }]);
    })])),
    'id.table.delete.row': when(!!sel && sel.to[1] - sel.from[1] + 1 < rows, () =>
      edit(tr('행 삭제'), [{ op: 'deleteRows', table, at: sel!.from[1], count: sel!.to[1] - sel!.from[1] + 1 }])),
    'id.table.delete.column': when(!!sel && sel.to[0] - sel.from[0] + 1 < columns, () =>
      edit(tr('열 삭제'), [{ op: 'deleteColumns', table, at: sel!.from[0], count: sel!.to[0] - sel!.from[0] + 1 }])),
    'id.table.delete.table': when(!!sel, () => { actions.current?.exit(); edit(tr('표 삭제'), [{ op: 'deleteTable', table }]); }),
    'id.table.select.cell': when(!!sel, () => select(sel!.anchor, sel!.anchor)),
    'id.table.select.row': when(!!sel, () => select([0, sel!.from[1]], [columns - 1, sel!.to[1]])),
    'id.table.select.column': when(!!sel, () => select([sel!.from[0], 0], [sel!.to[0], rows - 1])),
    'id.table.select.table': when(!!sel, () => select([0, 0], [columns - 1, rows - 1])),
    'id.table.select.headerRows': when(!!sel && header > 0, () => select([0, 0], [columns - 1, header - 1])),
    'id.table.select.bodyRows': when(!!sel && rows - header - footer > 0, () => select([0, header], [columns - 1, rows - footer - 1])),
    'id.table.select.footerRows': when(!!sel && footer > 0, () => select([0, rows - footer], [columns - 1, rows - 1])),
    'table.merge': when(!!sel && !single, () => edit(tr('셀 병합'), [{ op: 'mergeCells', table, from: sel!.from, to: sel!.to }])),
    'id.table.unmergeCells': when(!!sel && (span[0] > 1 || span[1] > 1) && selectedCells(sel).length === 1, () =>
      edit(tr('셀 병합 해제'), [{ op: 'unmergeCell', table, cell: sel!.from }])),
    'id.table.splitCellHorizontally': when(!!sel && selectedCells(sel).length === 1 && span[1] === 1, () =>
      edit(tr('셀을 가로로 분할'), [{ op: 'splitCell', table, cell: sel!.from, vertical: false }])),
    'id.table.splitCellVertically': when(!!sel && selectedCells(sel).length === 1 && span[0] === 1, () =>
      edit(tr('셀을 세로로 분할'), [{ op: 'splitCell', table, cell: sel!.from, vertical: true }])),
    ...cellRotationCommands(sel, sel ? selectedCells(sel) : [], edit),
    ...cellDirectionCommands(sel, sel ? selectedCells(sel) : [], edit),
    ...tableKeyCommands(sel, actions),
    ...tableSizeKeyCommands(sel, actions, units),
    ...tableClearCommands(sel, editable, edit),
  };
}

export function Modal({ title, close, ok, submit, className, children }: {
  title: string; close: () => void; ok: boolean; submit: () => void; className?: string; children: React.ReactNode;
}) {
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={title} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className={`card dialog style-options table-options ${className ?? ''}`} onSubmit={(e) => { e.preventDefault(); if (ok) { submit(); close(); } }}>
        <h2>{title}</h2>
        {children}
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!ok}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}

/** InDesign 2025's most body rows in a table (`Table.add` bodyRowCount and the Table Setup field: 1 to 10000; measured, oracle/measured/tablelimits-2025.json). */
export const MAX_BODY_ROWS = 10000;

const count = (text: string, min: number, max: number) => { const n = Number(text); return Number.isInteger(n) && n >= min && n <= max ? n : null; };

/** Table ▸ Create Table: body rows, columns, header and footer rows at the insertion point. */
export function CreateTableDialog({ text, edit, close }: { text: TextSelectionState; edit: Edit; close: () => void }) {
  const [body, setBody] = useState('4'), [cols, setCols] = useState('4'), [head, setHead] = useState('0'), [foot, setFoot] = useState('0');
  const b = count(body, 1, MAX_BODY_ROWS), c = count(cols, 1, 200), h = count(head, 0, 25), f = count(foot, 0, 25);
  const ok = b !== null && c !== null && h !== null && f !== null;
  return (
    <Modal title={tr('표 만들기')} close={close} ok={ok} submit={() => edit(tr('표 만들기'), [{ op: 'insertTable', story: text.story, at: text.start, rows: b!, columns: c!, header: h!, footer: f! }])}>
      <fieldset><legend>{tr('표 크기')}</legend>
        <label>{tr('본문 행:')}<TextNumberField width={220} label={tr('본문 행')} text={body} onText={setBody} format={formatInteger} parse={parseInteger} min={1} max={MAX_BODY_ROWS} step={1} decimals={0} /></label>
        <label>{tr('열:')}<TextNumberField width={220} label={tr('열')} text={cols} onText={setCols} format={formatInteger} parse={parseInteger} min={1} max={200} step={1} decimals={0} /></label>
        <label>{tr('머리글 행:')}<TextNumberField width={220} label={tr('머리글 행')} text={head} onText={setHead} format={formatInteger} parse={parseInteger} min={0} max={25} step={1} decimals={0} /></label>
        <label>{tr('바닥글 행:')}<TextNumberField width={220} label={tr('바닥글 행')} text={foot} onText={setFoot} format={formatInteger} parse={parseInteger} min={0} max={25} step={1} decimals={0} /></label>
      </fieldset>
    </Modal>
  );
}

/** Table ▸ Insert ▸ Row / Column: how many, and on which side of the selected cells. */
export function InsertDialog({ sel, what, edit, close }: { sel: TableSelection; what: 'rows' | 'columns'; edit: Edit; close: () => void }) {
  const [n, setN] = useState('1');
  const [before, setBefore] = useState(false);
  const k = count(n, 1, what === 'rows' ? 1000 : 200);
  const rows = what === 'rows';
  const submit = () => {
    const at = rows ? (before ? sel.from[1] : sel.to[1]) : (before ? sel.from[0] : sel.to[0]);
    edit(rows ? tr('행 삽입') : tr('열 삽입'), [{ op: rows ? 'insertRows' : 'insertColumns', table: sel.table, at, count: k!, before }]);
  };
  return (
    <Modal title={rows ? tr('행 삽입') : tr('열 삽입')} close={close} ok={k !== null} submit={submit}>
      <fieldset><legend>{tr('삽입')}</legend>
        <label>{tr('개수:')}<TextNumberField width={220} label={tr('개수')} text={n} onText={setN} format={formatInteger} parse={parseInteger} min={1} max={what === 'rows' ? 1000 : 200} step={1} decimals={0} /></label>
        <label className="check"><input type="radio" name="where" checked={before} onChange={() => setBefore(true)} /> {rows ? tr('위') : tr('왼쪽')}</label>
        <label className="check"><input type="radio" name="where" checked={!before} onChange={() => setBefore(false)} /> {rows ? tr('아래') : tr('오른쪽')}</label>
      </fieldset>
    </Modal>
  );
}

/** Table ▸ Go to Row…: a header, body or footer row by its number in that part, selected. */
export function GoToRowDialog({ sel, go, close }: { sel: TableSelection; go: (row: number) => void; close: () => void }) {
  const rows = sel.info.rows.length, header = sel.info.header ?? 0, footer = sel.info.footer ?? 0, body = rows - header - footer;
  const parts = ([['header', header], ['body', body], ['footer', footer]] as const).filter(([, n]) => n > 0);
  const [part, setPart] = useState<'header' | 'body' | 'footer'>(body > 0 ? 'body' : parts[0]?.[0] ?? 'body');
  const [row, setRow] = useState('1');
  const n = parts.find(([p]) => p === part)?.[1] ?? 0;
  const k = count(row, 1, Math.max(1, n));
  const first = part === 'header' ? 0 : part === 'body' ? header : header + body;
  const name = { header: tr('머리글'), body: tr('본문'), footer: tr('바닥글') };
  return (
    <Modal title={tr('행으로 이동')} close={close} ok={k !== null && n > 0} submit={() => go(first + k! - 1)}>
      <fieldset><legend>{tr('행')}</legend>
        {parts.length > 1 && <label>{tr('영역:')}<select aria-label={tr('영역')} value={part} onChange={(e) => { setPart(e.target.value as typeof part); setRow('1'); }}>
          {parts.map(([p]) => <option key={p} value={p}>{name[p]}</option>)}</select></label>}
        <label>{tr('행 번호:')}<TextNumberField width={220} label={tr('행 번호')} text={row} onText={setRow} format={formatInteger} parse={parseInteger} min={1} max={Math.max(1, n)} step={1} decimals={0} /></label>
      </fieldset>
    </Modal>
  );
}

/** Table ▸ Table Options ▸ Table Setup / Headers and Footers: body rows and columns (added or
 * taken at the end), header and footer rows (more are new empty rows, fewer delete those next to
 * the body); Row Strokes, Column Strokes and Fills: the alternating patterns (tablepatterns.tsx). */
export function TableOptionsDialog({ sel, info, start, edit, close }: { sel: TableSelection; info: DocumentInfo | null; start: TableTab; edit: Edit; close: () => void }) {
  const i = sel.info;
  const [tab, setTab] = useState<TableTab>(start);
  const swatches = useMemo(() => (info?.swatches ?? []).filter((s) => s.model !== 'gradient'), [info]);
  // the pattern settings as the dialog opens, and as they are being edited (all three tabs at once: one undo step for OK)
  const initial = useMemo<PatternValues>(() => i.options ?? {}, [i]);
  const [values, setValues] = useState<PatternValues>(initial);
  const setValue = (key: string, value: string) => setValues((v) => ({ ...v, [key]: value }));
  const tabs: PatternTab[] = ['rowStrokes', 'columnStrokes', 'fills'];
  const patternsOk = tabs.every((p) => patternsValid(p, values));
  const invalidTabs = tabs.filter((p) => !patternsValid(p, values));
  const changed = Object.assign({}, ...tabs.map((p) => patternChanges(p, initial, values))) as Record<string, string>;
  const body0 = i.rows.length - i.header - i.footer, cols0 = i.columns.length;
  const [body, setBody] = useState(String(body0)), [cols, setCols] = useState(String(cols0)), [head, setHead] = useState(String(i.header));
  const [foot, setFoot] = useState(String(i.footer));
  const b = count(body, 1, MAX_BODY_ROWS), c = count(cols, 1, 200), h = count(head, 0, 25), f = count(foot, 0, 25);
  const submit = () => {
    const out: EditCommand[] = [];
    const lastBody = i.header + body0 - 1;
    if (b! > body0) out.push({ op: 'insertRows', table: sel.table, at: lastBody, count: b! - body0, before: false });
    if (b! < body0) out.push({ op: 'deleteRows', table: sel.table, at: i.header + b!, count: body0 - b! });
    if (c! > cols0) out.push({ op: 'insertColumns', table: sel.table, at: cols0 - 1, count: c! - cols0, before: false });
    if (c! < cols0) out.push({ op: 'deleteColumns', table: sel.table, at: c!, count: cols0 - c! });
    if (h! !== i.header) out.push({ op: 'setHeaderRows', table: sel.table, header: h! });
    if (f! !== i.footer) out.push({ op: 'setFooterRows', table: sel.table, footer: f! });
    if (Object.keys(changed).length) out.push({ op: 'setTableOptions', table: sel.table, options: changed });
    if (out.length) edit(tr('표 옵션'), out);
  };
  const labels: Record<TableTab, string> = { setup: tr('표 설정'), rowStrokes: tr('행 획'), columnStrokes: tr('열 획'), fills: tr('칠') };
  return (
    <Modal title={tr('표 옵션')} close={close} ok={b !== null && c !== null && h !== null && f !== null && patternsOk} submit={submit} className={tab === 'setup' ? 'cell-options' : 'cell-options pattern-options'}>
      <div className="tabs" role="tablist">
        {(['setup', ...(i.options ? tabs : [])] as TableTab[]).map((k) => <button key={k} type="button" role="tab" aria-selected={tab === k} onClick={() => setTab(k)}>{labels[k]}</button>)}
      </div>
      {invalidTabs.length > 0 && <p role="alert" className="field-error">{tr('{0} 탭의 값을 확인하세요.', { 0: invalidTabs.map((p) => labels[p]).join(', ') })}</p>}
      {tab === 'rowStrokes' && <StrokePatterns dim="Row" values={values} set={setValue} swatches={swatches} />}
      {tab === 'columnStrokes' && <StrokePatterns dim="Column" values={values} set={setValue} swatches={swatches} />}
      {tab === 'fills' && <FillPatterns values={values} set={setValue} swatches={swatches} />}
      {tab === 'setup' && <>
      <fieldset><legend>{tr('표 크기')}</legend>
        <label>{tr('본문 행:')}<TextNumberField width={220} label={tr('본문 행')} text={body} onText={setBody} format={formatInteger} parse={parseInteger} min={1} max={MAX_BODY_ROWS} step={1} decimals={0} /></label>
        <label>{tr('열:')}<TextNumberField width={220} label={tr('열')} text={cols} onText={setCols} format={formatInteger} parse={parseInteger} min={1} max={200} step={1} decimals={0} /></label>
        <label>{tr('머리글 행:')}<TextNumberField width={220} label={tr('머리글 행')} text={head} onText={setHead} format={formatInteger} parse={parseInteger} min={0} max={25} step={1} decimals={0} /></label>
        <label>{tr('바닥글 행:')}<TextNumberField width={220} label={tr('바닥글 행')} text={foot} onText={setFoot} format={formatInteger} parse={parseInteger} min={0} max={25} step={1} decimals={0} /></label>
      </fieldset></>}
    </Modal>
  );
}

/** Table ▸ Cell Options: Text (insets, vertical justification), Strokes and Fills (the selected
 * cells' outer edges as chosen, and their fill), Rows and Columns (height, width). */
export function CellOptionsDialog({ sel, info, start, edit, close, format, parse }: {
  sel: TableSelection; info: DocumentInfo | null; start: CellTab; edit: Edit; close: () => void;
  format: (pt: number) => string; parse: (text: string) => number | null;
}) {
  const first = anchorCell(sel);
  const step = stepOfLength(format, parse);
  const swatches = useMemo(() => (info?.swatches ?? []).filter((s) => s.model !== 'gradient'), [info]);
  const [tab, setTab] = useState<CellTab>(start);
  const ins0 = first.insets.map((v) => v ?? 4);
  const [insets, setInsets] = useState(ins0.map(format));
  const [same, setSame] = useState(ins0.every((v) => v === ins0[0]));
  const [vertical, setVertical] = useState(first.vertical);
  const [edges, setEdges] = useState<EdgeSet>(ALL_EDGES);
  const s0 = first.strokes.Top;
  const [weight, setWeight] = useState(format(s0.weight ?? 1));
  const [color, setColor] = useState(s0.color ?? 'Color/Black');
  const [fill, setFill] = useState(first.fill ?? 'Swatch/None');
  const [tint, setTint] = useState(String(first.fillTint ?? 100));
  const row0 = sel.info.rows[sel.from[1]], width0 = sel.info.columns[sel.from[0]] ?? 0;
  const [exact, setExact] = useState(!row0.autoGrow);
  const [height, setHeight] = useState(format(row0.autoGrow ? (row0.minHeight ?? 3) : (row0.height ?? 3)));
  const [width, setWidth] = useState(format(width0));
  // Keep Options (tools-c2): as the first selected row has them
  const [keepNext, setKeepNext] = useState(!!row0.keepWithNext), [startRow, setStartRow] = useState(row0.start ?? 'Anywhere');
  // Diagonal Lines and Graphic (clip): as the first selected cell has them
  const d0 = first.diagonal;
  const [dLeft, setDLeft] = useState(d0.topLeft), [dRight, setDRight] = useState(d0.topRight), [front, setFront] = useState(d0.inFront);
  const [dWeight, setDWeight] = useState(format(d0.weight ?? 1)), [dType, setDType] = useState(d0.strokeType ?? SOLID);
  const [dColor, setDColor] = useState(d0.color ?? 'Color/Black'), [dTint, setDTint] = useState(String(d0.tint ?? 100));
  const [clip, setClip] = useState(first.clip);
  const dw = parse(dWeight), dt = Number(dTint);
  const ins = insets.map(parse), w = parse(weight), t = Number(tint), h = parse(height), cw = parse(width);
  const ok = ins.every((v) => v !== null && v >= 0) && w !== null && w >= 0 && Number.isFinite(t) && t >= 0 && t <= 100
    && h !== null && h >= 3 && cw !== null && cw >= 3 && dw !== null && dw >= 0 && Number.isFinite(dt) && dt >= 0 && dt <= 100;
  const setInset = (k: number, text: string) => setInsets((xs) => xs.map((x, j) => (same || j === k ? text : x)));
  const submit = () => {
    const cells = selectedCells(sel);
    const out: EditCommand[] = [];
    const change: CellChange = {};
    if (ins.some((v, k) => Math.abs((v as number) - ins0[k]) > 1e-6)) change.insets = ins as [number, number, number, number];
    if (vertical !== first.vertical) change.vertical = vertical;
    if (fill !== (first.fill ?? 'Swatch/None')) change.fill = fill;
    if (Math.abs(t - (first.fillTint ?? 100)) > 1e-6) change.fillTint = t;
    if (clip !== first.clip) change.clip = clip;
    const diagonal: DiagonalLines = {};
    if (dLeft !== d0.topLeft) diagonal.topLeft = dLeft;
    if (dRight !== d0.topRight) diagonal.topRight = dRight;
    if (front !== d0.inFront) diagonal.inFront = front;
    if (Math.abs((dw as number) - (d0.weight ?? 1)) > 1e-6) diagonal.weight = dw as number;
    if (dType !== (d0.strokeType ?? SOLID)) diagonal.strokeType = dType;
    if (dColor !== (d0.color ?? 'Color/Black')) diagonal.color = dColor;
    if (Math.abs(dt - (d0.tint ?? 100)) > 1e-6) diagonal.tint = dt;
    if (Object.keys(diagonal).length) change.diagonal = diagonal;
    if (Object.keys(change).length) out.push({ op: 'setCells', table: sel.table, cells, change });
    // edges: the outer edges of the selection on the chosen sides
    const strokeChanged = Math.abs((w as number) - (s0.weight ?? 1)) > 1e-6 || color !== (s0.color ?? 'Color/Black');
    if (tab === 'strokes' || strokeChanged) {
      const picked = sel.info.cells.filter((c) => cells.some((x) => x[0] === c.cell[0] && x[1] === c.cell[1]));
      SIDES.forEach((side) => {
        if (!edges[side] || !strokeChanged) return;
        const on = picked.filter((c) =>
          side === 'Top' ? c.cell[1] === sel.from[1] : side === 'Bottom' ? c.cell[1] + c.span[1] - 1 === sel.to[1]
            : side === 'Left' ? c.cell[0] === sel.from[0] : c.cell[0] + c.span[0] - 1 === sel.to[0]).map((c) => c.cell);
        if (on.length) out.push({ op: 'setCells', table: sel.table, cells: on, change: { strokes: { [side]: { weight: w as number, color } } } });
      });
      // the inner lines of the picture: the edge under (or right of) every selected cell that is not on the selection's last row (column)
      if (strokeChanged && edges.InnerH) {
        const on = picked.filter((c) => c.cell[1] + c.span[1] - 1 < sel.to[1]).map((c) => c.cell);
        if (on.length) out.push({ op: 'setCells', table: sel.table, cells: on, change: { strokes: { Bottom: { weight: w as number, color } } } });
      }
      if (strokeChanged && edges.InnerV) {
        const on = picked.filter((c) => c.cell[0] + c.span[0] - 1 < sel.to[0]).map((c) => c.cell);
        if (on.length) out.push({ op: 'setCells', table: sel.table, cells: on, change: { strokes: { Right: { weight: w as number, color } } } });
      }
    }
    const rows = range(sel.from[1], sel.to[1]), cols = range(sel.from[0], sel.to[0]);
    if (exact !== !row0.autoGrow || Math.abs((h as number) - (row0.autoGrow ? (row0.minHeight ?? 3) : (row0.height ?? 3))) > 1e-6) {
      out.push({ op: 'setRows', table: sel.table, rows, height: h as number, autoGrow: !exact });
    }
    if (Math.abs((cw as number) - width0) > 1e-6) out.push({ op: 'setColumns', table: sel.table, columns: cols, width: cw as number });
    if (keepNext !== !!row0.keepWithNext || startRow !== (row0.start ?? 'Anywhere')) {
      out.push({ op: 'setRowKeeps', table: sel.table, rows, keep_with_next: keepNext !== !!row0.keepWithNext ? keepNext : undefined, start: startRow !== (row0.start ?? 'Anywhere') ? startRow : undefined });
    }
    if (out.length) edit(tr('셀 옵션'), out);
  };
  const swatchSelect = (label: string, value: string, set: (v: string) => void) => (
    <select aria-label={label} value={value} onChange={(e) => set(e.target.value)}>
      {!swatches.some((s) => s.id === value) && <option value={value}>{value.replace(/^(Color|Swatch)\//, '')}</option>}
      {swatches.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
    </select>
  );
  const insetFields = (
    <fieldset><legend>{tr('셀 인세트')}</legend>
      <label className="check"><input type="checkbox" checked={same} onChange={(e) => { setSame(e.target.checked); if (e.target.checked) setInsets((xs) => xs.map(() => xs[0])); }} /> {tr('모든 설정을 동일하게')}</label>
      {SIDE_NAMES().map((side, k) => <label key={k}>{side}<TextNumberField width={220} label={tr('셀 인세트 {0}', { 0: side })} text={insets[k]} onText={(t) => setInset(k, t)} format={format} parse={parse} min={0} step={step} /></label>)}
    </fieldset>
  );
  return (
    <Modal title={tr('셀 옵션')} close={close} ok={ok} submit={submit} className="cell-options">
      <div className="tabs" role="tablist">
        <button type="button" role="tab" aria-selected={tab === 'text'} onClick={() => setTab('text')}>{tr('텍스트')}</button>
        <button type="button" role="tab" aria-selected={tab === 'graphic'} onClick={() => setTab('graphic')}>{tr('그래픽')}</button>
        <button type="button" role="tab" aria-selected={tab === 'strokes'} onClick={() => setTab('strokes')}>{tr('획 및 칠')}</button>
        <button type="button" role="tab" aria-selected={tab === 'rows'} onClick={() => setTab('rows')}>{tr('행 및 열')}</button>
        <button type="button" role="tab" aria-selected={tab === 'diagonal'} onClick={() => setTab('diagonal')}>{tr('대각선')}</button>
      </div>
      {tab === 'text' ? (
        <>
          {insetFields}
          <fieldset><legend>{tr('세로 균등 배치')}</legend>
            <label>{tr('정렬')}<select aria-label={tr('세로 정렬')} value={vertical} onChange={(e) => setVertical(e.target.value)}>
              {VERTICAL().map(([v, label]) => <option key={v} value={v}>{label}</option>)}</select></label>
          </fieldset>
        </>
      ) : tab === 'graphic' ? (
        <>
          {insetFields}
          <fieldset><legend>{tr('자르기')}</legend>
            <label className="check"><input type="checkbox" aria-label={tr('내용을 셀에 맞게 자르기')} checked={clip} onChange={(e) => setClip(e.target.checked)} /> {tr('내용을 셀에 맞게 자르기')}</label>
          </fieldset>
        </>
      ) : tab === 'diagonal' ? (
        <>
          <fieldset><legend>{tr('대각선')}</legend>
            <label className="check"><input type="checkbox" aria-label={tr('왼쪽 위에서 오른쪽 아래로')} checked={dLeft} onChange={(e) => setDLeft(e.target.checked)} /> {tr('왼쪽 위에서 오른쪽 아래로')}</label>
            <label className="check"><input type="checkbox" aria-label={tr('오른쪽 위에서 왼쪽 아래로')} checked={dRight} onChange={(e) => setDRight(e.target.checked)} /> {tr('오른쪽 위에서 왼쪽 아래로')}</label>
            <label className="check"><input type="checkbox" aria-label={tr('텍스트 앞에 선 그리기')} checked={front} onChange={(e) => setFront(e.target.checked)} /> {tr('텍스트 앞에 선 그리기')}</label>
          </fieldset>
          <fieldset><legend>{tr('선')}</legend>
            <label>{tr('두께:')}<TextNumberField width={220} label={tr('대각선 두께')} text={dWeight} onText={setDWeight} format={format} parse={parse} min={0} step={step} /></label>
            <label>{tr('유형:')}<select aria-label={tr('대각선 유형')} value={dType} onChange={(e) => setDType(e.target.value)}>
              {!STROKE_TYPES.some(([id]) => id === dType) && <option value={dType}>{dType.replace('StrokeStyle/$ID/', '')}</option>}
              {STROKE_TYPES.map(([id, label]) => <option key={id} value={id}>{label()}</option>)}</select></label>
            <label>{tr('색상:')}{swatchSelect(tr('대각선 색상'), dColor, setDColor)}</label>
            <label>{tr('색조:')}<TextNumberField width={220} label={tr('대각선 색조')} text={dTint} onText={setDTint} format={formatNumeric} parse={parseNumeric} min={0} max={100} step={1} /></label>
            <label className={`check`} title={tr('획 오버프린트')}><input type="checkbox" disabled /> {tr('획 오버프린트')}</label>
            <label aria-disabled="true" title={tr('간격 색상')}>{tr('간격 색상:')}<select disabled aria-label={tr('대각선 간격 색상')}><option>{tr('없음')}</option></select></label>
            <label className={`check`} title={tr('간격 오버프린트')}><input type="checkbox" disabled /> {tr('간격 오버프린트')}</label>
          </fieldset>
        </>
      ) : tab === 'strokes' ? (
        <>
          <fieldset><legend>{tr('셀 획')}</legend>
            <div className="cell-edges">
              <EdgeProxy edges={edges} onChange={setEdges} rows={sel.to[1] - sel.from[1] + 1} columns={sel.to[0] - sel.from[0] + 1} />
            </div>
            <label>{tr('두께:')}<TextNumberField width={220} label={tr('획 두께')} text={weight} onText={setWeight} format={format} parse={parse} min={0} step={step} /></label>
            <label>{tr('색상:')}{swatchSelect(tr('획 색상'), color, setColor)}</label>
          </fieldset>
          <fieldset><legend>{tr('셀 칠')}</legend>
            <label>{tr('색상:')}{swatchSelect(tr('칠 색상'), fill, setFill)}</label>
            <label>{tr('색조:')}<TextNumberField width={220} label={tr('칠 색조')} text={tint} onText={setTint} format={formatNumeric} parse={parseNumeric} min={0} max={100} step={1} /></label>
          </fieldset>
        </>
      ) : (
        <>
          <fieldset><legend>{tr('행 높이')}</legend>
            <label>{tr('행 높이:')}<select aria-label={tr('행 높이 방식')} value={exact ? 'exact' : 'atLeast'} onChange={(e) => setExact(e.target.value === 'exact')}>
              <option value="atLeast">{tr('최소')}</option><option value="exact">{tr('정확히')}</option></select>
              <TextNumberField width={220} label={tr('행 높이')} text={height} onText={setHeight} format={format} parse={parse} min={3} step={step} /></label>
          </fieldset>
          <fieldset><legend>{tr('열 폭')}</legend>
            <label>{tr('열 폭:')}<TextNumberField width={220} label={tr('열 폭')} text={width} onText={setWidth} format={format} parse={parse} min={3} step={step} /></label>
          </fieldset>
          <fieldset data-keep-options><legend>{tr('유지 옵션')}</legend>
            <label>{tr('행 시작:')}<select aria-label={tr('행 시작')} value={startRow} onChange={(e) => setStartRow(e.target.value)}>
              {ROW_STARTS.map(([v, label]) => <option key={v} value={v}>{label()}</option>)}</select></label>
            <label><input type="checkbox" aria-label={tr('다음 행과 함께 유지')} checked={keepNext} onChange={(e) => setKeepNext(e.target.checked)} />{tr('다음 행과 함께 유지')}</label>
          </fieldset>
        </>
      )}
    </Modal>
  );
}
