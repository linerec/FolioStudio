import type { TextProperties } from '../engine';
import { tr } from '../i18n';
import type { Command } from './commands';
import type { TextSelectionState } from './textedit';

type Entry = { id: string; key: string; value: string | boolean; off?: string | boolean; tags: string[] };
const TOGGLES = [
  ['discretionaryLigatures', 'otfDiscretionaryLigature', 'dlig'],
  ['fractions', 'otfFraction', 'frac'],
  ['ordinal', 'otfOrdinal', 'ordn'],
  ['swash', 'otfSwash', 'swsh'],
  ['titlingAlternates', 'otfTitling', 'titl'],
  ['contextualAlternates', 'otfContextualAlternate', 'calt'],
  ['slashedZero', 'otfSlashedZero', 'zero'],
] as const;
const FORMS = [
  ['generalForm', 'NONE', ''], ['automaticForm', 'CALCULATE', ''],
  ['initialForm', 'INITIAL', 'init'], ['medialForm', 'MEDIAL', 'medi'],
  ['finalForm', 'FINAL', 'fina'], ['isolatedForm', 'ISOLATED', 'isol'],
] as const;
const FIGURES = [
  ['tabularLining', 'TABULAR_LINING', ['tnum', 'lnum']],
  ['proportionalOldstyle', 'PROPORTIONAL_OLDSTYLE', ['pnum', 'onum']],
  ['proportionalLining', 'PROPORTIONAL_LINING', ['pnum', 'lnum']],
  ['tabularOldstyle', 'TABULAR_OLDSTYLE', ['tnum', 'onum']],
  ['defaultFigureStyle', 'DEFAULT_VALUE', []],
] as const;
const ENTRIES: Entry[] = [
  ...TOGGLES.map(([id, key, tag]) => ({ id, key, value: true, off: false, tags: [tag] })),
  { id: 'allSmallCaps', key: 'capitalization', value: 'CAP_TO_SMALL_CAP', off: 'NORMAL', tags: ['smcp', 'c2sc'] },
  { id: 'superscriptSuperior', key: 'position', value: 'OT_SUPERSCRIPT', off: 'NORMAL', tags: ['sups'] },
  { id: 'subscriptInferior', key: 'position', value: 'OT_SUBSCRIPT', off: 'NORMAL', tags: ['subs'] },
  { id: 'numerator', key: 'position', value: 'OT_NUMERATOR', off: 'NORMAL', tags: ['numr'] },
  { id: 'denominator', key: 'position', value: 'OT_DENOMINATOR', off: 'NORMAL', tags: ['dnom'] },
  ...Array.from({ length: 20 }, (_, i) => ({ id: `stylisticSets.set${i + 1}`, key: `otfStylisticSet${i + 1}`, value: true, off: false, tags: [`ss${String(i + 1).padStart(2, '0')}`] })),
  ...FORMS.map(([id, value, tag]) => ({ id: `positionalForms.${id}`, key: 'positionalForm', value, tags: tag ? [tag] : [] })),
  ...FIGURES.map(([id, value, tags]) => ({ id, key: 'otfFigureStyle', value, tags: [...tags] })),
];

export function openTypeMenu(editable: boolean, selection: TextSelectionState | null, apply: (props: TextProperties) => void): Record<string, Partial<Command>> {
  const out: Record<string, Partial<Command>> = {};
  const supported = new Set(selection?.features ?? []);
  for (const entry of ENTRIES) {
    const checked = selection?.format[entry.key] === entry.value;
    const available = entry.tags.every(tag => supported.has(tag));
    out[`id.panel.character.opentype.${entry.id}`] = {
      checked,
      run: editable && selection && available ? () => apply({ [entry.key]: checked && entry.off !== undefined ? entry.off : entry.value }) : undefined,
      why: selection && !available ? tr('선택한 글꼴이 이 OpenType 기능을 지원하지 않습니다.') : undefined,
    };
  }
  return out;
}
