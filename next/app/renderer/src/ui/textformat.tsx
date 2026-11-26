// Shared controls: the control bar and text panels edit the same live text selection.
import { ControlBarMenu, HiddenGroups, useControlBar } from './controlbar';
import { useRef } from 'react';
import type { TextProperties } from '../engine';
import type { DocumentInfo } from '../inspect';
import type { TextSelectionState } from './textedit';
import { clearStyleOverrides } from './keys';
import { fontFamilyProperties } from './fontfamily';
import { ComposerField } from './parcomp';
import { wholeNumber, DROP_CAP_LIMITS } from './drop-caps';
import { t as tr } from '../i18n';
import { PreviewNumberField } from './controls/previewnumberfield';
import type { PreviewPort } from './controls/usePreviewEdit';

export type TextFormatting = {
  opticalMarginEnabled?:boolean;
  selection:TextSelectionState|null;
  info:DocumentInfo|null;
  apply:(props:TextProperties)=>void|Promise<void>;
  /** The engine's preview session for the number fields (textedit.tsx `preview`); none without a text selection. */
  preview?:PreviewPort|null;
  style:(kind:'paragraph'|'character',id:string,clear:boolean)=>void;
  insertGlyph:(text:string,props:TextProperties)=>void;
  insertGlyphById:(family:string,style:string,glyph:number)=>void;
};
/** Each number field of the panels: its range and one arrow press. */
const FIELDS: Record<string, { min: number; max: number; step: number; whole?: boolean; suffix?: string }> = {
  pointSize: { min: 0.1, max: 1296, step: 1 }, leading: { min: 0, max: 5000, step: 1 }, tracking: { min: -1000, max: 10000, step: 1 },
  baselineShift: { min: -5000, max: 5000, step: 1 }, kerningValue: { min: -1000, max: 10000, step: 1 },
  leftIndent: { min: -5000, max: 5000, step: 1 }, rightIndent: { min: -5000, max: 5000, step: 1 }, firstLineIndent: { min: -5000, max: 5000, step: 1 },
  lastLineIndent: { min: -5000, max: 5000, step: 1 },
  // Character panel scale and skew boxes: InDesign's ranges (25% to 400%, -85 to 85 degrees), shown with their unit
  horizontalScale: { min: 25, max: 400, step: 1, suffix: '%' }, verticalScale: { min: 25, max: 400, step: 1, suffix: '%' }, skew: { min: -85, max: 85, step: 1, suffix: '°' },
  // Character Rotation (-360 to 360 degrees, counter-clockwise around the em box centre; vtext-c1)
  characterRotation: { min: -360, max: 360, step: 1, suffix: '°' },
  spaceBefore: { min: 0, max: 5000, step: 1 }, spaceAfter: { min: 0, max: 5000, step: 1 },
  dropCapLines: { min: 0, max: DROP_CAP_LIMITS.lines, step: 1, whole: true }, dropCapCharacters: { min: 0, max: DROP_CAP_LIMITS.characters, step: 1, whole: true },
};
/** What a leading box takes for "auto": the word the engine uses and the screen's own word for it. */
const autoWords=()=>['auto',tr('자동').toLowerCase()];
/**
 * A number field of the character and paragraph panels: the shared field with the engine's preview session (a value tried by an arrow, ▲▼,
 * the wheel or a drag of the name shows in the text at once; Esc gives it back exactly; letting go makes it one undo step). A typed value
 * is the ordinary format edit. The leading also takes "auto"; the drop cap lines and characters are whole numbers (`whole`: the engine refuses
 * anything else too).
 */
function NumberField({control,name,label}: {control:TextFormatting;name:string;label:string}) {
  const s=control.selection, value=s?.format[name], {min,max,step,whole,suffix}=FIELDS[name];
  const auto=name==='leading';
  return <label className="tf-field">{label}<PreviewNumberField value={typeof value==='number'?value:null} label={label} undoLabel={tr('글 서식')} min={min} max={max} step={step} decimals={whole?0:4} width={64} suffix={suffix}
    disabled={!s} placeholder={value===null?tr('혼합 값'):undefined} isolateKeys port={control.preview ?? null}
    anchor={auto&&typeof s?.format.pointSize==='number'?s.format.pointSize*1.2:undefined}
    word={auto?{text:'auto',words:autoWords(),active:value==='auto',onChoose:()=>control.apply({leading:'auto'})}:undefined}
    parse={t=>{const raw=t.trim().replace(/\s*(pt|%|°)$/i,'').replace(',','.');if(!raw)return null;if(whole)return wholeNumber(raw,max);const n=Number(raw);return Number.isFinite(n)&&n>=min&&n<=max?n:null;}}
    build={v=>s?[{op:'textFormat',story:s.story,start:s.start,end:s.end,props:{[name]:v}}]:[]}
    edit={(_label,commands)=>{const c=commands[0];if(c?.op==='textFormat')return control.apply(c.props);}}/></label>;
}
export function StyleSelect({control,kind}:{control:TextFormatting;kind:'paragraph'|'character'}) {
  const alt=useRef(false),key=kind==='paragraph'?'paragraphStyle':'characterStyle';
  const list=kind==='paragraph'?control.info?.paragraphStyles:control.info?.characterStyles;
  return <label className="tf-field">{kind==='paragraph'?tr("단락 스타일"):tr("문자 스타일")}<select aria-label={kind==='paragraph'?tr("단락 스타일"):tr("문자 스타일")} disabled={!control.selection}
    value={String(control.selection?.format[key]??'')} title={tr("Alt를 누르고 적용하면 재정의도 지웁니다.")}
    onPointerDown={e=>{alt.current=clearStyleOverrides(e);}} onKeyDown={e=>{alt.current=clearStyleOverrides(e);}}
    onChange={e=>control.style(kind,e.target.value,alt.current)}>
    <option value="">{control.selection?tr("혼합"):''}</option>{list?.map(s=><option key={s.id} value={s.id}>{[...s.group,s.name].join(' / ')}</option>)}
  </select></label>;
}
/** The Language box of the Character panel: the document's languages (appliedLanguage); blank for a mixed selection. */
function LanguageField({control}:{control:TextFormatting}) {
  const s=control.selection, value=s?.format.appliedLanguage, choices=control.info?.languageChoices??[];
  const current=typeof value==='string'?value:'';
  return <label className="tf-field">{tr('언어')}<select aria-label={tr('언어')} disabled={!s} value={current} onChange={e=>{if(e.target.value)control.apply({appliedLanguage:e.target.value});}}>
    <option value="">{value===null?tr('혼합'):''}</option>{current&&!choices.some(c=>c.name===current)&&<option>{current}</option>}{choices.map(c=><option key={c.name} value={c.name}>{c.label}</option>)}</select></label>;
}
export function CharacterFields({control,options=true}:{control:TextFormatting;options?:boolean}) {
  const s=control.selection,f=s?.format??{},fonts=s?.fonts??{},family=String(f.appliedFont??''),fontStyle=String(f.fontStyle??'');
  const toggle=(name:string,value:string|boolean)=>control.apply({[name]:f[name]===value?(typeof value==='boolean'?false:'NORMAL'):value});
  return <>
    <div className="tf-group" data-cp="char.font">
      <label className="tf-field">{tr("글꼴")}<select aria-label={tr("글꼴")} disabled={!s} value={family} onChange={e=>{
        control.apply(fontFamilyProperties(fonts,e.target.value,fontStyle));
      }}><option value="">{f.appliedFont===null?tr("혼합"):''}</option>{family&&!fonts[family]&&<option>{family}</option>}{Object.keys(fonts).map(f=><option key={f}>{f}</option>)}</select></label>
      <label className="tf-field">{tr("글꼴 스타일")}<select aria-label={tr("글꼴 스타일")} disabled={!s||!family} value={fontStyle} onChange={e=>control.apply({fontStyle:e.target.value})}>
        <option value="">{f.fontStyle===null?tr("혼합"):''}</option>{(fonts[family]??[fontStyle]).map(f=><option key={f}>{f}</option>)}</select></label>
    </div>
    <div className="tf-group" data-cp="char.size"><NumberField control={control} name="pointSize" label={tr("글꼴 크기")}/><NumberField control={control} name="leading" label={tr("행간")}/></div>
    {options && <div className="tf-group" data-cp="char.tracking"><NumberField control={control} name="tracking" label={tr("자간")}/><NumberField control={control} name="baselineShift" label={tr("기준선 이동")}/></div>}
    {options && <div className="tf-group" data-cp="char.scale"><NumberField control={control} name="verticalScale" label={tr('세로 비율')}/><NumberField control={control} name="horizontalScale" label={tr('가로 비율')}/></div>}
    {options && <div className="tf-group" data-cp="char.skew"><NumberField control={control} name="skew" label={tr('기울기')}/><LanguageField control={control}/></div>}
    {options && <div className="tf-group" data-cp="char.rotation"><NumberField control={control} name="characterRotation" label={tr('글자 회전')}/></div>}
    <div className="tf-group" data-cp="char.kerning">
      <label className="tf-field">{tr("커닝 방식")}<select aria-label={tr("커닝 방식")} value={f.kerningManual?'Manual':String(f.kerningMethod??'')} disabled={!s} onChange={e=>control.apply({kerningMethod:e.target.value})}>
        <option value="">{tr("혼합")}</option><option value="Metrics">{tr("메트릭")}</option><option value="Manual" disabled>{tr("수동")}</option><option value="Optical" disabled>{tr("광학 (조판 미완성)")}</option></select></label>
      <NumberField control={control} name="kerningValue" label={tr("수동 커닝")}/>
    </div>
    {options && <div className="tf-group tf-buttons" data-cp="char.buttons">
      {[['capitalization','ALL_CAPS',tr("모두 대문자"),'TT'],['capitalization','SMALL_CAPS',tr("작은 대문자"),'Tᴛ'],['position','SUPERSCRIPT',tr('위 첨자'),'T¹'],['position','SUBSCRIPT',tr('아래 첨자'),'T₁'],['underline',true,tr("밑줄"),'U̲'],['strikeThru',true,tr("취소선"),'S̶'],['tatechuyoko',true,tr('세로 안 가로'),'⇄']].map(([name,value,label,short])=>
        <button key={String(label)} type="button" aria-label={String(label)} aria-pressed={f[String(name)]===value} disabled={!s}
          onClick={()=>toggle(String(name),value)} title={String(label)}>{short}</button>)}
    </div>}
  </>;
}
export function ParagraphFields({control,options=true}:{control:TextFormatting;options?:boolean}) {
  const s=control.selection;
  return <>
    <div className="tf-group tf-align" data-cp="para.align">
      {[['LEFT_ALIGN',tr("왼쪽 정렬")],['CENTER_ALIGN',tr("가운데 정렬")],['RIGHT_ALIGN',tr("오른쪽 정렬")],['LEFT_JUSTIFIED',tr("마지막 줄 왼쪽 균등")],['FULLY_JUSTIFIED',tr("모든 줄 균등")],
        ['CENTER_JUSTIFIED',tr("마지막 줄 가운데 균등")],['TO_BINDING_SIDE',tr("안쪽 정렬")],['AWAY_FROM_BINDING_SIDE',tr("바깥쪽 정렬")]].map(([v,label])=>
        <button key={v} aria-label={label} aria-pressed={s?.format.justification===v} disabled={!s} onClick={()=>control.apply({justification:v})}>{label}</button>)}
    </div>
    <div className="tf-group" data-cp="para.indent"><NumberField control={control} name="leftIndent" label={tr("왼쪽 들여쓰기")}/><NumberField control={control} name="rightIndent" label={tr("오른쪽 들여쓰기")}/></div>
    {options && <><div className="tf-group" data-cp="para.space"><NumberField control={control} name="firstLineIndent" label={tr("첫 줄 들여쓰기")}/><NumberField control={control} name="spaceBefore" label={tr("앞 공백")}/></div>
    <div className="tf-group" data-cp="para.spaceAfter"><NumberField control={control} name="lastLineIndent" label={tr('마지막 줄 오른쪽 들여쓰기')}/><NumberField control={control} name="spaceAfter" label={tr("뒤 공백")}/></div>
    <div className="tf-group" data-cp="para.dropCap"><NumberField control={control} name="dropCapLines" label={tr('장식 문자 줄 수')}/><NumberField control={control} name="dropCapCharacters" label={tr('장식 문자 글자 수')}/></div></>}
    {options && <label className="tf-field" data-cp="para.hyphenate"><input type="checkbox" aria-label={tr('하이픈 넣기')} disabled={!s} checked={s?.format.hyphenation === true} ref={el => { if (el) el.indeterminate = s?.format.hyphenation === null; }} onChange={e => control.apply({ hyphenation: e.target.checked })}/>{tr('하이픈 넣기')}</label>}
    {options && <div className="tf-group" data-cp="para.composer"><ComposerField control={control}/></div>}
    {options && <label className="tf-field" data-cp="para.optical"><input type="checkbox" aria-label={tr('광학 여백 무시')} disabled={!s || !control.opticalMarginEnabled} checked={s?.format.ignoreEdgeAlignment === true} ref={el => { if (el) el.indeterminate = s?.format.ignoreEdgeAlignment === null; }} onChange={e => control.apply({ ignoreEdgeAlignment: e.target.checked })}/>{tr('광학 여백 무시')}</label>}
  </>;
}
export function TextFormatBar({control,mode,setMode}:{control:TextFormatting;mode:'character'|'paragraph';setMode:(m:'character'|'paragraph')=>void}) {
  const bar=useControlBar();
  return <div className="control text-format-bar">
    <div className="cp-group mode"><button className={`cp-mode ${mode==='character'?'on':''}`} onClick={()=>setMode('character')} title={tr("문자 서식 컨트롤")}>A</button>
      <button className={`cp-mode ${mode==='paragraph'?'on':''}`} onClick={()=>setMode('paragraph')} title={tr("단락 서식 컨트롤")}>¶</button></div>
    {mode==='character'?<CharacterFields control={control}/>:<ParagraphFields control={control}/>}
    <div className="tf-group" data-cp={mode==='character'?'char.style':'para.style'}><StyleSelect control={control} kind={mode}/><span className="tf-help">{control.selection?tr("단위: pt · Alt+스타일: 재정의 지우기"):tr("텍스트 프레임 안을 누르세요")}</span></div>
    <HiddenGroups hidden={mode==='character'?bar.character:bar.paragraph} bar={mode}/>
    <ControlBarMenu bar={mode}/>
  </div>;
}
