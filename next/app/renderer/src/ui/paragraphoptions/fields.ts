import { t as tr } from '../../i18n';
import type { StyleField } from '../textstyles/fields';
import { columnFields } from './columns';

export const justificationFields = (): StyleField[] => [
  { key: 'minimumWordSpacing', label: tr('단어 간격 최소 (%)'), page: 'justification' },
  { key: 'desiredWordSpacing', label: tr('단어 간격 원하는 값 (%)'), page: 'justification' },
  { key: 'maximumWordSpacing', label: tr('단어 간격 최대 (%)'), page: 'justification' },
  { key: 'minimumLetterSpacing', label: tr('문자 간격 최소 (%)'), page: 'justification' },
  { key: 'desiredLetterSpacing', label: tr('문자 간격 원하는 값 (%)'), page: 'justification' },
  { key: 'maximumLetterSpacing', label: tr('문자 간격 최대 (%)'), page: 'justification' },
  { key: 'minimumGlyphScaling', label: tr('글리프 비율 최소 (%)'), page: 'justification' },
  { key: 'desiredGlyphScaling', label: tr('글리프 비율 원하는 값 (%)'), page: 'justification' },
  { key: 'maximumGlyphScaling', label: tr('글리프 비율 최대 (%)'), page: 'justification' },
  { key: 'singleWordJustification', label: tr('한 단어 정렬'), page: 'justification', choices: [
    { value: 'LEFT_ALIGN', label: tr('왼쪽') }, { value: 'CENTER_ALIGN', label: tr('가운데') },
    { value: 'RIGHT_ALIGN', label: tr('오른쪽') }, { value: 'FULLY_JUSTIFIED', label: tr('모든 줄 균등') },
  ] },
];

export const hyphenationFields = (): StyleField[] => [
  { key: 'hyphenation', label: tr('하이픈 사용'), page: 'hyphenation', boolean: true, choices: toggle() },
  { key: 'hyphenateWordsLongerThan', label: tr('최소 단어 길이'), page: 'hyphenation' },
  { key: 'hyphenateAfterFirst', label: tr('앞쪽 최소 문자 수'), page: 'hyphenation' },
  { key: 'hyphenateBeforeLast', label: tr('뒤쪽 최소 문자 수'), page: 'hyphenation' },
  { key: 'hyphenateLadderLimit', label: tr('연속 하이픈 행 수 (0은 제한 없음)'), page: 'hyphenation' },
  { key: 'hyphenateCapitalizedWords', label: tr('대문자로 시작하는 단어'), page: 'hyphenation', boolean: true, choices: toggle() },
  { key: 'hyphenateLastWord', label: tr('마지막 단어에 하이픈 허용'), page: 'hyphenation', boolean: true, choices: toggle() },
  { key: 'hyphenationZone', label: tr('하이픈 영역 (pt)'), page: 'hyphenation' },
  { key: 'hyphenWeight', label: tr('균형 가중치 (0 좋은 간격 ~ 10 하이픈 줄이기)'), page: 'hyphenation' },
  { key: 'hyphenateAcrossColumns', label: tr('단 경계에서 하이픈 허용'), page: 'hyphenation', boolean: true, choices: toggle() },
];
const toggle = () => [{ value: 'true', label: tr('켜기') }, { value: 'false', label: tr('끄기') }];
export const keepFields = (): StyleField[] => [
  { key: 'keepWithNext', label: tr('다음 줄과 유지 (0~5)'), page: 'keepOptions' },
  { key: 'keepLinesTogether', label: tr('행 분리 금지'), page: 'keepOptions', boolean: true, choices: toggle() },
  { key: 'keepAllLinesTogether', label: tr('단락 전체 유지'), page: 'keepOptions', boolean: true, choices: toggle() },
  { key: 'keepFirstLines', label: tr('처음 유지할 줄 수 (1~50)'), page: 'keepOptions' },
  { key: 'keepLastLines', label: tr('끝에 유지할 줄 수 (1~50)'), page: 'keepOptions' },
  { key: 'startParagraph', label: tr('시작 위치'), page: 'keepOptions', choices: [
    { value: 'ANYWHERE', label: tr('아무 위치') }, { value: 'NEXT_COLUMN', label: tr('다음 단') },
    { value: 'NEXT_FRAME', label: tr('다음 프레임') }, { value: 'NEXT_PAGE', label: tr('다음 페이지') },
  ] },
];
/** `note`: a grey line on what Folio does on this page (never a comparison with InDesign: user decision 2026-11-21; the differences are `approximate` in rules/interop.tsv). */
export const paragraphPages = (): { id: 'spanColumns' | 'justification' | 'keepOptions' | 'hyphenation'; label: string; fields: StyleField[]; note?: string }[] => [
  { id: 'spanColumns' as const, label: tr('단 걸치기'), fields: columnFields() },
  { id: 'justification' as const, label: tr('균등 배치'), fields: justificationFields() },
  { id: 'keepOptions' as const, label: tr('텍스트 분리 금지 옵션'), fields: keepFields() },
  { id: 'hyphenation' as const, label: tr('하이픈 넣기'), fields: hyphenationFields(), note: tr('하이픈 사전은 영어·독일어·프랑스어·스페인어가 있습니다. 다른 언어의 글에는 하이픈을 넣지 않습니다.') },
];
