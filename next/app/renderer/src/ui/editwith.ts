// Edit ▸ Edit With: the file of a link opens in a program the person picked (Other…) or used before. The programs are this computer's
// setting, never in a document or project: their paths differ on every computer.
import { tr } from '../i18n';
import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';
import type { Command } from './commands';

const KEY = 'folio.editWith.programs';
const MAX = 8;

/** The programs used before, the latest first. */
function editorList(value: string | null): string[] {
  try { const v = JSON.parse(value ?? '[]'); return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : []; } catch { return []; }
}
export function editors(): string[] {
  return editorList(userSettings.getItem(KEY));
}

function remember(program: string) {
  userSettings.setItem(KEY, JSON.stringify([program, ...editors().filter((p) => p !== program)].slice(0, MAX)));
}
/** The program used last (the menu is drawn again when it changes). */
export function useLastEditor(): string | undefined {
  return editorList(useUserPreference(KEY))[0];
}
/** A program's name for a menu: its file name without the extension. */
export const programName = (p: string) => (p.split(/[\\/]/).pop() ?? p).replace(/\.(exe|app|bat|cmd|com)$/i, '');

/** The commands of Edit With for the file (none: nothing to edit). The list item is the program used last. */
export function editWithHandlers(file: string | null, note: (text: string) => void, last: string | undefined): Record<string, Partial<Command>> {
  const open = async (program: string) => {
    const err = await window.folio.openWith(program, file!);
    if (err) note(tr('{0}(으)로 열지 못했습니다: {1}', { 0: programName(program), 1: err }));
    else remember(program);
  };
  return {
    'id.edit.editWith.other': { run: file ? async () => { const p = await window.folio.chooseProgram(); if (p) await open(p); } : undefined },
    'id.list.editWith': last ? { label: programName(last), run: file ? () => void open(last) : undefined } : {},
  };
}
