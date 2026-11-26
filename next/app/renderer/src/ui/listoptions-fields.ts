import { t as tr } from '../i18n';
import type { StyleChoice, StyleField } from './textstyles/fields';

export const listOptionFields = (styles: StyleChoice[] = [], lists: StyleChoice[] = []): StyleField[] => {
  const toggle = [{value:'true',label:tr('켜기')},{value:'false',label:tr('끄기')}];
  const alignment = [{value:'LEFT_ALIGN',label:tr('왼쪽')},{value:'CENTER_ALIGN',label:tr('가운데')},{value:'RIGHT_ALIGN',label:tr('오른쪽')}];
  return [
    {key:'appliedNumberingList',label:tr('정의된 목록'),page:'lists',choices:lists},
    {key:'bulletsAndNumberingListType',label:tr('목록 종류'),page:'lists',choices:[{value:'NO_LIST',label:tr('없음')},{value:'BULLET_LIST',label:tr('글머리 기호')},{value:'NUMBERED_LIST',label:tr('번호 매기기')}]},
    {key:'bulletChar',label:tr('기호 문자'),page:'lists',text:true},
    {key:'bulletsTextAfter',label:tr('기호 뒤 문자열'),page:'lists',text:true},
    {key:'numberingFormat',label:tr('번호 형식'),page:'lists',choices:['1, 2, 3, 4...','A, B, C, D...','a, b, c, d...','I, II, III, IV...','i, ii, iii, iv...'].map(value=>({value,label:value}))},
    {key:'numberingExpression',label:tr('번호 앞뒤 문자열'),page:'lists',text:true},
    {key:'numberingStartAt',label:tr('시작 번호'),page:'lists'},
    {key:'numberingContinue',label:tr('이전 번호 계속'),page:'lists',boolean:true,choices:toggle},
    {key:'numberingLevel',label:tr('목록 수준'),page:'lists'},
    {key:'numberingApplyRestartPolicy',label:tr('상위 수준 뒤에 다시 시작'),page:'lists',boolean:true,choices:toggle},
    {key:'bulletsAlignment',label:tr('기호 정렬'),page:'lists',choices:alignment},
    {key:'numberingAlignment',label:tr('번호 정렬'),page:'lists',choices:alignment},
    {key:'bulletsCharacterStyle',label:tr('기호 문자 스타일'),page:'lists',choices:styles},
    {key:'numberingCharacterStyle',label:tr('번호 문자 스타일'),page:'lists',choices:styles},
  ];
};
