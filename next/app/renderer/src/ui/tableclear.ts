// The Clear and Delete keys on selected table cells (tools-c1; InDesign's Text and Tables ▸ Clear = Backspace, Delete = Delete in the table
// context): the text of every selected text cell goes, the cells and their formatting stay. A cell's text is a story of its own (its `Self`),
// so this is one `deleteText` per cell with text, in one undo step. Graphic cells and empty cells are left alone (InDesign keeps the cell; its
// graphic is removed only by selecting the graphic). With the text cursor in a cell the keys edit that text as usual (the command is off).
import { t as tr } from '../i18n';
import { textStory, type EditCommand } from '../engine';
import type { Command } from './commands';
import type { TableSelection } from './textedit';

type Edit = (label: string, commands: EditCommand[]) => unknown;

/** The cells of the selection, as the table info has them (merged cells once). */
const chosen = (sel: TableSelection) => {
  const [c0, r0] = sel.from, [c1, r1] = sel.to;
  return sel.info.cells.filter((c) => c.cell[0] >= c0 && c.cell[0] <= c1 && c.cell[1] >= r0 && c.cell[1] <= r1);
};

export async function clearCells(sel: TableSelection, edit: Edit): Promise<void> {
  const commands: EditCommand[] = [];
  for (const cell of chosen(sel).filter((c) => !c.graphic && !c.empty)) {
    const story = await textStory(cell.id);
    if (story.length > 0) commands.push({ op: 'deleteText', story: cell.id, start: 0, end: story.length });
  }
  if (commands.length) await edit(tr('셀 내용 지우기'), commands);
}

export function tableClearCommands(sel: TableSelection | null, editable: boolean, edit: Edit): Record<string, Partial<Command>> {
  const run = editable && sel && !sel.cursor && chosen(sel).some((c) => !c.graphic && !c.empty) ? () => void clearCells(sel, edit) : undefined;
  return {
    'table.key.clear': { fallThrough: true, shortcutVisible: true, run },
    'table.key.delete': { fallThrough: true, shortcutVisible: true, run },
  };
}
