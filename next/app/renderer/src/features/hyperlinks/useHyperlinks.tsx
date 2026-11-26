import { useEffect, useState } from 'react';
import { tr } from '../../i18n';
import type { DocumentInfo } from '../../inspect';
import type { EditCommand, EditResult } from '../../engine';
import type { TextSelectionState } from '../../ui/textedit';
import type { Command } from '../../ui/commands';
import { emptyLinks, type Link } from './types';
import { useTarget } from './useTarget';
import { goToTarget, type GotoHost } from './goto';
import { DestinationsBlock, type DestinationForm } from './destinations';
import './hyperlinks.css';

type Context={info:DocumentInfo|null;editable:boolean;selection:TextSelectionState|null;
  edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;show:(id:'hyperlinks'|'crossReferences')=>void;
  /** The selected objects' ids: an object that is a link's source makes that link the active one. */
  objects:string[];
  goto:GotoHost;
  /** The open document: a link picked in the panel is forgotten when another document opens (its ids can repeat). */
  document?:string};
export function useHyperlinks(ctx:Context) {
  const data=ctx.info?.hyperlinks??emptyLinks;
  const [picked,pick]=useState('');
  const selected=ctx.selection&&!ctx.selection.nested?ctx.selection:null;
  useEffect(()=>pick(''),[selected?.story,selected?.start,selected?.end,ctx.document]);
  // the destinations (build-c): the one picked in the panel's list and the open form (new, options)
  const [pickedDest,pickDest]=useState(''),[destForm,setDestForm]=useState<DestinationForm>(null);
  useEffect(()=>{pickDest('');setDestForm(null);},[ctx.document]);
  const destPicked=data.destinations.some(d=>d.id===pickedDest);
  const selectedSource=selected&&data.sources.find(s=>s.story===selected.story&&s.start<=selected.start&&s.end>=selected.end);
  const objectSources=(data.itemSources??[]).filter(s=>ctx.objects.includes(s.item));
  const objectLink=selected?undefined:data.links.find(l=>objectSources.some(s=>s.id===l.source));
  const active=data.links.find(l=>l.id===picked)??data.links.find(l=>l.source===selectedSource?.id);
  const source=data.sources.find(s=>s.id===active?.source);
  // Go To Destination also works from a selected source object; the other commands act on the active text link only.
  const target=useTarget(data,(active??objectLink)?.id);
  const isCross=(l:Link)=>!!data.sources.find(s=>s.id===l.source)?.format;
  // the engine session holds the dialog (`newHyperlink`, `editHyperlink`, `newCrossReference`, `editCrossReference`); the frame shows it
  const open=(cross:boolean,link?:Link)=>{
    const kind=cross?'CrossReference':'Hyperlink';
    const request=link?{cmd:'openDialog',dialog:`edit${kind}`,ids:[link.id]}
      :{cmd:'openDialog',dialog:`new${kind}`,values:{story:selected!.story,start:selected!.start,...(cross?{}:{end:selected!.end}),namePrefix:cross?tr('상호 참조'):tr('하이퍼링크')}};
    void window.folio.request(request).then(({header})=>{if(!header.ok)ctx.goto.note(String(header.error));});
  };
  // Define Cross-Reference Formats: the engine's `crossReferenceFormat` dialog starts from the first format (or a new one); the words the names of a new and a copied format are made with are the screen's language
  const openFormats=()=>{void window.folio.request({cmd:'openDialog',dialog:'crossReferenceFormat',values:{format:data.formats[0]?.id??'',namePrefix:tr('새 상호 참조 형식'),copySuffix:tr('복사')}}).then(({header})=>{if(!header.ok)ctx.goto.note(String(header.error));});};
  const options=ctx.editable&&active?()=>open(isCross(active),active):undefined;
  const remove=ctx.editable&&active?()=>void ctx.edit(tr('링크 삭제'),[{op:'deleteHyperlink',id:active.id}]):undefined;
  const update=ctx.editable&&active&&source?.format?()=>void ctx.edit(tr('상호 참조 업데이트'),[{op:'updateCrossReference',id:active.id,locations:[]}]):undefined;
  const go=target?()=>goToTarget(target,ctx.goto,active?'text':'object'):undefined;
  const create=ctx.editable&&selected?()=>open(true):undefined;
  // The panel menus (menuaudit-c): the menu items call the functions the panel's buttons call. Delete / Rename / Edit act on the active link of
  // their own kind (a hyperlink in the Hyperlinks menu, a cross-reference in the Cross-Reference menu).
  const [sort,setSort]=useState<'manually'|'byName'|'byType'>('manually');
  const kindOf=(l:Link)=>isCross(l)?'cross':data.destinations.find(d=>d.id===l.destination)?.kind??'';
  const shown=(cross:boolean)=>{
    const links=data.links.filter(l=>!cross||isCross(l));
    return sort==='byName'?[...links].sort((a,b)=>a.name.localeCompare(b.name))
      :sort==='byType'?[...links].sort((a,b)=>kindOf(a).localeCompare(kindOf(b))||a.name.localeCompare(b.name)):links;
  };
  const plain=active&&!isCross(active)?active:undefined,crossLink=active&&isCross(active)?active:undefined;
  // Check URL Status (build-c2): the engine asks each URL destination's address (it uses the network, only when pressed); the answers
  // stay with this panel until another document opens
  type UrlAnswer={url:string;kind:string;status:number|null;final:string;redirects:number};
  const [urlStatus,setUrlStatus]=useState<Record<string,UrlAnswer>>({}),[checking,setChecking]=useState(false);
  useEffect(()=>setUrlStatus({}),[ctx.document]);
  const urlOf=(l:Link)=>{const d=data.destinations.find(x=>x.id===l.destination);return d?.kind==='HyperlinkURLDestination'?d.value:'';};
  const urls=[...new Set(data.links.filter(l=>!isCross(l)).map(urlOf).filter(Boolean))];
  const checkUrls=urls.length&&!checking?async()=>{
    setChecking(true);
    try{const {header}=await window.folio.request({cmd:'hyperlinkUrlStatus',urls});if(!header.ok)throw Error(String(header.error));
      setUrlStatus(Object.fromEntries((header.results as UrlAnswer[]).map(r=>[r.url,r])));}
    catch(e){ctx.goto.note(tr('웹 주소 상태를 확인하지 못했습니다: {0}',{0:String(e instanceof Error?e.message:e)}));}finally{setChecking(false);}
  }:undefined;
  const statusText=(a:UrlAnswer)=>({ok:a.redirects?tr('응답함 ({0}, 다른 주소로 옮겨짐: {1})',{0:a.status??'',1:a.final}):tr('응답함 ({0})',{0:a.status??''}),
    notFound:tr('찾을 수 없음 ({0})',{0:a.status??''}),clientError:tr('요청 거절 ({0})',{0:a.status??''}),serverError:tr('서버 오류 ({0})',{0:a.status??''}),
    tooManyRedirects:tr('다른 주소로 너무 많이 옮겨짐'),timeout:tr('시간 초과'),dns:tr('서버 이름을 찾을 수 없음'),connect:tr('연결할 수 없음'),
    tls:tr('보안 연결(인증서) 오류'),invalid:tr('올바른 웹 주소가 아님'),notChecked:tr('웹 주소가 아니어서 확인하지 않음')} as Record<string,string>)[a.kind]??a.kind;
  const sourceText=source&&!!source.story?source:undefined;
  const goSource=sourceText?()=>ctx.goto.revealText({story:sourceText.story,start:sourceText.start,end:sourceText.end}):undefined;
  const commands:Record<string,Partial<Command>>={
    'id.type.hyperlinksAndCrossReferences.newHyperlink':{run:ctx.editable&&selected&&selected.start!==selected.end?()=>open(false):undefined},
    'id.type.hyperlinksAndCrossReferences.editHyperlink':{run:ctx.editable&&plain?()=>open(false,plain):undefined},
    'id.panel.hyperlinks.deleteHyperlink':{run:plain?remove:undefined},
    'id.panel.hyperlinks.renameHyperlink':{run:ctx.editable&&plain?()=>open(false,plain):undefined},
    'id.panel.hyperlinks.goToSource':{run:goSource},
    'id.panel.hyperlinks.sort.manually':{run:()=>setSort('manually'),checked:sort==='manually'},
    'id.panel.hyperlinks.sort.byName':{run:()=>setSort('byName'),checked:sort==='byName'},
    'id.panel.hyperlinks.sort.byType':{run:()=>setSort('byType'),checked:sort==='byType'},
    'id.panel.crossReferences.deleteCrossReference':{run:crossLink?remove:undefined},
    'id.panel.crossReferences.relinkCrossReference':{run:ctx.editable&&crossLink?()=>open(true,crossLink):undefined},
    'id.panel.crossReferences.defineCrossReferenceFormats':{run:ctx.editable?openFormats:undefined},
    'id.type.hyperlinksAndCrossReferences.insertCrossReference':{run:create},
    'id.type.hyperlinksAndCrossReferences.crossReferenceOptions':{run:source?.format?options:undefined},
    'id.type.hyperlinksAndCrossReferences.updateCrossReference':{run:update},
    'id.window.interactive.hyperlinks':{run:()=>ctx.show('hyperlinks')},
    'id.window.typeAndTables.crossReferences':{run:()=>ctx.show('crossReferences')},
    'id.panel.hyperlinks.goToDestination':{run:go},
    // destinations and Reset Hyperlink (build-c); the Cross-Reference panel menu's copies are the same commands
    'id.panel.hyperlinks.newHyperlinkDestination':{run:ctx.editable?()=>{ctx.show('hyperlinks');setDestForm({mode:'new'});}:undefined},
    'id.panel.hyperlinks.hyperlinkDestinationOptions':{run:ctx.editable&&destPicked?()=>{ctx.show('hyperlinks');setDestForm({mode:'edit',id:pickedDest});}:undefined},
    'id.panel.hyperlinks.deleteUnusedDestinations':{run:ctx.editable&&data.destinations.length?()=>void ctx.edit(tr('안 쓰는 대상 삭제'),[{op:'deleteUnusedHyperlinkDestinations'}]):undefined},
    'id.panel.hyperlinks.resetHyperlink':{run:ctx.editable&&plain&&selected&&selected.start!==selected.end
      ?()=>void ctx.edit(tr('하이퍼링크 재설정'),[{op:'resetHyperlink',id:plain.id,story:selected.story,start:selected.start,end:selected.end}]):undefined},
  };
  const panel=(cross:boolean)=><section className="hyperlinks-panel" aria-label={cross?tr('상호 참조'):tr('하이퍼링크')}>
    {shown(cross).length===0&&<p className="p-empty">{cross?tr('상호 참조가 없습니다. 글에 커서를 두고 새로 만들기를 누르세요.'):tr('하이퍼링크가 없습니다. 글을 고르고 새로 만들기를 누르세요.')}</p>}
    <div className="hyperlinks-list" role="listbox" aria-label={tr('링크 목록')}>{shown(cross).map(l=>{const a=cross?undefined:urlStatus[urlOf(l)];
      return <button key={l.id} role="option" aria-selected={active?.id===l.id} onClick={()=>pick(l.id)} onDoubleClick={()=>open(isCross(l),l)} data-url-status={a?.kind}
        title={a?`${urlOf(l)} — ${statusText(a)}`:undefined}>{l.name}{a&&<span className={`url-status url-${a.kind==='ok'?'ok':a.kind==='notChecked'?'skip':'bad'}`} aria-label={statusText(a)}>{a.kind==='ok'?' ●':a.kind==='notChecked'?' ○':' ✕'}</span>}</button>;})}</div>
    <div className="hyperlinks-actions"><button disabled={!ctx.editable||!selected||(!cross&&selected.start===selected.end)} onClick={()=>open(cross)}>{tr('새로 만들기')}</button>
      <button disabled={!options} onClick={options}>{tr('옵션…')}</button><button disabled={!remove} onClick={remove}>{tr('삭제')}</button>
      <button disabled={!go} onClick={go}>{tr('대상으로 이동')}</button>{cross&&<button disabled={!update} onClick={update}>{tr('업데이트')}</button>}
      {!cross&&<button data-check-urls disabled={!checkUrls} title={tr('URL 대상의 웹 주소에 지금 접속해 응답을 확인합니다(인터넷 연결을 씁니다).')} onClick={checkUrls?()=>void checkUrls():undefined}>{checking?tr('확인하는 중…'):tr('웹 주소 상태 확인')}</button>}</div>
    {!cross&&plain&&urlStatus[urlOf(plain)]&&<p className="url-status-line" role="status">{urlOf(plain)}: {statusText(urlStatus[urlOf(plain)])}</p>}
    {cross&&<button disabled={!ctx.editable} onClick={openFormats}>{tr('형식 편집…')}</button>}
    {!cross&&<DestinationsBlock data={data} editable={ctx.editable} caret={selected?{story:selected.story,at:selected.start}:null} form={destForm} setForm={setDestForm}
      picked={pickedDest} pick={pickDest} edit={ctx.edit} pageCount={ctx.info?.pages.length??0}/>}
  </section>;
  return {commands,panel};
}
