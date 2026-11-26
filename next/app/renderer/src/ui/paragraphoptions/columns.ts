import { t as tr } from '../../i18n';
import type { StyleField } from '../textstyles/fields';

export const columnFields = (): StyleField[] => [
  { key: 'spanColumnType', label: tr('단락 레이아웃'), page: 'spanColumns', choices: [
    { value: 'SINGLE_COLUMN', label: tr('한 단') },
    { value: 'SPAN_COLUMNS', label: tr('단 걸치기') },
    { value: 'SPLIT_COLUMNS', label: tr('단 분할') },
  ] },
  { key: 'spanSplitColumnCount', label: tr('걸치거나 분할할 단 수'), page: 'spanColumns', numericChoice: true, choices: [
    { value: 'ALL', label: tr('모든 단') }, { value: '2', label: tr('2단') },
    { value: '3', label: tr('3단') }, { value: '4', label: tr('4단') },
  ] },
  { key: 'spanColumnMinSpaceBefore', label: tr('걸침 위 간격 (pt)'), page: 'spanColumns' },
  { key: 'spanColumnMinSpaceAfter', label: tr('걸침 아래 간격 (pt)'), page: 'spanColumns' },
  { key: 'splitColumnInsideGutter', label: tr('분할 안쪽 간격 (pt)'), page: 'spanColumns' },
  { key: 'splitColumnOutsideGutter', label: tr('분할 바깥쪽 여백 (pt)'), page: 'spanColumns' },
];
