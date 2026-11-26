import { t as tr } from '../../i18n';
import type { StyleField } from './fields';
import { STROKE_TYPES } from '../stroke';

export type DecorationPage = 'ruleAbove' | 'ruleBelow' | 'shading' | 'border';
export const ruleDecorationPages: DecorationPage[] = ['ruleAbove', 'ruleBelow'];
export const boxDecorationPages: DecorationPage[] = ['shading', 'border'];
export const decorationPages = () => [
  {id:'ruleAbove' as const,label:tr('위 경계선')},
  {id:'ruleBelow' as const,label:tr('아래 경계선')},
  {id:'shading' as const,label:tr('단락 음영')},
  {id:'border' as const,label:tr('단락 테두리')},
];
const toggle = () => [{value:'true',label:tr('켜기')},{value:'false',label:tr('끄기')}];
const widthChoices = () => [{value:'COLUMN_WIDTH',label:tr('단 너비')},{value:'TEXT_WIDTH',label:tr('텍스트 너비')}];
const topOrigins = () => [{value:'ASCENT_TOP_ORIGIN',label:tr('글자 위(어센트)')},{value:'BASELINE_TOP_ORIGIN',label:tr('기준선')},{value:'LEADING_TOP_ORIGIN',label:tr('행간 위')}];
const bottomOrigins = () => [{value:'DESCENT_BOTTOM_ORIGIN',label:tr('글자 아래(디센트)')},{value:'BASELINE_BOTTOM_ORIGIN',label:tr('기준선')}];
// every built-in stroke style of the Stroke panel (the layout draws them all, with their gap colour: text/layout/strokes.rs)
const ruleTypes = () => STROKE_TYPES.map(([value,label])=>({value,label:label()}));
export function decorationFields(swatches:{id:string;name:string}[]=[]):StyleField[] {
  const colors = swatches.map(s=>({value:s.id,label:s.name}));
  const rule = ({id:page}:{id:'ruleAbove'|'ruleBelow'}):StyleField[] => [
    {key:page,label:tr('경계선 사용'),page,boolean:true,choices:toggle()},
    {key:page+'LineWeight',label:tr('굵기'),page},
    {key:page+'Type',label:tr('선 종류'),page,choices:ruleTypes()},
    {key:page+'Overprint',label:tr('선 중복 인쇄'),page,boolean:true,choices:toggle()},
    {key:page+'GapColor',label:tr('간격 색'),page,choices:colors},
    {key:page+'GapTint',label:tr('간격 색조'),page},
    {key:page+'GapOverprint',label:tr('간격 중복 인쇄'),page,boolean:true,choices:toggle()},
    {key:page+'Color',label:tr('색상'),page,choices:[{value:'Text Color',label:tr('텍스트 색')},...colors]},
    {key:page+'Tint',label:tr('색조'),page},
    {key:page+'Width',label:tr('폭'),page,choices:widthChoices()},
    {key:page+'Offset',label:tr('오프셋'),page},
    {key:page+'LeftIndent',label:tr('왼쪽 들여쓰기'),page},
    {key:page+'RightIndent',label:tr('오른쪽 들여쓰기'),page},
    ...(page==='ruleAbove'?[{key:'keepRuleAboveInFrame',label:tr('프레임 안에 유지'),page,boolean:true,choices:toggle()}]:[]),
  ];
  const shading:StyleField[] = [
    {key:'paragraphShadingOn',label:tr('음영 사용'),page:'shading',boolean:true,choices:toggle()},
    {key:'paragraphShadingColor',label:tr('색상'),page:'shading',choices:colors},
    {key:'paragraphShadingTint',label:tr('색조'),page:'shading'},
    {key:'paragraphShadingWidth',label:tr('폭'),page:'shading',choices:widthChoices()},
    {key:'paragraphShadingTopOrigin',label:tr('위 기준'),page:'shading',choices:topOrigins()},
    {key:'paragraphShadingBottomOrigin',label:tr('아래 기준'),page:'shading',choices:bottomOrigins()},
    {key:'paragraphShadingTopOffset',label:tr('위 오프셋'),page:'shading'},
    {key:'paragraphShadingLeftOffset',label:tr('왼쪽 오프셋'),page:'shading'},
    {key:'paragraphShadingBottomOffset',label:tr('아래 오프셋'),page:'shading'},
    {key:'paragraphShadingRightOffset',label:tr('오른쪽 오프셋'),page:'shading'},
  ];
  const border:StyleField[] = [
    {key:'paragraphBorderOn',label:tr('테두리 사용'),page:'border',boolean:true,choices:toggle()},
    {key:'paragraphBorderColor',label:tr('색상'),page:'border',choices:colors},
    {key:'paragraphBorderTint',label:tr('색조'),page:'border'},
    {key:'paragraphBorderType',label:tr('선 종류'),page:'border',choices:ruleTypes()},
    {key:'paragraphBorderOverprint',label:tr('선 중복 인쇄'),page:'border',boolean:true,choices:toggle()},
    {key:'paragraphBorderGapColor',label:tr('간격 색'),page:'border',choices:colors},
    {key:'paragraphBorderGapTint',label:tr('간격 색조'),page:'border'},
    {key:'paragraphBorderGapOverprint',label:tr('간격 중복 인쇄'),page:'border',boolean:true,choices:toggle()},
    {key:'paragraphBorderWidth',label:tr('폭'),page:'border',choices:widthChoices()},
    {key:'paragraphBorderTopOrigin',label:tr('위 기준'),page:'border',choices:topOrigins()},
    {key:'paragraphBorderBottomOrigin',label:tr('아래 기준'),page:'border',choices:bottomOrigins()},
    {key:'paragraphBorderTopLineWeight',label:tr('위 굵기'),page:'border'},
    {key:'paragraphBorderLeftLineWeight',label:tr('왼쪽 굵기'),page:'border'},
    {key:'paragraphBorderBottomLineWeight',label:tr('아래 굵기'),page:'border'},
    {key:'paragraphBorderRightLineWeight',label:tr('오른쪽 굵기'),page:'border'},
    {key:'paragraphBorderTopOffset',label:tr('위 오프셋'),page:'border'},
    {key:'paragraphBorderLeftOffset',label:tr('왼쪽 오프셋'),page:'border'},
    {key:'paragraphBorderBottomOffset',label:tr('아래 오프셋'),page:'border'},
    {key:'paragraphBorderRightOffset',label:tr('오른쪽 오프셋'),page:'border'},
    {key:'mergeConsecutiveParaBorders',label:tr('같은 테두리의 연속 단락 병합'),page:'border',boolean:true,choices:toggle()},
  ];
  return [...rule({id:'ruleAbove'}),...rule({id:'ruleBelow'}),...shading,...border];
}
