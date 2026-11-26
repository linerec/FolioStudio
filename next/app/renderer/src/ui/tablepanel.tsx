// Window ▸ Type & Tables ▸ Table: the selected table's size (rows, columns), the selected rows' height and the columns' width, and
// the selected cells' vertical alignment, text rotation and insets. Each number is a shared number field (ui/controls): the Table
// menu's own edit (Table Options, Cell Options: insertRows, setRows, setColumns, setCells) is made as soon as a value is chosen,
// shown provisionally while a value is being tried (arrows, the ▲▼ buttons, the wheel, a drag of the label) and Esc gives the table back.
import { useEffect, useState } from 'react';
import { tr } from '../i18n';
import type { EditCommand } from '../engine';
import { formatInteger, lengthSpec, parseInteger } from './controls/lengthfield';
import { PreviewNumberField } from './controls/previewnumberfield';
import type { PanelCtx } from './panels';
import { MAX_BODY_ROWS, SIDE_NAMES, VERTICAL, anchorCell, range, selectedCells } from './tables';
import type { TableSelection } from './textedit';

const ROTATIONS = [0, 90, 180, 270] as const;

export function TablePanel({ ctx }: { ctx: PanelCtx }) {
  const sel = ctx.tableSelection ?? null;
  if (!sel) return <p className="p-empty" style={{ padding: 10 }}>{tr('표의 셀을 선택하거나 셀 안을 누르세요.')}</p>;
  return <TableFields ctx={ctx} sel={sel} />;
}

/**
 * The size edits that take a table of `total` rows and `cols` columns to `n` rows and `c` columns (rows are added before the footer,
 * columns after the last). `total` and `cols` are the table as it was when the value began to be tried, not as a preview shows it.
 */
function sizeCommands(table: string, info: TableSelection['info'], total: number, cols: number, n: number, c: number): EditCommand[] {
  const out: EditCommand[] = [];
  if (n > total) out.push({ op: 'insertRows', table, at: total - info.footer - 1, count: n - total, before: false });
  if (n < total) out.push({ op: 'deleteRows', table, at: n - info.footer, count: total - n });
  if (c > cols) out.push({ op: 'insertColumns', table, at: cols - 1, count: c - cols, before: false });
  if (c < cols) out.push({ op: 'deleteColumns', table, at: c, count: cols - c });
  return out;
}

function TableFields({ ctx, sel }: { ctx: PanelCtx; sel: TableSelection }) {
  const info = sel.info, first = anchorCell(sel), row = info.rows[sel.from[1]], width = info.columns[sel.from[0]] ?? 0;
  const total = info.rows.length, cols = info.columns.length;
  const hu = lengthSpec(ctx.units.horizontal), vu = lengthSpec(ctx.units.vertical);
  const exact = !row.autoGrow, rowNow = row.autoGrow ? (row.minHeight ?? 3) : (row.height ?? 3);
  const ins = first.insets.map((v) => v ?? 4);
  // "all settings the same" follows the selected cell; checked, an inset written to one side is written to all four
  const sameNow = ins.every((v) => v === ins[0]);
  const [same, setSame] = useState(sameNow);
  const cellKey = JSON.stringify([sel.table, sel.from, sel.to, first.insets]);
  useEffect(() => setSame(sameNow), [cellKey]);
  const edit = (label: string, commands: EditCommand[]) => { if (commands.length) void ctx.edit(label, commands); };
  const cells = selectedCells(sel);

  // a typed value equal to the one shown changes nothing (`base` is null: no trial); a trial is always built, the field itself drops one that
  // ends where it began
  const rowHeight = (h: number, exactNow = exact, base: number | null = null): EditCommand[] =>
    base === null && Math.abs(h - rowNow) < 1e-6 && exactNow === exact ? [] : [{ op: 'setRows', table: sel.table, rows: range(sel.from[1], sel.to[1]), height: h, autoGrow: !exactNow }];
  const columnWidth = (w: number, base: number | null): EditCommand[] =>
    base === null && Math.abs(w - width) < 1e-6 ? [] : [{ op: 'setColumns', table: sel.table, columns: range(sel.from[0], sel.to[0]), width: w }];
  const cellChange = (label: string, change: NonNullable<Extract<EditCommand, { op: 'setCells' }>['change']>) => edit(label, [{ op: 'setCells', table: sel.table, cells, change }]);
  const insetCommands = (k: number, v: number, base: number | null): EditCommand[] => {
    const next = same ? [v, v, v, v] : ins.map((x, j) => (j === k ? v : x));
    if (base === null && next.every((x, j) => Math.abs(x - ins[j]) < 1e-6)) return [];
    return [{ op: 'setCells', table: sel.table, cells, change: { insets: next as [number, number, number, number] } }];
  };
  const writable = !!ctx.editable;
  return <fieldset className="table-panel" disabled={!writable} style={{ border: 0, margin: 0, padding: 10, display: 'grid', gap: 6 }}>
    <div style={{ display: 'flex', gap: 10 }}>
      <label className="tf-field">{tr('행')}<PreviewNumberField isolateKeys value={total} min={info.header + info.footer + 1} max={MAX_BODY_ROWS + info.header + info.footer} step={1} decimals={0} width={66} parse={parseInteger} format={formatInteger}
        label={tr('행 수')} undoLabel={tr('표 크기')} disabled={!writable} edit={edit} build={(v, base) => sizeCommands(sel.table, info, base ?? total, cols, v, cols)} /></label>
      <label className="tf-field">{tr('열')}<PreviewNumberField isolateKeys value={cols} min={1} max={200} step={1} decimals={0} width={66} parse={parseInteger} format={formatInteger}
        label={tr('열 수')} undoLabel={tr('표 크기')} disabled={!writable} edit={edit} build={(v, base) => sizeCommands(sel.table, info, total, base ?? cols, total, v)} /></label>
    </div>
    <div style={{ display: 'flex', gap: 6, alignItems: 'center' }}>
      <label className="tf-field">{tr('행 높이')}<PreviewNumberField isolateKeys value={rowNow} min={3} max={1e5} width={66} {...vu}
        label={tr('행 높이')} undoLabel={tr('행 높이')} disabled={!writable} edit={edit} build={(v, base) => rowHeight(v, exact, base ?? null)} /></label>
      <select aria-label={tr('행 높이 방식')} value={exact ? 'exact' : 'least'} onChange={(e) => edit(tr('행 높이'), rowHeight(rowNow, e.target.value === 'exact'))}>
        <option value="least">{tr('최소')}</option><option value="exact">{tr('정확하게')}</option>
      </select>
    </div>
    <label className="tf-field">{tr('열 폭')}<PreviewNumberField isolateKeys value={width} min={3} max={1e5} width={66} {...hu}
      label={tr('열 폭')} undoLabel={tr('열 폭')} disabled={!writable} edit={edit} build={(v, base) => columnWidth(v, base ?? null)} /></label>
    <div role="group" aria-label={tr('세로 정렬')} style={{ display: 'flex', gap: 4 }}>
      {VERTICAL().map(([v, label]) => <button key={v} type="button" aria-pressed={first.vertical === v} className={first.vertical === v ? 'on' : ''} title={label}
        onClick={() => first.vertical === v ? undefined : cellChange(tr('셀 세로 정렬'), { vertical: v })}>{label}</button>)}
    </div>
    <div role="group" aria-label={tr('텍스트 회전')} style={{ display: 'flex', gap: 4 }}>
      {ROTATIONS.map((r) => <button key={r} type="button" aria-pressed={first.rotation === r} className={first.rotation === r ? 'on' : ''} title={tr('텍스트 {0}° 회전', { 0: r })}
        onClick={() => first.rotation === r ? undefined : cellChange(tr('셀 텍스트 회전'), { rotation: r })}>{r}°</button>)}
    </div>
    <label className="check"><input type="checkbox" checked={same} onChange={(e) => setSame(e.target.checked)} /> {tr('모든 설정을 동일하게')}</label>
    <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 6 }}>
      {SIDE_NAMES().map((side, k) => <label key={k} className="tf-field">{side}
        <PreviewNumberField isolateKeys value={same ? ins[0] : ins[k]} min={0} max={1e5} width={66} {...hu}
          label={tr('셀 인세트 {0}', { 0: side })} undoLabel={tr('셀 인세트')} disabled={!writable} edit={edit} build={(v, base) => insetCommands(k, v, base ?? null)} /></label>)}
    </div>
  </fieldset>;
}
