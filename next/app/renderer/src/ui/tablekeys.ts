import { commandFor, type Commands, type Command } from './commands';
import type { TableActions, TableSelection } from './textedit';
import { tableLines } from '../engine';

export type CellMove = { axis: 0 | 1; direction: -1 | 1; edge?: boolean; extend?: boolean };

/** The active end crosses the entire merged cell. The engine expands the resulting selection. */
export function movedCell(sel: TableSelection, move: CellMove): [number, number] {
  const end: [number, number] = [...sel.end];
  const cell = sel.info.cells.find(c => c.cell.every((n, i) => n <= end[i] && end[i] < n + c.span[i]));
  const axis = move.axis, last = (axis === 0 ? sel.info.columns : sel.info.rows).length - 1;
  end[axis] = move.edge ? (move.direction < 0 ? 0 : last)
    : cell ? cell.cell[axis] + (move.direction < 0 ? -1 : cell.span[axis]) : end[axis] + move.direction;
  end[axis] = Math.max(0, Math.min(last, end[axis]));
  return end;
}

const moves: Record<string, CellMove> = {};
for (const [name, axis, direction] of [['Left', 0, -1], ['Right', 0, 1], ['Up', 1, -1], ['Down', 1, 1]] as const) {
  moves[`table.key.move${name}`] = { axis, direction };
  moves[`table.key.select${name}`] = { axis, direction, extend: true };
}
for (const [name, axis, direction] of [['firstColumn', 1, -1], ['lastColumn', 1, 1], ['firstRow', 0, -1], ['lastRow', 0, 1]] as const)
  moves[`table.key.${name}`] = { axis, direction, edge: true };

/** The rows of the table that show in the frame of the selection (tableLines: line k lies under row k-1), first and last; null when unknown. */
export async function rowsInFrame(sel: TableSelection): Promise<[number, number] | null> {
  const { pieces } = await tableLines({ table: sel.table });
  const lines = pieces.find((p) => p.frame === sel.frame)?.rows.map((r) => r.line) ?? [];
  if (!lines.length) return null;
  const rows = sel.info.rows.length;
  return [Math.max(0, Math.min(...lines) - 1), Math.min(rows - 1, Math.max(...lines) - 1)];
}

export function tableKeyCommands(sel: TableSelection | null, actions: React.MutableRefObject<TableActions | null>): Record<string, Partial<Command>> {
  const out: Record<string, Partial<Command>> = Object.fromEntries(Object.entries(moves).map(([id, move]) => [id, {
    fallThrough: true,
    shortcutVisible: true,
    run: sel && (!sel.cursor || move.edge) ? () => actions.current?.move(move) : undefined,
  }]));
  // Move to First / Last Row in Frame (Page Up / Page Down in a table, tools-c1): the cell of the same column in the first (last) row of the
  // table that shows in this frame; a table across frames has its own first and last rows in each
  for (const [id, last] of [['table.key.firstRowInFrame', false], ['table.key.lastRowInFrame', true]] as const) {
    out[id] = { fallThrough: true, shortcutVisible: true, run: sel ? () => void rowsInFrame(sel).then((r) => {
      if (!r) return;
      const cell: [number, number] = [sel.end[0], last ? r[1] : r[0]];
      actions.current?.select(cell, cell);
    }) : undefined };
  }
  return out;
}

/** The table editor gets its contextual keys before text editing and document/tool shortcuts. */
export function runTableKey(e: KeyboardEvent, commands: Commands): boolean {
  const local = Object.fromEntries(Object.entries(commands).filter(([id, c]) => id.startsWith('table.key.') && c.run));
  const id = commandFor(e, local);
  if (!id) return false;
  local[id].run?.();
  return true;
}
