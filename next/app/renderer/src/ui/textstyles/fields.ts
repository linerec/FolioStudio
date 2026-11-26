import { t as tr } from '../../i18n';
import { paragraphPages } from '../paragraphoptions/fields';
import { composerChoices } from '../parcomp';
import { listOptionFields } from '../listoptions-fields';

import { decorationFields, decorationPages, type DecorationPage } from './decorations';
export type StylePage = 'general' | 'character' | 'paragraph' | 'justification' | 'hyphenation' | 'keepOptions' | 'lists' | 'spanColumns' | 'nestedStyles' | 'grepStyles' | DecorationPage;
export type StyleChoice = { value: string; label: string; disabled?: boolean };
export type StyleField = { key: string; label: string; page: StylePage; text?: boolean; boolean?: boolean; choices?: StyleChoice[]; numericChoice?: boolean; unfinished?: boolean };
const toggle = () => [{ value: 'true', label: tr('켜기') }, { value: 'false', label: tr('끄기') }];
export const styleFields = (characterStyles: StyleChoice[] = [], lists: StyleChoice[] = [], swatches:{id:string;name:string}[] = []): StyleField[] => [
  { key: 'appliedFont', label: tr('글꼴'), page: 'character', text: true },
  { key: 'fontStyle', label: tr('글꼴 스타일'), page: 'character', text: true },
  { key: 'pointSize', label: tr('크기'), page: 'character' },
  { key: 'leading', label: tr('행간'), page: 'character' },
  { key: 'tracking', label: tr('자간'), page: 'character' },
  { key: 'kerningMethod', label: tr('커닝 방식'), page: 'character', unfinished: true, choices: [
    { value: 'Metrics', label: tr('메트릭') }, { value: 'Optical', label: tr('광학 (미완성)'), disabled: true },
  ] },
  { key: 'capitalization', label: tr('대소문자'), page: 'character', unfinished: true, choices: [
    { value: 'NORMAL', label: tr('보통') }, { value: 'ALL_CAPS', label: tr('모두 대문자') }, { value: 'SMALL_CAPS', label: tr('작은 대문자') },
  ] },
  { key: 'position', label: tr('문자 위치'), page: 'character', choices: [
    { value: 'NORMAL', label: tr('보통') }, { value: 'SUPERSCRIPT', label: tr('위 첨자') }, { value: 'SUBSCRIPT', label: tr('아래 첨자') },
  ] },
  { key: 'underline', label: tr('밑줄'), page: 'character', boolean: true, choices: toggle() },
  { key: 'strikeThru', label: tr('취소선'), page: 'character', boolean: true, unfinished: true, choices: toggle() },
  { key: 'ligatures', label: tr('합자'), page: 'character', boolean: true, choices: toggle() },
  { key: 'noBreak', label: tr('줄 바꿈 안 함'), page: 'character', boolean: true, choices: toggle() },
  { key: 'leftIndent', label: tr('왼쪽 들여쓰기'), page: 'paragraph' },
  { key: 'rightIndent', label: tr('오른쪽 들여쓰기'), page: 'paragraph' },
  { key: 'firstLineIndent', label: tr('첫 줄 들여쓰기'), page: 'paragraph' },
  { key: 'spaceBefore', label: tr('앞 간격'), page: 'paragraph' },
  { key: 'spaceAfter', label: tr('뒤 간격'), page: 'paragraph' },
  ...paragraphPages().flatMap(p => p.fields),
  ...listOptionFields(characterStyles,lists),
  ...decorationFields(swatches),
  { key: 'balanceRaggedLines', label: tr('불규칙한 행 균형 맞추기'), page: 'paragraph', boolean: true, choices: toggle() },
  { key: 'composer', label: tr('컴포저'), page: 'paragraph', choices: composerChoices(), unfinished: true },
  { key: 'alignToBaseline', label: tr('기준선 격자에 정렬'), page: 'paragraph', boolean: true, choices: toggle(), unfinished: true },
  { key: 'gridAlignFirstLineOnly', label: tr('첫 줄만 격자에 정렬'), page: 'paragraph', boolean: true, choices: toggle() },
];
export const stylePages = () => [
  { id: 'general' as const, label: tr('일반') },
  { id: 'character' as const, label: tr('기본 문자 형식') },
  { id: 'paragraph' as const, label: tr('들여쓰기와 간격') },
  ...paragraphPages().map(({ id, label }) => ({ id, label })),
  { id: 'lists' as const, label: tr('글머리 기호 및 번호 매기기') },
  ...decorationPages(),
  { id: 'nestedStyles' as const, label: tr('중첩 스타일') },
  { id: 'grepStyles' as const, label: tr('GREP 스타일') },
];
export const styleAlignments = () => [
  ['LEFT_ALIGN', tr('왼쪽')], ['CENTER_ALIGN', tr('가운데')], ['RIGHT_ALIGN', tr('오른쪽')],
  ['LEFT_JUSTIFIED', tr('양쪽')], ['FULLY_JUSTIFIED', tr('모든 줄 균등')],
];
