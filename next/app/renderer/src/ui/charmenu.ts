// The Character panel menu's toggles on the selected text (oracle text-format, text-format-char):
// All Caps, Small Caps, Superscript, Subscript, Underline, Strikethrough, Ligatures, No Break and
// the OpenType superscript and subscript forms. Choosing a form that the whole selection has turns
// it off again (back to Normal); the item is checked while the selection has it.
import type { TextProperties } from '../engine';
import { tr } from '../i18n';
import { openTypeMenu } from './opentype';
import type { Command } from './commands';
import type { TextSelectionState } from './textedit';

type Toggle = { id: string; key: string; on: string | boolean; off: string | boolean; has: (v: unknown) => boolean };

const TOGGLES: Toggle[] = [
  { id: 'lowercase', key: 'capitalization', on: 'LOWER_CASE', off: 'NORMAL', has: (v) => v === 'LOWER_CASE' },
  { id: 'allCaps', key: 'capitalization', on: 'ALL_CAPS', off: 'NORMAL', has: (v) => v === 'ALL_CAPS' },
  { id: 'smallCaps', key: 'capitalization', on: 'SMALL_CAPS', off: 'NORMAL', has: (v) => v === 'SMALL_CAPS' },
  { id: 'superscript', key: 'position', on: 'SUPERSCRIPT', off: 'NORMAL', has: (v) => v === 'SUPERSCRIPT' },
  { id: 'subscript', key: 'position', on: 'SUBSCRIPT', off: 'NORMAL', has: (v) => v === 'SUBSCRIPT' },
  { id: 'underline', key: 'underline', on: true, off: false, has: (v) => v === true },
  { id: 'strikethrough', key: 'strikeThru', on: true, off: false, has: (v) => v === true },
  // ligatures are on unless turned off
  { id: 'ligatures', key: 'ligatures', on: true, off: false, has: (v) => v !== false },
  { id: 'noBreak', key: 'noBreak', on: true, off: false, has: (v) => v === true },
];

export function characterMenu({ editable, selection, apply, list, restart }: {
  editable: boolean; selection: TextSelectionState | null; apply: (props: TextProperties) => void; list?: (numbers: boolean) => void; restart?: () => void;
}): Record<string, Partial<Command>> {
  const out: Record<string, Partial<Command>> = {};
  // Restart/Continue Numbering: for the numbered paragraph (the engine refuses a selection of several); reads Continue once it restarts
  const numbered = selection?.format.bulletsAndNumberingListType === 'NUMBERED_LIST';
  out['id.type.bulletedAndNumberedLists.restartContinueNumbering'] = {
    run: editable && numbered && restart ? restart : undefined,
    label: numbered && selection?.format.numberingContinue === false ? tr('번호 계속 매기기') : numbered ? tr('번호 다시 매기기') : undefined,
  };
  // Type ▸ Bulleted & Numbered Lists (oracle text-format-lists): checked while all the paragraphs are that list
  for (const [id, numbers, kind] of [['applyRemoveBullets', false, 'BULLET_LIST'], ['applyRemoveNumbers', true, 'NUMBERED_LIST']] as const) {
    out[`id.type.bulletedAndNumberedLists.${id}`] = { run: editable && selection && list ? () => list(numbers) : undefined, checked: selection?.format.bulletsAndNumberingListType === kind };
  }
  for (const t of TOGGLES) {
    // a mixed selection (null) turns the form on
    const has = !!selection && t.has(selection.format[t.key]);
    out[`id.panel.character.${t.id}`] = { run: editable && selection ? () => apply({ [t.key]: has ? t.off : t.on }) : undefined, checked: has };
  }
  return { ...out, ...openTypeMenu(editable, selection, apply) };
}
