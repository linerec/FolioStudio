import {useState} from 'react';
import {tr} from '../../i18n';
import type {EditCommand,EditResult} from '../../engine';
import type {Command,Commands} from '../../ui/commands';
import {Flyout} from '../../ui/flyout';
import {PANEL_MENUS} from '../../ui/menudef';

type Asset={id:string;name:string;description:string;path:string;sha256:string;kind?:string;label?:string};
type Library={name:string;assets:Asset[]};
const KIND:Record<string,()=>string>={geometry:()=>tr('도형'),text:()=>tr('글'),image:()=>tr('그림'),pdf:()=>tr('PDF'),eps:()=>tr('EPS'),page:()=>tr('페이지'),structure:()=>tr('구조'),indesignFile:()=>tr('InDesign 파일')};
const SORTS:[string,()=>string][]=[['newest',()=>tr('최신순')],['oldest',()=>tr('오래된 순')],['name',()=>tr('이름순')],['type',()=>tr('종류순')]];
/** File ▸ New ▸ Library…: the engine's Save dialog (next/shared/dialogs/newLibrary.json: a folder of the project, the file name, Replace?) makes an
 *  empty library file in the project; `made` opens the library it answers (the window below, empty), `openExisting` the window to open one. */
/** `pageIds`: the objects of the page on view (Add Items on Page); `getCommands`: the live command table the panel menu reads. */
export function useLibrary({root,editable,ids,page,pageIds=[],text,edit,getCommands=()=>({})}:{root?:string;editable:boolean;ids:string[];page?:string;pageIds?:string[];text?:{story:string;start:number;end:number}|null;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;getCommands?:()=>Commands}){
  // `panel`: the open library (or the row that opens an existing one)
  const [phase,setPhase]=useState<'closed'|'panel'>('closed'),[path,setPath]=useState(''),[assetName,setAssetName]=useState(''),[description,setDescription]=useState(''),[label,setLabel]=useState(''),[selected,setSelected]=useState(''),[replacement,setReplacement]=useState('');
  const [search,setSearch]=useState(''),[sort,setSort]=useState('newest');
  const [library,setLibrary]=useState<Library|null>(null),[view,setView]=useState<string[]>([]),[missing,setMissing]=useState<string[]>([]),[error,setError]=useState(''),[busy,setBusy]=useState(false),[notice,setNotice]=useState('');
  const request=async(action:string,extra:Record<string,unknown>={})=>{
    setBusy(true);setError('');
    try{
      const {header}=await window.folio.request({cmd:'library',root,path,action,search,sort,...extra});
      if(!header.ok)throw Error(header.error==='library_external_references'?tr('그림 원본을 모두 찾을 수 있어야 등록할 수 있습니다.'):String(header.error));
      if(header.command){await edit(tr('라이브러리 배치'),[header.command as EditCommand]);}
      else{setLibrary(header.library as Library);setView((header.view as string[]|undefined)??[]);setMissing((header.status as {id:string;ok:boolean}[]).filter(s=>!s.ok).map(s=>s.id));if(header.created)setSelected(String(header.created));}
    }catch(e){setError(String(e));}finally{setBusy(false);}
  };
  const pick=(id:string)=>{setSelected(id);const a=library?.assets.find(x=>x.id===id);if(a){setAssetName(a.name);setDescription(a.description);setLabel(a.label??'');}};
  // the Save dialog's answer (the engine made the file: `library` action `new`): the library opens, empty
  const made=(header:Record<string,unknown>)=>{setPath(String(header.path));setLibrary(header.library as Library);setView((header.view as string[]|undefined)??[]);setMissing([]);setSelected('');setSearch('');setError('');setPhase('panel');};
  // the Library panel menu (InDesign's, menutree PANEL_MENUS.library): each entry does what this window's controls do
  const focus=(label:string)=>{window.setTimeout(()=>document.querySelector<HTMLElement>(`[role=dialog] [aria-label="${label}"]`)?.focus(),0);};
  const open=phase==='panel'&&!!library,asset=library?.assets.find(a=>a.id===selected),usable=!!selected&&!missing.includes(selected);
  const fresh=(f:()=>void)=>()=>{setNotice('');f();};
  const sorted=(value:string)=>({run:open?fresh(()=>{setSort(value);void request('read',{sort:value});}):undefined,checked:sort===value});
  // Add Items on Page as Separate Objects: one library item per object; Update Library Item: the selected objects take the place of the item (the new item is stored first, then the old one goes, so an interruption leaves both)
  const storeEach=async()=>{setBusy(true);setError('');try{for(const id of pageIds){const {header}=await window.folio.request({cmd:'library',root,path,action:'store',ids:[id],search,sort});if(!header.ok)throw Error(header.error==='library_external_references'?tr('그림 원본을 모두 찾을 수 있어야 등록할 수 있습니다.'):String(header.error));}}catch(e){setError(String(e));}finally{setBusy(false);}await request('read');};
  const replaceItem=async()=>{if(!asset)return;setBusy(true);setError('');try{
    const stored=(await window.folio.request({cmd:'library',root,path,action:'store',ids,name:asset.name,description:asset.description,label:asset.label??'',search,sort})).header;
    if(!stored.ok)throw Error(stored.error==='library_external_references'?tr('그림 원본을 모두 찾을 수 있어야 등록할 수 있습니다.'):String(stored.error));
    const removed=(await window.folio.request({cmd:'library',root,path,action:'remove',asset:asset.id,search,sort})).header;if(!removed.ok)throw Error(String(removed.error));
    setSelected('');
  }catch(e){setError(String(e));}finally{setBusy(false);}await request('read');};
  const handlers:Record<string,Partial<Command>>={'id.file.new.library':{enabled:!!root,why:root?undefined:tr('라이브러리 파일은 프로젝트 폴더 안에 만듭니다. 프로젝트를 먼저 여세요.'),run:()=>void window.folio.request({cmd:'openDialog',dialog:'newLibrary',values:{project:root,library:library?path:'',sort,fileName:tr('라이브러리')+'.foliolibrary'}})},
    'id.panel.library.closeLibrary':{run:open?()=>setPhase('closed'):undefined},
    'id.panel.library.addItem':{run:open&&editable&&ids.length?fresh(()=>void request('store',{ids,...(assetName.trim()?{name:assetName,description,label}:{})})):undefined},
    'id.panel.library.addItemsOnPage':{run:open&&editable&&pageIds.length?fresh(()=>void request('store',{ids:pageIds,...(assetName.trim()?{name:assetName,description,label}:{})})):undefined},
    'id.panel.library.addItemsOnPageAsSeparateObjects':{run:open&&editable&&pageIds.length?fresh(()=>void storeEach()):undefined},
    'id.panel.library.placeItemS':{run:open&&editable&&page&&usable?fresh(()=>void request('place',{asset:selected,page})):undefined},
    'id.panel.library.deleteItemS':{run:open&&selected?fresh(()=>void request('remove',{asset:selected})):undefined},
    'id.panel.library.updateLibraryItem':{run:open&&editable&&asset&&ids.length?fresh(()=>void replaceItem()):undefined},
    'id.panel.library.itemInformation':{run:open&&selected?fresh(()=>focus(tr('개체 이름'))):undefined},
    'id.panel.library.showSubset':{run:open?fresh(()=>focus(tr('라이브러리 검색'))):undefined},
    'id.panel.library.showAll':{run:open?fresh(()=>{setSearch('');void request('read',{search:''});}):undefined},
    'id.panel.library.listView':{run:open?()=>setNotice(tr('라이브러리는 목록으로 보입니다. 섬네일 보기는 지원하지 않습니다.')):undefined,checked:true},
    'id.panel.library.sortItems.byName':sorted('name'),'id.panel.library.sortItems.byNewest':sorted('newest'),'id.panel.library.sortItems.byOldest':sorted('oldest'),'id.panel.library.sortItems.byType':sorted('type'),
  };
  const openExisting=()=>{setLibrary(null);setError('');setPhase('panel');};
  // a library of the project chosen in the explorer (project-relative): the window opens with it read
  const openFile=(rel:string)=>{setPath(rel);setLibrary(null);setSelected('');setError('');setPhase('panel');void request('read',{path:rel});};
  const shown=library?view.map(id=>library.assets.find(a=>a.id===id)).filter((a):a is Asset=>!!a):[];
  return {handlers,made,openExisting,openFile,dialog:phase==='panel'&&<div className="style-modal" role="dialog" aria-label={tr('라이브러리')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>e.stopPropagation()}><div className="card dialog style-options" style={{width:620,padding:20,maxHeight:'90vh',overflow:'auto'}}>
    <div style={{display:'flex',alignItems:'center',justifyContent:'space-between'}}><h2>{tr('라이브러리')}</h2><Flyout items={PANEL_MENUS.library??[]} getCommands={getCommands} label={tr('라이브러리 패널 메뉴')}/></div><p>{tr('프로젝트 안에 로컬 라이브러리를 저장합니다. Folio 형식(.foliolibrary)이며 INDB/INDL 네이티브 호환과 클라우드 라이브러리는 지원하지 않습니다.')}</p>
    <p className="differs-note" data-note="library">{tr('돌리거나 뒤집은 개체를 글 안에 놓는 것과 구조·페이지·InDesign 파일 항목은 지원하지 않습니다.')}</p>
    <p><label>{tr('프로젝트 상대 경로')} <input aria-label={tr('프로젝트 상대 경로')} value={path} onChange={e=>{setPath(e.target.value);setLibrary(null);}}/></label><button disabled={busy||!path} onClick={()=>void request('read')}>{tr('라이브러리 열기')}</button></p>
    {library&&<><h3>{library.name}</h3>
      <p><label>{tr('라이브러리 검색')} <input aria-label={tr('라이브러리 검색')} value={search} onChange={e=>{setSearch(e.target.value);void request('read',{search:e.target.value});}}/></label>
        <label>{tr('라이브러리 정렬')} <select aria-label={tr('라이브러리 정렬')} value={sort} onChange={e=>{setSort(e.target.value);void request('read',{sort:e.target.value});}}>{SORTS.map(([v,l])=><option key={v} value={v}>{l()}</option>)}</select></label></p>
      <select size={6} aria-label={tr('라이브러리 개체')} style={{width:'100%'}} value={selected} onChange={e=>pick(e.target.value)}>{shown.map(a=><option key={a.id} value={a.id}>{a.name}{a.kind&&KIND[a.kind]?` (${KIND[a.kind]()})`:''}{missing.includes(a.id)?' ⚠':''}</option>)}</select>
      <p><label>{tr('개체 이름')} <input aria-label={tr('개체 이름')} value={assetName} onChange={e=>setAssetName(e.target.value)}/></label></p>
      <p><label>{tr('라이브러리 개체 설명')} <input aria-label={tr('라이브러리 개체 설명')} value={description} onChange={e=>setDescription(e.target.value)}/></label></p>
      <p><label>{tr('라이브러리 개체 레이블')} <input aria-label={tr('라이브러리 개체 레이블')} value={label} onChange={e=>setLabel(e.target.value)}/></label></p>
      <button disabled={busy||!editable||!ids.length||!assetName.trim()} onClick={()=>void request('store',{ids,name:assetName,description,label})}>{tr('선택 개체 등록')}</button>
      <button disabled={busy||!selected||!assetName.trim()} onClick={()=>void request('update',{asset:selected,name:assetName,description,label})}>{tr('항목 정보 바꾸기')}</button>
      <button disabled={busy||!editable||!page||!selected||missing.includes(selected)} onClick={()=>void request('place',{asset:selected,page})}>{tr('라이브러리 배치')}</button>
      <button disabled={busy||!editable||!text||!selected||missing.includes(selected)} title={tr('글 안의 커서 자리나 고른 글자 자리에 인라인 개체로 놓습니다. 여러 개체를 담은 항목은 놓을 수 없습니다.')} onClick={()=>void request('place',{asset:selected,story:text?.story,start:text?.start,end:text?.end})}>{tr('글 안에 놓기')}</button>
      <button disabled={busy||!selected} onClick={()=>void request('remove',{asset:selected})}>{tr('삭제')}</button>
      <p><input aria-label={tr('다시 연결할 상대 경로')} value={replacement} onChange={e=>setReplacement(e.target.value)}/><button disabled={busy||!selected||!replacement} onClick={()=>void request('relink',{asset:selected,replacement})}>{tr('다시 연결')}</button></p>
      <p>{tr('선택한 개체가 여러 개이면 하나의 항목으로 등록하고, 배치하면 그대로 여러 개가 놓입니다. 같은 이름의 색상과 스타일은 배치하는 문서의 것을 쓰고, 문서에 없는 스타일은 더해집니다.')}</p>
      <p>{tr('링크 그림은 프로젝트 안에 복사합니다. 같은 이름의 다른 그림은 내용별로 따로 보관합니다.')}</p>
    </>}
    {notice&&<p role="status">{notice}</p>}{error&&<p role="alert">{error}</p>}<footer><button disabled={busy} onClick={()=>setPhase('closed')}>{tr('닫기')}</button></footer>
  </div></div>};
}
