import { t as tr } from '../i18n';
import type { EditCommand } from '../engine';
import type { Command } from './commands';
import type { TableSelection } from './textedit';

export function cellDirectionCommands(sel: TableSelection | null, cells: [number, number][],
  edit: (label: string, commands: EditCommand[]) => unknown): Record<string, Partial<Command>> {
  const selected = sel?.info.cells.filter((c) => cells.some(([x, y]) => x === c.cell[0] && y === c.cell[1])) ?? [];
  return Object.fromEntries(['horizontal', 'vertical'].map((direction) => {
    const writingDirection = direction === 'horizontal';
    return [`id.ctx.table.writingDirection.${direction}`, {
      checked: selected.length > 0 && selected.every((c) => c.writingDirection === writingDirection),
      run: sel && cells.length ? () => edit(tr('셀 쓰기 방향'), [{ op: 'setCells', table: sel.table, cells, change: { writingDirection } }]) : undefined,
    }];
  }));
}
