// The word space keys (tools-c1; InDesign's Text and Tables ▸ Increase / Decrease word space, Ctrl+Alt+\ and Ctrl+Alt+Backspace, with Shift five
// times): the space between the words of the selected text grows or shrinks by the document's kerning increment (Units & Increments, 1/1000 em),
// as tracking on each word space of the selection (each keeps its own value plus the step). One undo step. Without a range the keys fall through.
// Folio's choice: InDesign's own rule (kerning or tracking of the space) could not be measured; the tracking of the spaces gives the same look.
import { t as tr } from '../i18n';
import { textFormat, textStory, type EditCommand } from '../engine';
import type { Command } from './commands';

type Range = { story: string; start: number; end: number };
type Edit = (label: string, commands: EditCommand[]) => unknown;
/** Word spaces: the space, the no-break and the fixed-width spaces InDesign inserts (en, em, thin, hair, figure, punctuation, sixth, quarter, third). */
const SPACE = /[   -  　]/;
const MAX = 2000;

export async function stepWordSpaces(range: Range, step: number, edit: Edit): Promise<number> {
  const text = (await textStory(range.story)).text.slice(range.start, range.end);
  const commands: EditCommand[] = [];
  for (let i = 0; i < text.length && commands.length < MAX; i++) {
    if (!SPACE.test(text[i])) continue;
    const at = range.start + i;
    const { format } = await textFormat(range.story, at, at + 1);
    const now = typeof format.tracking === 'number' ? format.tracking : 0;
    commands.push({ op: 'textFormat', story: range.story, start: at, end: at + 1, props: { tracking: Math.max(-1000, Math.min(10000, Math.round((now + step) * 1e4) / 1e4)) } });
  }
  if (commands.length) await edit(step > 0 ? tr('낱말 간격 늘리기') : tr('낱말 간격 줄이기'), commands);
  return commands.length;
}

export function wordSpaceCommands(range: Range | null, increment: number, editable: boolean, edit: Edit): Record<string, Partial<Command>> {
  const out: Record<string, Partial<Command>> = {};
  for (const [name, dir, times] of [['Up', 1, 1], ['Down', -1, 1], ['Up5', 1, 5], ['Down5', -1, 5]] as const) {
    out[`text.key.wordSpace${name}`] = { fallThrough: true, run: editable && range && range.end > range.start ? () => void stepWordSpaces(range, dir * times * increment, edit) : undefined };
  }
  return out;
}
