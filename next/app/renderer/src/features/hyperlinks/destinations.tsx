// Hyperlinks panel ▸ destinations (build-c): the document's hyperlink destinations, New Hyperlink Destination (a text anchor at the text
// cursor, a page, a URL), Hyperlink Destination Options and Delete Unused Destinations. The work is the engine's
// (`createHyperlinkDestination`, `setHyperlinkDestination`, `deleteUnusedHyperlinkDestinations`); this file only collects the values.
import {useState} from 'react';
import {tr} from '../../i18n';
import type {EditCommand,EditResult} from '../../engine';
import type {HyperlinkData} from './types';

export type DestinationForm=null|{mode:'new'}|{mode:'edit';id:string};
type Destination=HyperlinkData['destinations'][number]&{viewSetting?:string};
const ZOOMS:[string,()=>string][]=[['FitView',()=>tr('창에 맞춤')],['Fixed',()=>tr('고정')],['FitWidth',()=>tr('너비에 맞춤')],['FitHeight',()=>tr('높이에 맞춤')],['FitVisible',()=>tr('보이는 영역에 맞춤')],['InheritZoom',()=>tr('확대/축소 상속')]];
/** The destinations Data Merge keeps for its fields are its own: not listed here. */
const listed=(d:Destination)=>!/^(DBF|IMG|QRC)_/.test(d.name);
export const kindName=(kind:string)=>kind==='HyperlinkTextDestination'||kind==='ParagraphDestination'?tr('텍스트 앵커'):kind==='HyperlinkPageDestination'?tr('페이지'):kind==='HyperlinkURLDestination'?'URL':kind;

function nextName(data:HyperlinkData,prefix:string):string {
  let n=1;while(data.destinations.some(d=>d.name===`${prefix} ${n}`))n++;return `${prefix} ${n}`;
}

type Props={data:HyperlinkData;editable:boolean;caret:{story:string;at:number}|null;form:DestinationForm;setForm:(f:DestinationForm)=>void;
  picked:string;pick:(id:string)=>void;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;pageCount:number};

function NewForm({data,caret,setForm,edit,pageCount}:Props) {
  const [kind,setKind]=useState<'text'|'page'|'url'>(caret?'text':'page');
  const prefix=kind==='text'?tr('앵커'):kind==='page'?tr('페이지'):'URL';
  const [name,setName]=useState(''),[url,setUrl]=useState('https://'),[page,setPage]=useState('1'),[zoom,setZoom]=useState('FitView');
  const shown=name||nextName(data,prefix);
  const ok=async()=>{
    const r=await edit(tr('새 하이퍼링크 대상'),[{op:'createHyperlinkDestination',kind,name:shown,story:caret?.story??'',at:caret?.at??0,page:kind==='page'?page:'',url:kind==='url'?url:'',view_setting:kind==='page'?zoom:''}]);
    if(r)setForm(null);
  };
  return <form className="hyperlink-dest-form" data-destination-form="new" onSubmit={e=>{e.preventDefault();void ok();}}>
    <label>{tr('종류')}<select aria-label={tr('대상 종류')} value={kind} onChange={e=>setKind(e.target.value as 'text'|'page'|'url')}>
      <option value="text" disabled={!caret}>{tr('텍스트 앵커')}</option><option value="page">{tr('페이지')}</option><option value="url">URL</option></select></label>
    <label>{tr('이름')}<input aria-label={tr('대상 이름')} value={shown} onChange={e=>setName(e.target.value)}/></label>
    {kind==='text'&&<p>{caret?tr('텍스트 커서 자리에 앵커를 만듭니다.'):tr('문자 도구로 앵커를 둘 자리에 커서를 두세요.')}</p>}
    {kind==='url'&&<label>URL<input aria-label={tr('대상 URL')} value={url} onChange={e=>setUrl(e.target.value)}/></label>}
    {kind==='page'&&<>
      <label>{tr('페이지')}<input aria-label={tr('대상 페이지')} inputMode="numeric" value={page} onChange={e=>setPage(e.target.value)}/><small>{tr('1–{0}',{0:pageCount})}</small></label>
      <label>{tr('확대/축소 설정')}<select aria-label={tr('확대/축소 설정')} value={zoom} onChange={e=>setZoom(e.target.value)}>{ZOOMS.map(([v,l])=><option key={v} value={v}>{l()}</option>)}</select></label>
    </>}
    <div className="hyperlink-dest-actions"><button type="submit" disabled={kind==='text'&&!caret}>{tr('확인')}</button><button type="button" onClick={()=>setForm(null)}>{tr('취소')}</button></div>
  </form>;
}

function EditForm({dest,setForm,edit}:{dest:Destination;setForm:(f:DestinationForm)=>void;edit:Props['edit']}) {
  const [name,setName]=useState(dest.name),[url,setUrl]=useState(dest.kind==='HyperlinkURLDestination'?dest.value:''),[page,setPage]=useState('');
  const [zoom,setZoom]=useState(dest.viewSetting||'FitView');
  const ok=async()=>{
    const r=await edit(tr('하이퍼링크 대상 옵션'),[{op:'setHyperlinkDestination',id:dest.id,name,url:dest.kind==='HyperlinkURLDestination'?url:'',page:dest.kind==='HyperlinkPageDestination'?page:'',view_setting:dest.kind==='HyperlinkPageDestination'?zoom:''}]);
    if(r)setForm(null);
  };
  return <form className="hyperlink-dest-form" data-destination-form="edit" onSubmit={e=>{e.preventDefault();void ok();}}>
    <p>{tr('종류: {0}',{0:kindName(dest.kind)})}</p>
    <label>{tr('이름')}<input aria-label={tr('대상 이름')} value={name} onChange={e=>setName(e.target.value)}/></label>
    {dest.kind==='HyperlinkURLDestination'&&<label>URL<input aria-label={tr('대상 URL')} value={url} onChange={e=>setUrl(e.target.value)}/></label>}
    {dest.kind==='HyperlinkPageDestination'&&<>
      <label>{tr('페이지 (비우면 그대로)')}<input aria-label={tr('대상 페이지')} inputMode="numeric" value={page} onChange={e=>setPage(e.target.value)}/></label>
      <label>{tr('확대/축소 설정')}<select aria-label={tr('확대/축소 설정')} value={zoom} onChange={e=>setZoom(e.target.value)}>{ZOOMS.map(([v,l])=><option key={v} value={v}>{l()}</option>)}</select></label>
    </>}
    <div className="hyperlink-dest-actions"><button type="submit" disabled={!name.trim()}>{tr('확인')}</button><button type="button" onClick={()=>setForm(null)}>{tr('취소')}</button></div>
  </form>;
}

/** The destinations block of the Hyperlinks panel. */
export function DestinationsBlock(props:Props) {
  const {data,editable,form,setForm,picked,pick,edit}=props;
  const dests=(data.destinations as Destination[]).filter(listed);
  const current=dests.find(d=>d.id===picked);
  const [note,setNote]=useState('');
  const removeUnused=async()=>{const r=await edit(tr('안 쓰는 대상 삭제'),[{op:'deleteUnusedHyperlinkDestinations'}]);if(r)setNote(tr('안 쓰는 대상 {0}개를 지웠습니다.',{0:r.created?.length??0}));};
  return <fieldset className="hyperlink-dests" data-destinations={dests.length}>
    <legend>{tr('하이퍼링크 대상')}</legend>
    <div className="hyperlinks-list hyperlink-dest-list" aria-label={tr('대상 목록')}>
      {dests.map(d=><button type="button" key={d.id} aria-pressed={picked===d.id} data-destination={d.id} onClick={()=>pick(d.id)} onDoubleClick={()=>{pick(d.id);if(editable)setForm({mode:'edit',id:d.id});}}>
        {d.name} <small>{kindName(d.kind)}</small></button>)}
      {!dests.length&&<p className="p-empty">{tr('대상이 없습니다.')}</p>}
    </div>
    <div className="hyperlinks-actions">
      <button type="button" disabled={!editable} onClick={()=>setForm({mode:'new'})}>{tr('새 대상…')}</button>
      <button type="button" disabled={!editable||!current} onClick={()=>current&&setForm({mode:'edit',id:current.id})}>{tr('대상 옵션…')}</button>
      <button type="button" disabled={!editable||!data.destinations.length} onClick={()=>void removeUnused()}>{tr('안 쓰는 대상 삭제')}</button>
    </div>
    {form?.mode==='new'&&<NewForm {...props}/>}
    {form?.mode==='edit'&&(()=>{const d=(data.destinations as Destination[]).find(x=>x.id===form.id);return d?<EditForm key={d.id} dest={d} setForm={setForm} edit={edit}/>:null;})()}
    {note&&<p role="status">{note}</p>}
  </fieldset>;
}
