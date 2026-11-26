import { t as tr } from '../i18n';
import type { EditCommand } from '../engine';
import type { Command } from './commands';
import type { TableSelection } from './textedit';

/** Cell rotation shares the undoable Cell Options command and the current cell selection. */
export function cellRotationCommands(sel: TableSelection | null, cells: [number, number][],
  edit: (label: string, commands: EditCommand[]) => unknown): Record<string, Partial<Command>> {
  return Object.fromEntries([0, 90, 180, 270].map((rotation) => [
    `id.ctx.table.rotate.${rotation}Deg`, {
      checked: !!sel && cells.length > 0 && sel.info.cells.filter((c) => cells.some(([x, y]) => x === c.cell[0] && y === c.cell[1])).every((c) => c.rotation === rotation),
      run: sel && cells.length ? () => edit(tr('셀 텍스트 회전'), [{ op: 'setCells', table: sel.table, cells, change: { rotation } }]) : undefined,
    },
  ]));
}
