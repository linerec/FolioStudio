import { t as tr } from '../../i18n';
import type { GrepStyle } from '../../engine';
import './grep.css';

export function GrepFields({rules,styles,set,disabled=false}:{rules:GrepStyle[];styles:{id:string;name:string}[];set:(rules:GrepStyle[])=>void;disabled?:boolean}) {
  const move=(index:number,delta:number)=>{const next=[...rules];[next[index],next[index+delta]]=[next[index+delta],next[index]];set(next);};
  return <fieldset disabled={disabled} className="grep-styles-fields">
    <legend>{tr('GREP 스타일')}</legend>
    <p>{tr('아래쪽 규칙이 같은 속성에서 우선합니다. 문자 스타일과 로컬 서식은 유지됩니다.')}</p>
    {rules.map((rule,index)=><div className="grep-style-rule" key={index} data-grep-rule={index}>
      <label>{tr('문자 스타일')}<select aria-label={tr('GREP 문자 스타일')} value={rule.style} onChange={e=>set(rules.map((r,i)=>i===index?{...r,style:e.target.value}:r))}>
        {styles.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
      </select></label>
      <label>{tr('정규식')}<input aria-label={tr('GREP 정규식')} value={rule.expression} onChange={e=>set(rules.map((r,i)=>i===index?{...r,expression:e.target.value}:r))}/></label>
      <div className="dialog-actions">
        <button type="button" disabled={index===0} onClick={()=>move(index,-1)}>{tr('위로')}</button>
        <button type="button" disabled={index===rules.length-1} onClick={()=>move(index,1)}>{tr('아래로')}</button>
        <button type="button" onClick={()=>set(rules.filter((_,i)=>i!==index))}>{tr('삭제')}</button>
      </div>
    </div>)}
    <button type="button" disabled={!styles.length||rules.length>=256} onClick={()=>set([...rules,{style:styles[0].id,expression:''}])}>{tr('규칙 추가')}</button>
    <p>{tr('지원: 문자 클래스(\\d \\w \\s \\l \\u \\h, [[:alpha:]]), 한정자, 그룹, 앞뒤 탐색, 역참조, (?i), 앵커, ~ 특수 코드')}</p>
  </fieldset>;
}
