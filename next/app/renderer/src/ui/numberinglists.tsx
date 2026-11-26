import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { Command } from './commands';
import { fieldKey } from './keys';
type List={id:string;name:string;acrossStories:boolean;acrossDocuments:boolean};
type Context={path?:string;editable:boolean;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>};
export function useNumberingLists(ctx:Context){
 const [open,setOpen]=useState(false);
 useEffect(()=>setOpen(false),[ctx.path]);
 const handlers:Record<string,Partial<Command>>={'id.type.bulletedAndNumberedLists.defineLists':{run:ctx.editable?()=>setOpen(true):undefined}};
 return {handlers,dialog:open&&<ListsDialog edit={ctx.edit} close={()=>setOpen(false)}/>};
}
function ListsDialog({edit,close}:{edit:Context['edit'];close:()=>void}){
 const [lists,setLists]=useState<List[]>([]),[id,setId]=useState(''),[name,setName]=useState(''),[acrossStories,setAcrossStories]=useState(true),[acrossDocuments,setAcrossDocuments]=useState(true),[busy,setBusy]=useState(false),[error,setError]=useState('');
 const load=async()=>{const {header:r}=await window.folio.request({cmd:'numberingLists'});if(!r.ok)throw new Error(String(r.error));setLists(r.lists as List[]);};
 useEffect(()=>{void load().catch(e=>setError(String(e)));},[]);
 const choose=(value:string)=>{setId(value);const l=lists.find(l=>l.id===value);setName(l?.name??'');setAcrossStories(l?.acrossStories??true);setAcrossDocuments(l?.acrossDocuments??true);};
 const builtin=id==='NumberingList/$ID/[Default]';
 const run=async(remove:boolean)=>{setBusy(true);setError('');try{const cmd:EditCommand=remove?{op:'deleteNumberingList',id}:{op:'setNumberingList',id,name,props:{acrossStories,acrossDocuments}};if(await edit(tr('목록 정의'),[cmd])){await load();choose('');}else setError(tr('입력값을 확인하세요.'));}catch(e){setError(String(e));}finally{setBusy(false);}};
 return createPortal(<div className="style-modal" role="dialog" aria-label={tr('목록 정의')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='cancel'&&!busy)close();}}><form className="card dialog style-options" onSubmit={e=>{e.preventDefault();void run(false);}}>
 <h2>{tr('목록 정의')}</h2>
 <label>{tr('목록')}<select aria-label={tr('목록')} value={id} disabled={busy} onChange={e=>choose(e.target.value)}><option value="">{tr('새 목록')}</option>{lists.map(l=><option key={l.id} value={l.id}>{l.name}</option>)}</select></label>
 <label>{tr('목록 이름')}<input aria-label={tr('목록 이름')} value={name} disabled={builtin||busy} onChange={e=>setName(e.target.value)}/></label>
 <label><input type="checkbox" checked={acrossStories} disabled={builtin||busy} onChange={e=>setAcrossStories(e.target.checked)}/>{tr('스토리 간 번호 계속')}</label>
 <label><input type="checkbox" checked={acrossDocuments} disabled={builtin||busy} onChange={e=>setAcrossDocuments(e.target.checked)}/>{tr('이전 문서에서 번호 계속')}</label>
 <p>{tr('문서 간 번호 이어짐은 IDML에 저장됩니다. 책 연결을 통한 조판은 준비 중입니다.')}</p>
 {error&&<p role="alert">{error}</p>}<div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('닫기')}</button><button type="button" disabled={busy||!id||builtin} onClick={()=>void run(true)}>{tr('삭제')}</button><button type="submit" disabled={busy||builtin||!name.trim()}>{tr('저장')}</button></div>
 </form></div>,document.body);
}

export function useNumberingListChoices(){
 const [choices,setChoices]=useState([{value:'NumberingList/$ID/[Default]',label:tr('기본 목록')}]);
 useEffect(()=>{let active=true;void window.folio.request({cmd:'numberingLists'}).then(({header:r})=>{if(active&&r.ok)setChoices((r.lists as List[]).map(l=>({value:l.id,label:l.id==='NumberingList/$ID/[Default]'?tr('기본 목록'):l.name})));}).catch(()=>{});return()=>{active=false;};},[]);
 return choices;
}
