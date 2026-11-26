import { t as tr } from '../i18n';
import { tableLines, type EditCommand } from '../engine';
import { unitOf } from '../inspect';
import type { Command } from './commands';
import type { TableActions, TableSelection } from './textedit';

export type CellSizeStep = { axis: 0 | 1; direction: -1 | 1; unit: string };

/** Read composed dimensions, including automatic row heights, before stepping each selected row/column. */
export async function cellSizeCommands(sel: TableSelection, step: CellSizeStep): Promise<EditCommand[]> {
  const lines = await tableLines({ table: sel.table });
  const sizes = new Map<number, number>();
  for (const piece of lines.pieces) for (const line of step.axis === 0 ? piece.columns : piece.rows)
    if (line.line > 0) sizes.set(line.line - 1, line.size);
  const perPoint = unitOf(step.unit).perPoint;
  const commands: EditCommand[] = [];
  for (let i = sel.from[step.axis]; i <= sel.to[step.axis]; i++) {
    const current = sizes.get(i);
    if (current === undefined) continue;
    const value = current * perPoint;
    const next = (step.direction > 0 ? Math.floor(value + 1e-7) + 1 : Math.ceil(value - 1e-7) - 1) / perPoint;
    const size = Math.max(3, Math.min(10000, next));
    commands.push(step.axis === 0 ? { op: 'setColumns', table: sel.table, columns: [i], width: size }
      : { op: 'setRows', table: sel.table, rows: [i], height: size });
  }
  return commands;
}

export function tableSizeKeyCommands(sel: TableSelection | null, actions: React.MutableRefObject<TableActions | null>,
  units: { horizontal: string; vertical: string }): Record<string, Partial<Command>> {
  return { 'table.key.autogrow': {
    shortcutVisible: true, fallThrough: true,
    run: sel ? () => actions.current?.toggleAutogrow() : undefined,
  }, ...Object.fromEntries(([
    ['wider', 0, 1], ['narrower', 0, -1], ['taller', 1, 1], ['shorter', 1, -1],
  ] as const).map(([name, axis, direction]) => [`table.key.${name}`, {
    fallThrough: true, shortcutVisible: true,
    run: sel && !sel.cursor ? () => actions.current?.resize({ axis, direction, unit: axis === 0 ? units.horizontal : units.vertical }) : undefined,
  }])) };
}

export const cellSizeLabel = (axis: 0 | 1) => axis === 0 ? tr('셀 폭 변경') : tr('셀 높이 변경');
