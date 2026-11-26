import { useEffect, useRef, useState } from 'react';
import type { TabStop } from '../engine';
import { tr } from '../i18n';
import type { PanelCtx } from './panels';
import { fieldKey } from './keys';
import type { Commands } from './commands';
import { Dropdown } from './menu';
import { CONTEXT_MENUS } from './menudef';
import { declarePanelCommands, usePanelCommands } from './panelbus';
import { NumberField } from './controls/numberfield';

const empty = ():TabStop => ({position:36,alignment:'LEFT_ALIGN',leader:'',alignmentCharacter:'.'});
// The right-click menu of a tab marker (menuaudit-c): what each item does, by command id.
const TC = 'id.ctx.tabs.';
declarePanelCommands([TC+'clearAll', TC+'deleteTab', TC+'repeatTab', TC+'resetIndents', TC+'tabType.leftJustifiedTab', TC+'tabType.centerJustifiedTab', TC+'tabType.rightJustifiedTab', TC+'tabType.decimalTab']);
const TAB_TYPES: [string, TabStop['alignment']][] = [['leftJustifiedTab','LEFT_ALIGN'],['centerJustifiedTab','CENTER_ALIGN'],['rightJustifiedTab','RIGHT_ALIGN'],['decimalTab','CHARACTER_ALIGN']];

/** The width of a column of the first frame of a story (its own width, less the insets, shared by the columns and gutters); null when not loaded. */
function columnWidth(ctx:PanelCtx,story:string|undefined):number|null{
  if(!story)return null;
  for(const pg of ctx.info?.pages??[]){
    const it=ctx.store.peek(pg.index)?.items.find(i=>i.text?.story===story);
    if(!it)continue;
    const o=it.text?.options,m=it.transform,w=(it.inner[2]-it.inner[0])*Math.hypot(m[0],m[1])-(o?(o.insets[1]??0)+(o.insets[3]??0):0),n=Math.max(1,it.text?.columns??1);
    return (w-(n-1)*(o?.gutter??12))/n;
  }
  return null;
}

export function TabsPanel({ctx,commands}:{ctx:PanelCtx;commands?:Commands}) {
  const control=ctx.textFormatting, value=control.selection?.format.tabList;
  const tabs:TabStop[]=Array.isArray(value)?value:[];
  const [index,setIndex]=useState(-1),[draft,setDraftState]=useState<TabStop>(empty),[error,setError]=useState(false);
  // The draft is also kept in a ref that is written at once: Enter in the position box chooses the typed value and the panel's Enter
  // then applies the draft in the same key press, before React has drawn the new draft.
  const draftRef=useRef(draft);
  const setDraft=(next:TabStop)=>{draftRef.current=next;setDraftState(next);};
  const signature=JSON.stringify(value);
  useEffect(()=>{if(index>=0&&tabs[index])setDraft(tabs[index]);else{setIndex(-1);setDraft(empty());}},[signature,control.selection?.story,control.selection?.start,control.selection?.end]);
  const [menu,setMenu]=useState<{x:number;y:number}|null>(null);
  const span=Math.max(360,...tabs.map(t=>t.position+36));
  const save=(next:TabStop[])=>{
    if(next.some(t=>!Number.isFinite(t.position)||t.position<0||t.position>10000)||new Set(next.map(t=>t.position)).size!==next.length){setError(true);return;}
    setError(false);control.apply({tabList:next.sort((a,b)=>a.position-b.position)});
  };
  const apply=()=>{const next=[...tabs];if(index>=0)next[index]=draftRef.current;else next.push(draftRef.current);save(next);setIndex(-1);};
  const picked=index>=0&&tabs[index]?index:-1;
  usePanelCommands('tabs',{
    [TC+'clearAll']:control.selection&&tabs.length?()=>{save([]);setIndex(-1);}:undefined,
    [TC+'deleteTab']:control.selection&&picked>=0?()=>{save(tabs.filter((_,i)=>i!==picked));setIndex(-1);}:undefined,
    // Repeat Tab (tools-c1): more tabs of the selected one's kind, each the distance between it and the tab before it (or the left indent) further
    // on, up to the width of the frame's column; a place that already has a tab keeps it (Folio's choice: InDesign's own count was not measured)
    [TC+'repeatTab']:control.selection&&picked>=0&&ctx.editable?()=>{
      const t=tabs[picked],before=picked>0?tabs[picked-1].position:Number(control.selection?.format.leftIndent??0)||0,step=t.position-before;
      const width=columnWidth(ctx,control.selection?.story)??span;
      if(!(step>0.01))return;
      const more:TabStop[]=[];
      for(let p=t.position+step;p<=width+0.001&&more.length<200;p+=step){const at=Math.round(p*1e4)/1e4;if(!tabs.some(x=>Math.abs(x.position-at)<0.01))more.push({...t,position:at});}
      if(more.length)save([...tabs,...more]);
    }:undefined,
    // Reset Indents: the left and the first line indent of the selected paragraphs go back to 0
    [TC+'resetIndents']:control.selection&&ctx.editable?()=>control.apply({leftIndent:0,firstLineIndent:0}):undefined,
    ...Object.fromEntries(TAB_TYPES.map(([name,alignment])=>[TC+'tabType.'+name,control.selection&&picked>=0?{run:()=>save(tabs.map((t,i)=>i===picked?{...t,alignment}:t)),checked:tabs[picked].alignment===alignment}:undefined])),
  });
  const position=(e:React.PointerEvent<SVGElement>)=>{
    const rect=e.currentTarget.ownerSVGElement?.getBoundingClientRect()??e.currentTarget.getBoundingClientRect();
    return Math.max(0,Math.round((e.clientX-rect.left)/rect.width*span));
  };
  return <div className="tabs-panel" style={{padding:10}} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='commit'){e.preventDefault();apply();}}}>
    {!control.selection&&<p>{tr('텍스트 프레임 안을 누르세요')}</p>}
    <fieldset disabled={!control.selection} style={{border:0,padding:0,margin:0}}>
      <svg aria-label={tr('탭 눈금자')} viewBox={`0 0 ${span} 38`} style={{width:'100%',height:48,background:'var(--panel-bg, #eee)',touchAction:'none'}}
        onPointerDown={e=>{if(!control.selection)return;setIndex(-1);setDraft({...empty(),position:position(e)});}}>
        {Array.from({length:Math.floor(span/36)+1},(_,i)=><g key={i}><path d={`M${i*36},20 v12`} stroke="currentColor"/><text x={i*36+2} y={15} fontSize={11}>{i*36}</text></g>)}
        {tabs.map((t,i)=><path key={i} aria-label={tr('탭 위치 {0}',{0:t.position})} d={`M${t.position-5},25 h10 l-5,10 z`} fill={i===index?'#1676db':'currentColor'}
          onContextMenu={e=>{e.preventDefault();e.stopPropagation();setIndex(i);setDraft(t);setMenu({x:e.clientX,y:e.clientY});}}
          onPointerDown={e=>{e.stopPropagation();setIndex(i);setDraft(t);e.currentTarget.setPointerCapture(e.pointerId);}}
          onPointerUp={e=>{e.stopPropagation();const p=position(e);if(Math.abs(p-t.position)>2){save(tabs.map((s,j)=>j===i?{...s,position:p}:s));setIndex(-1);}}}/>)}
      </svg>
      <select aria-label={tr('탭 목록')} style={{width:'100%'}} value={index} onChange={e=>{const n=Number(e.target.value);setIndex(n);setDraft(tabs[n]??empty());}}>
        <option value={-1}>{tr('새 탭')}</option>{tabs.map((t,i)=><option key={i} value={i}>{t.position} pt · {t.leader}</option>)}
      </select>
      <label className="tf-field">{tr('탭 위치')}<NumberField value={draft.position} label={tr('탭 위치')} min={0} max={10000} step={1} decimals={4} width={88} onCommit={v=>setDraft({...draftRef.current,position:v})}/></label>
      <label className="tf-field">{tr('탭 정렬')}<select aria-label={tr('탭 정렬')} value={draft.alignment} onChange={e=>setDraft({...draftRef.current,alignment:e.target.value as TabStop['alignment']})}>
        <option value="LEFT_ALIGN">{tr('왼쪽 정렬')}</option><option value="CENTER_ALIGN">{tr('가운데 정렬')}</option><option value="RIGHT_ALIGN">{tr('오른쪽 정렬')}</option><option value="CHARACTER_ALIGN">{tr('소수점 정렬')}</option>
      </select></label>
      <label className="tf-field">{tr('채울 문자')}<input aria-label={tr('채울 문자')} maxLength={8} value={draft.leader} onChange={e=>setDraft({...draftRef.current,leader:e.target.value})}/></label>
      <label className="tf-field">{tr('정렬 문자')}<input aria-label={tr('정렬 문자')} maxLength={8} value={draft.alignmentCharacter} onChange={e=>setDraft({...draftRef.current,alignmentCharacter:e.target.value})}/></label>
      <div style={{display:'flex',gap:6,marginTop:8}}><button onClick={apply}>{tr('적용')}</button><button disabled={index<0} onClick={()=>{save(tabs.filter((_,i)=>i!==index));setIndex(-1);}}>{tr('삭제')}</button><button disabled={!tabs.length} onClick={()=>{save([]);setIndex(-1);}}>{tr('모든 탭 지우기')}</button></div>
      {error&&<p role="alert">{tr('탭 위치는 중복 없이 0~10000pt로 입력하세요.')}</p>}
    </fieldset>
    {menu&&commands&&<Dropdown items={CONTEXT_MENUS.tabs??[]} commands={commands} x={menu.x} y={menu.y} onClose={()=>setMenu(null)}/>}
  </div>;
}
