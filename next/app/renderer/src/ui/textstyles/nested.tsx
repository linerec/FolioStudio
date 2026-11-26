import { t as tr } from '../../i18n';
import type { NestedStyle } from '../../engine';
import { NumberField } from '../controls/numberfield';
import './grep.css';

const NONE = 'CharacterStyle/$ID/[No character style]';
const CUSTOM = '*custom*';
/** Delimiters Folio applies (measured with InDesign: next/oracle/rules/specialchars-c1.ko.md). */
const named = () => [
  ['AnyCharacter', tr('글자')], ['AnyWord', tr('단어')], ['Sentence', tr('문장')], ['Letters', tr('문자(글자 종류)')], ['Digits', tr('숫자')],
  ['Tabs', tr('탭 문자')], ['ForcedLineBreak', tr('강제 줄 바꿈')], ['IndentHereTab', tr('여기서 들여쓰기 탭')],
  ['EmSpace', tr('전각 공백(Em)')], ['EnSpace', tr('반각 공백(En)')], ['NonbreakingSpace', tr('줄 바꿈 없는 공백')],
  ['EndNestedStyle', tr('중첩 스타일 끝 문자')], ['AutoPageNumber', tr('자동 쪽 번호')], ['SectionMarker', tr('섹션 표시')],
  ['InlineGraphic', tr('인라인 그래픽')], ['Repeat', tr('[반복]')],
] as const;

/** The drop cap's character style is the list's first item with this delimiter (next/oracle/rules/dropcap.ko.md). */
export const DROP_CAP_DELIMITER = 'Dropcap';
export const isDropCapItem = (item?: NestedStyle) => !!item && !item.custom && item.delimiter === DROP_CAP_DELIMITER;

export function NestedFields({items:all,styles,set:setAll,disabled=false}:{items:NestedStyle[];styles:{id:string;name:string}[];set:(items:NestedStyle[])=>void;disabled?:boolean}) {
  // The drop cap's character style has its own field; the list below holds the other items.
  const drop=isDropCapItem(all[0])?all[0]:undefined, items=drop?all.slice(1):all;
  const set=(next:NestedStyle[])=>setAll(drop?[drop,...next]:next);
  const setDrop=(style:string)=>setAll(style===''?items:[{...(drop??{delimiter:DROP_CAP_DELIMITER,repetition:1,inclusive:true}),style},...items]);
  const change=(index:number,patch:Partial<NestedStyle>)=>set(items.map((x,i)=>i===index?{...x,...patch}:x));
  const move=(index:number,delta:number)=>{const next=[...items];[next[index],next[index+delta]]=[next[index+delta],next[index]];set(next);};
  return <fieldset disabled={disabled} className="grep-styles-fields nested-styles-fields">
    <legend>{tr('중첩 스타일')}</legend>
    <label>{tr('장식 문자 글자 스타일')}<select aria-label={tr('장식 문자 글자 스타일')} value={drop?.style??''} onChange={e=>setDrop(e.target.value)}>
      <option value="">{tr('[없음]')}</option>
      {styles.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
    </select></label>
    <p>{tr('단락 시작부터 위에서 아래로 차례로 글자 스타일을 입힙니다. 문자 스타일과 로컬 서식이 우선합니다.')}</p>
    {items.map((item,index)=>{
      const repeat=!item.custom&&item.delimiter==='Repeat';
      const choice=item.custom?CUSTOM:item.delimiter;
      return <div className="grep-style-rule" key={index} data-nested-item={index}>
        {!repeat&&<label>{tr('문자 스타일')}<select aria-label={tr('중첩 스타일 문자 스타일')} value={item.style} onChange={e=>change(index,{style:e.target.value})}>
          <option value={NONE}>{tr('[없음]')}</option>
          {styles.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
        </select></label>}
        {!repeat&&<label>{tr('범위')}<select aria-label={tr('중첩 스타일 범위')} value={item.inclusive?'through':'upto'} onChange={e=>change(index,{inclusive:e.target.value==='through'})}>
          <option value="through">{tr('포함')}</option><option value="upto">{tr('앞까지')}</option>
        </select></label>}
        <label>{repeat?tr('앞의 항목 수'):tr('개수')}<NumberField value={item.repetition} label={tr('중첩 스타일 개수')} min={1} max={9999} step={1} decimals={0} width={72} inDialog disabled={disabled}
          parse={t=>Math.max(1,Math.min(9999,Math.round(Number(t))||1))} onCommit={v=>change(index,{repetition:v})}/></label>
        <label>{tr('구분자')}<select aria-label={tr('중첩 스타일 구분자')} value={choice} onChange={e=>change(index,e.target.value===CUSTOM?{custom:true,delimiter:item.custom?item.delimiter:','}:{custom:false,delimiter:e.target.value})}>
          {named().map(([id,label])=><option key={id} value={id}>{label}</option>)}
          <option value={CUSTOM}>{tr('입력한 글자')}</option>
        </select></label>
        {item.custom&&<label>{tr('구분 글자')}<input aria-label={tr('중첩 스타일 구분 글자')} value={item.delimiter} onChange={e=>change(index,{delimiter:e.target.value})}/></label>}
        <div className="dialog-actions">
          <button type="button" disabled={index===0} onClick={()=>move(index,-1)}>{tr('위로')}</button>
          <button type="button" disabled={index===items.length-1} onClick={()=>move(index,1)}>{tr('아래로')}</button>
          <button type="button" onClick={()=>set(items.filter((_,i)=>i!==index))}>{tr('삭제')}</button>
        </div>
      </div>;
    })}
    <button type="button" disabled={items.length>=64} onClick={()=>set([...items,{style:styles[0]?.id??NONE,delimiter:'AnyWord',repetition:1,inclusive:true}])}>{tr('새 중첩 스타일')}</button>
  </fieldset>;
}
