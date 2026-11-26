import { t as tr } from '../i18n';
import type { TextProperties } from '../engine';
import type { Command } from './commands';
import type { TextSelectionState } from './textedit';
import type { TextFormatting } from './textformat';

export const composerChoices = () => [
  { id: 'paragraphComposer', value: 'HL Composer', label: tr('단락 컴포저') },
  { id: 'singleLineComposer', value: 'HL Single', label: tr('단일 행 컴포저') },
  { id: 'worldReadyParagraphComposer', value: 'HL Composer Optyca', label: tr('World-Ready 단락 컴포저') },
  { id: 'worldReadySingleLineComposer', value: 'HL Single Optyca', label: tr('World-Ready 단일 행 컴포저') },
];

export function paragraphComposerMenu(editable: boolean, selection: TextSelectionState | null, apply: (props: TextProperties) => void): Record<string, Partial<Command>> {
  return { ...Object.fromEntries(composerChoices().map(c => [`id.panel.paragraph.${c.id}`, {
    run: editable && selection ? () => apply({ composer: c.value }) : undefined,
    checked: selection?.format.composer === c.value,
    // measured against InDesign (print comparison of the Latin, Korean and Arabic frames): all but World-Ready Paragraph, whose mixed English/Arabic and Arabic lines InDesign orders and breaks differently (rules/spancols-wraptail.ko.md): it works (the screen does not explain the difference, user decision 2026-11-21; it is `approximate` in rules/interop.tsv) and on the Composer field, `approximate` in rules/interop.tsv
  }])), 'id.panel.paragraph.onlyAlignFirstLineToGrid': {
    run: editable && selection ? () => apply({ gridAlignFirstLineOnly: selection.format.gridAlignFirstLineOnly !== true }) : undefined,
    checked: selection?.format.gridAlignFirstLineOnly === true,
  }, 'id.panel.paragraph.balanceRaggedLines': {
    run: editable && selection ? () => apply({ balanceRaggedLines: selection.format.balanceRaggedLines !== true }) : undefined,
    checked: selection?.format.balanceRaggedLines === true,
  },
  // Paragraph panel menu ▸ Auto Tate-Chu-Yoko (vtext-c2): the paragraph's autoTcy digits (0: off) and autoTcyIncludeRoman, vertical text
  ...Object.fromEntries([0, 2, 3, 4].map(n => [n ? `para.autoTcy${n}` : 'para.autoTcyOff', {
    run: editable && selection ? () => apply({ autoTcy: n }) : undefined,
    checked: Number(selection?.format.autoTcy ?? -1) === n,
  }])),
  'para.autoTcyRoman': {
    run: editable && selection ? () => apply({ autoTcyIncludeRoman: selection.format.autoTcyIncludeRoman !== true }) : undefined,
    checked: selection?.format.autoTcyIncludeRoman === true,
  } };
}

export function ComposerField({ control }: { control: TextFormatting }) {
  const value = control.selection?.format.composer;
  return <><label className="tf-field">{tr('컴포저')}<select aria-label={tr('컴포저')} disabled={!control.selection}
    value={String(value ?? '')} onChange={e => control.apply({ composer: e.target.value })}>
    <option value="" disabled>{value === null ? tr('혼합') : ''}</option>
    {composerChoices().map(c => <option key={c.value} value={c.value}>{c.label}</option>)}
  </select></label>
    <label className="tf-field"><input type="checkbox" disabled={!control.selection} checked={control.selection?.format.alignToBaseline === true}
      onChange={e => control.apply({ alignToBaseline: e.target.checked })}/>{tr('기준선 격자에 정렬')}</label>
  </>;
}
