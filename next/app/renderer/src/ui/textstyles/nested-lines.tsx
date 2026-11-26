import { t as tr } from '../../i18n';
import type { NestedLineStyle } from '../../engine';
import { NumberField } from '../controls/numberfield';
import './grep.css';

const NONE = 'CharacterStyle/$ID/[No character style]';
const REPEAT = '*repeat*';
export const MAX_LINE_ITEMS = 64;

/** A paragraph style's nested line styles (command `setLineStyles`, next/oracle/rules/dropcap.ko.md): a character style for the
 *  paragraph's next N lines, top down; `[Repeat]` runs the items before it again to the end of the paragraph. */
export function NestedLineFields({items,styles,set,disabled=false}:{items:NestedLineStyle[];styles:{id:string;name:string}[];set:(items:NestedLineStyle[])=>void;disabled?:boolean}) {
  const change=(index:number,patch:Partial<NestedLineStyle>)=>set(items.map((x,i)=>i===index?{...x,...patch}:x));
  const move=(index:number,delta:number)=>{const next=[...items];[next[index],next[index+delta]]=[next[index+delta],next[index]];set(next);};
  const whole=(text:string,min:number,max:number)=>Math.max(min,Math.min(max,Math.round(Number(text))||min));
  return <fieldset disabled={disabled} className="grep-styles-fields nested-line-styles-fields">
    <legend>{tr('중첩 줄 스타일')}</legend>
    <p>{tr('단락의 첫 줄부터 위에서 아래로 줄마다 글자 스타일을 입힙니다. 글자 스타일 때문에 줄 나눔이 바뀌면 바뀐 줄에 다시 입힙니다. 직접 입힌 글자 스타일과 로컬 서식이 우선합니다.')}</p>
    {items.map((item,index)=>{
      const repeat=item.repeat===true;
      return <div className="grep-style-rule" key={index} data-nested-line-item={index}>
        <label>{tr('문자 스타일')}<select aria-label={tr('중첩 줄 스타일 문자 스타일')} value={repeat?REPEAT:item.style} onChange={e=>change(index,e.target.value===REPEAT?{repeat:true,style:NONE,lines:Math.max(1,Math.min(index,item.lines))}:{repeat:false,style:e.target.value})}>
          <option value={NONE}>{tr('[없음]')}</option>
          {styles.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
          <option value={REPEAT}>{tr('[반복]')}</option>
        </select></label>
        <label>{repeat?tr('앞의 항목 수'):tr('줄 수')}<NumberField value={item.lines} label={tr('중첩 줄 스타일 줄 수')} min={1} max={repeat?Math.max(1,index):9999} step={1} decimals={0} width={72} inDialog disabled={disabled}
          parse={t=>whole(t,1,repeat?Math.max(1,index):9999)} onCommit={v=>change(index,{lines:v})}/></label>
        <div className="dialog-actions">
          <button type="button" disabled={index===0} onClick={()=>move(index,-1)}>{tr('위로')}</button>
          <button type="button" disabled={index===items.length-1} onClick={()=>move(index,1)}>{tr('아래로')}</button>
          <button type="button" onClick={()=>set(items.filter((_,i)=>i!==index))}>{tr('삭제')}</button>
        </div>
      </div>;
    })}
    <button type="button" disabled={items.length>=MAX_LINE_ITEMS} onClick={()=>set([...items,{style:styles[0]?.id??NONE,lines:1}])}>{tr('새 중첩 줄 스타일')}</button>
  </fieldset>;
}
