import { useEffect, useRef, useState } from 'react';
import type { DocumentSummary, EditCommand, EditResult, PageInfo } from '../engine';
import type { DocumentInfo } from '../inspect';
import { tr } from '../i18n';
import type { Command } from './commands';
import { commandKey } from './keys';
import { queue } from './docview';
import { Icon } from './icons';
import type { PanelCtx } from './panels';
import { NumberField } from './controls/numberfield';
import { formatInteger, parseInteger } from './controls/lengthfield';
import './pages.css';
import { ParentLoadingDialog } from './parentloading';
import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';

type Dialog = 'insert' | 'move' | 'master' | 'section' | 'adjust' | 'delete' | 'newParent' | 'parentOptions' | 'flattener' | 'panelOptions' | 'loadParents';
type Input = {
  doc: DocumentSummary | null; info: DocumentInfo | null; editable: boolean;
  current: number; selectedPage: number | null; setSelectedPage: (p: number | null) => void;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};
export type PageEditing = ReturnType<typeof usePageEditing>;

/** Pages panel ▸ View Pages: spreads in a column (InDesign's default) or in rows (this computer only). */
const PAGES_VIEW = 'folio.pagesPanel.view';

/** Pages panel ▸ Panel Options: page thumbnail height (px) by size, parent icon zoom by size, which area is on top (this computer only). */
const PAGES_OPTIONS = 'folio.pagesPanel.options';
export const PAGE_SIZES = { xs: 36, s: 48, m: 62, l: 84, xl: 112 } as const;
export const PARENT_ZOOM = { s: 0.75, m: 1, l: 1.4 } as const;
export type PanelOptions = { pageSize: keyof typeof PAGE_SIZES; parentSize: keyof typeof PARENT_ZOOM; top: 'parents' | 'pages' };
const DEFAULT_OPTIONS: PanelOptions = { pageSize: 'm', parentSize: 'm', top: 'parents' };
function loadOptions(value: string | null): PanelOptions {
  try {
    const o = JSON.parse(value ?? '{}');
    return { pageSize: Object.hasOwn(PAGE_SIZES, o.pageSize) ? o.pageSize : 'm', parentSize: Object.hasOwn(PARENT_ZOOM, o.parentSize) ? o.parentSize : 'm', top: o.top === 'pages' ? 'pages' : 'parents' };
  } catch { return DEFAULT_OPTIONS; }
}

/** All page operations share the menu, panel and keyboard command path. */
export function usePageEditing(input: Input) {
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [selection, setSelection] = useState<string[]>([]);
  /** A parent picked in the panel (Duplicate, Delete and Parent Options act on it). */
  const [parents, setParents] = useState<string[]>([]);
  const layout = useUserPreference(PAGES_VIEW) === 'horizontal' ? 'horizontal' : 'vertical';
  const opts = loadOptions(useUserPreference(PAGES_OPTIONS));
  const { doc, info, editable, selectedPage, current, edit } = input;
  const pages = doc?.pages ?? [];
  const idOf = (p: PageInfo) => p.id ?? info?.pages[p.index]?.id ?? '';
  const primary = pages[Math.min(selectedPage ?? current, pages.length - 1)];
  const selected = selection.filter(id => pages.some(p => idOf(p) === id));
  const ids = selected.length ? selected : primary ? [idOf(primary)] : [];
  /** The spreads of the selected pages (Hide Spread, Show Parent Items, Spread Flattening). */
  const selSpreads = [...new Set(ids.map(id=>pages.find(p=>idOf(p)===id)?.spread).filter((s):s is string=>!!s))];
  useEffect(() => { setSelection([]); setDialog(null); setParents([]); }, [doc?.path]);
  const chosenParents = parents.filter((id) => info?.masters.some((m) => m.id === id));
  const chosenParent = chosenParents[0] ?? null;
  const open = (d: Dialog | 'setup') => {
    if (!editable) return;
    // File ▸ Document Setup…: the engine session holds the dialog (next/shared/dialogs.json)
    if (d === 'setup') void window.folio.request({ cmd: 'openDialog', dialog: 'documentSetup' }); else setDialog(d);
  };
  // Layout ▸ Margins and Columns… / Create Guides…: the engine session holds the dialog (next/shared/dialogs.json); Margins and Columns takes
  // every selected page, Create Guides the first one's spread
  const openLayout = (dialog: 'margins' | 'createGuides') => { if (editable && ids.length) void window.folio.request({ cmd: 'openDialog', dialog, ids: dialog === 'margins' ? ids : [ids[0]] }); };
  const select = (p: PageInfo, extend: boolean, range: boolean) => {
    setParents([]);
    const id = idOf(p);
    if (range && primary) {
      const [a,b] = [primary.index,p.index].sort((a,b)=>a-b);
      setSelection(pages.slice(a,b+1).map(idOf));
    } else if (extend) setSelection(ids.includes(id) ? ids.filter(k=>k!==id) : [...ids,id]);
    else setSelection([id]);
    input.setSelectedPage(p.index);
  };
  const run = (label: string, commands: EditCommand[]) => editable ? edit(label, commands) : Promise.resolve(null);
  const move = (moving: string[], before?: string) => {
    if (before && moving.includes(before)) return;
    void run(tr('페이지 이동'), [{ op:'movePages',pages:moving,before }]);
  };
  const apply = (master: string | null, target = ids) => void run(tr('부모 페이지 적용'),[{op:'applyMaster',pages:target,master}]);
  const spread = primary?.spread;
  const handlers: Record<string,Partial<Command>> = {
    'id.panel.pages.parentPages.saveAsParent': { run: () => void run(tr('상위 페이지로 저장'), selSpreads.map(spread => ({ op: 'saveAsParent', spread }))), enabled: editable && !chosenParent && selSpreads.length > 0 },
    'id.panel.pages.parentPages.loadParentPages': { run: () => open('loadParents'), enabled: editable },
    'layout.addPage': { run:()=>void run(tr('페이지 추가'),[{op:'insertPages',count:1,before:primary ? pages[primary.index+1]?.id : undefined,master:info?.pages[primary?.index??0]?.master}]) },
    'layout.insertPages':{run:()=>open('insert')},
    'layout.movePages':{run:()=>open('move')},
    'layout.deletePages':{run:()=>open('delete'),enabled:editable&&ids.length<pages.length},
    'layout.numbering':{run:()=>open('section')},
    'layout.margins':{run:()=>openLayout('margins')},
    'layout.createGuides':{run:()=>openLayout('createGuides')},
    'file.documentSetup':{run:()=>open('setup')},
    'layout.adjust':{run:()=>open('adjust'),enabled:editable},
    'id.layout.pages.applyParentToPages':{run:()=>open('master')},
    'id.layout.pages.duplicateSpread':{run:()=>{if(chosenParent)void run(tr('부모 펼침면 복제'),chosenParents.map(id=>({op:'duplicateParent' as const,id})));else if(spread)void run(tr('펼침면 복제'),[{op:'duplicateSpread',spread}]);}},
    'id.panel.pages.deleteSpread':{run:()=>{if(chosenParent){setParents([]);void run(tr('부모 펼침면 삭제'),chosenParents.map(id=>({op:'deleteParent' as const,id})));return;}setSelection(pages.filter(p=>p.spread===spread).map(idOf));open('delete');},enabled:editable&&(!!chosenParent||pages.some(p=>p.spread!==spread))},
    // Hide Spread / Show Spread: the selected pages' spreads, hidden unless the first is (oracle pages-hide-spread)
    'id.layout.pages.hideSpread':(()=>{
      const spreads=selSpreads;
      const hidden=!!spreads.length&&spreads.every(s=>doc?.pageSettings?.hidden?.[s]);
      return {run:()=>void run(hidden?tr('펼침면 표시'):tr('펼침면 숨기기'),[{op:'setSpreadHidden',spreads,hidden:!hidden}]),label:hidden?tr('펼침면 표시'):undefined};
    })(),
    // View ▸ Rotate Spread and Page Attributes ▸ Rotate Spread View (one command): the spread of the selected page turns
    // further, or back upright; Clear Rotation is off for an upright spread (oracle spreadrotate)
    ...(()=>{
      const turn=spread?doc?.pageSettings?.rotation?.[spread]??0:0;
      const rotate=(how:'cw'|'ccw'|'180'|'clear',on=true)=>({run:()=>{if(spread)void run(tr('펼침면 보기 회전'),[{op:'rotateSpreadView',spread,how}]);},enabled:editable&&!!spread&&on});
      return {
        'id.view.rotateSpread.90DegCw':rotate('cw'),
        'id.view.rotateSpread.90DegCcw':rotate('ccw'),
        'id.view.rotateSpread.180Deg':rotate('180'),
        'id.view.rotateSpread.clearRotation':rotate('clear',turn!==0),
      };
    })(),
    // Parent items on the selected pages (Ctrl+Shift+click overrides one; oracle pages-override)
    ...(()=>{
      const withParent=ids.filter(id=>info?.pages.find(p=>p.id===id)?.master);
      const each=(label:string,op:'overrideParentItems'|'removeLocalOverrides'|'detachFromParent')=>({run:()=>void run(label,withParent.map(page=>({op,page}))),enabled:editable&&withParent.length>0});
      return {
        'id.panel.pages.overrideAllParentPageItems':each(tr('모든 상위 페이지 항목 재정의'),'overrideParentItems'),
        'id.panel.pages.parentPages.removeAllLocalOverrides':each(tr('모든 로컬 재정의 제거'),'removeLocalOverrides'),
        'id.panel.pages.parentPages.detachAllObjectsFromParent':each(tr('상위 페이지에서 모든 개체 분리'),'detachFromParent'),
      };
    })(),
    ...(()=>{
      // Page Attributes ▸ Color Label: the selected pages', or the chosen parent's
      const ps=doc?.pageSettings;
      const now=chosenParent?(ps?.parentColors?.[chosenParent]??'UseMasterColor'):ids.length?(ps?.pageColors?.[ids[0]]??'UseMasterColor'):'';
      const colors:Record<string,string>={none:'Nothing',useParentColor:'UseMasterColor',white:'White',pink:'Pink',gold:'Gold',yellow:'Yellow',green:'Green',lightBlue:'LightBlue',lavender:'Lavender',gray:'Gray',red:'Red',orange:'Orange',tan:'Tan',darkGreen:'DarkGreen',blue:'Blue',violet:'Violet',black:'Black'};
      const out:Record<string,Partial<Command>>={};
      for(const [k,color] of Object.entries(colors))out[`id.panel.pages.pageAttributes.colorLabel.${k}`]={run:()=>void run(tr('색상 레이블'),[{op:'setPageColor',...(chosenParent?{masters:chosenParents}:{pages:ids}),color}]),checked:now===color,enabled:editable&&(!!chosenParent||ids.length>0)};
      // Parent Pages ▸ Show Parent Items, Page Attributes ▸ Spread Flattening: for the spreads of the selected pages
      const spreads=selSpreads;
      const hiding=!!spreads.length&&spreads.every(s=>ps?.hideParentItems?.[s]);
      out['id.panel.pages.parentPages.showParentItems']={run:()=>void run(tr('상위 항목 표시'),[{op:'setSpreadShowMaster',spreads,show:hiding}]),checked:!hiding,enabled:editable&&spreads.length>0};
      const level=spreads.length?ps?.flattener?.[spreads[0]]?.level??'Default':'';
      const flatten=(l:'Default'|'None')=>({run:()=>void run(tr('펼침면 병합'),[{op:'setSpreadFlattener',spreads,level:l}]),checked:level===l,enabled:editable&&spreads.length>0});
      out['id.panel.pages.pageAttributes.spreadFlattening.default']=flatten('Default');
      out['id.panel.pages.pageAttributes.spreadFlattening.noneIgnoreTransparency']=flatten('None');
      out['id.panel.pages.pageAttributes.spreadFlattening.custom']={run:()=>open('flattener'),checked:level==='Custom',enabled:editable&&spreads.length>0};
      return out;
    })(),
    'id.panel.pages.newParent':{run:()=>open('newParent')},
    'id.panel.pages.parentOptions':{run:()=>open('parentOptions'),enabled:editable&&chosenParents.length===1},
    'id.panel.pages.allowDocumentPagesToShuffle':{run:()=>void run(tr('문서 페이지 섞기'),[{op:'setPageShuffle',allowed:!doc?.pageSettings?.allowShuffle}]),checked:doc?.pageSettings?.allowShuffle??true},
    'id.panel.pages.allowSelectedSpreadToShuffle':{run:()=>{if(spread)void run(tr('펼침면 섞기'),[{op:'setPageShuffle',spread,allowed:!(doc?.pageSettings?.spreads[spread]??true)}]);},checked:spread?doc?.pageSettings?.spreads[spread]??true:false},
  };
  for(const h of Object.values(handlers)) h.enabled=(h.enabled??true)&&editable&&!!primary;
  const view = (v: 'vertical' | 'horizontal') => () => userSettings.setItem(PAGES_VIEW, v);
  // Parent Pages ▸ Select Unused Parents: every parent no page uses is picked in the panel (selection only; Delete Spread then removes them)
  const unused=(info?.masters??[]).filter(m=>!info?.pages.some(p=>p.master===m.id)).map(m=>m.id);
  handlers['id.panel.pages.parentPages.selectUnusedParents']={run:()=>{setSelection([]);setParents(unused);},enabled:!!doc&&unused.length>0};
  handlers['id.panel.pages.panelOptions']={run:()=>setDialog('panelOptions')};
  // Pages Panel Section Context Menu ▸ Delete Section: the section that starts on the selected page; its pages join the section before it. The document's first section is the default one and stays (InDesign: "Cannot delete the default section").
  {const sections=doc?.pageSettings?.sections??[],at=primary?sections.findIndex(s=>s.page===primary.id):-1;
   handlers['id.ctx.pagesSection.deleteSection']={run:()=>{if(primary?.id)void run(tr('구역 지우기'),[{op:'deleteSection',page:primary.id}]);},enabled:editable&&at>0,why:at===0?tr('문서의 첫 구역은 지울 수 없습니다'):undefined};}
  handlers['id.panel.pages.viewPages.horizontally']={run:view('horizontal'),checked:layout==='horizontal'};
  handlers['id.panel.pages.viewPages.vertically']={run:view('vertical'),checked:layout==='vertical'};
  const view2=(v:'vertical'|'horizontal')=>view(v)();
  const setOptions = (o: PanelOptions) => userSettings.setItem(PAGES_OPTIONS, JSON.stringify(o));
  return {ids,idOf,select,move,apply,open,handlers,layout,opts,parent:chosenParent,parents:chosenParents,selectParent:(id:string|null)=>setParents(id?[id]:[]),
    dialog:dialog&&doc&&info ? (dialog==='loadParents' ? <ParentLoadingDialog edit={run} close={()=>setDialog(null)}/>
      : dialog==='newParent'||dialog==='parentOptions'
      ? <ParentDialog key={`${dialog}:${chosenParent}`} kind={dialog} info={info} parent={chosenParent} edit={run} onClose={()=>setDialog(null)}/>
      : dialog==='panelOptions'
      ? <PanelOptionsDialog opts={opts} layout={layout} onApply={(o,l)=>{setOptions(o);view2(l);}} onClose={()=>setDialog(null)}/>
      : dialog==='flattener'
      ? <FlattenerDialog key={ids.join(',')} spreads={selSpreads} doc={doc} edit={run} onClose={()=>setDialog(null)}/>
      : <PageDialog key={`${dialog}:${primary?.id}`} kind={dialog} doc={doc} info={info} ids={ids} primary={primary} edit={run} onClose={()=>setDialog(null)}/>) : null};
}

export function PagesPanel({ctx}:{ctx:PanelCtx}) {
  const {doc,info,pageEditing:actions}=ctx;
  const [drop,setDrop]=useState<string|null>(null);
  // what the drag over a page would do (uix-b4, ui/motion.css): pages go before it (a line), a parent goes onto it (a ring)
  const [dropKind,setDropKind]=useState<'before'|'into'>('before');
  if(!doc||!actions)return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  const rows:PageInfo[][]=[];
  for(const p of doc.pages){const last=rows[rows.length-1];if(last?.[0].spread===p.spread)last.push(p);else rows.push([p]);}
  const masterOf=new Map((info?.masters??[]).map(m=>[m.id,m.prefix]));
  const dragMaster=(e:React.DragEvent,id:string)=>{e.dataTransfer.setData('application/x-folio-parent',id);e.dataTransfer.effectAllowed='copy';};
  const receive=(e:React.DragEvent,page?:PageInfo)=>{
    e.preventDefault();setDrop(null);if(!ctx.editable)return;
    const master=e.dataTransfer.getData('application/x-folio-parent');
    if(master&&page)actions.apply(master==='none'?null:master,[actions.idOf(page)]);
    else {try{const ids=JSON.parse(e.dataTransfer.getData('application/x-folio-pages'));if(Array.isArray(ids)&&ids.every(x=>typeof x==='string'))actions.move(ids,page?actions.idOf(page):undefined);}catch{/* Other apps' drags are not page moves. */}}
  };
  const o=actions.opts,psize=PAGE_SIZES[o.pageSize];
  // right-click (InDesign's Pages Panel context menus, menutree CONTEXT_MENUS pagesPages / pagesPanel): on a page or parent the item is picked first
  const menu=(kind:'pagesPages'|'pagesPanel'|'pagesSection',e:React.MouseEvent)=>{e.preventDefault();e.stopPropagation();ctx.openContext?.(kind,{x:e.clientX,y:e.clientY});};
  return <div className={o.top==='pages'?'pages-panel pp-pages-top':'pages-panel'} style={{'--pp-zoom':PARENT_ZOOM[o.parentSize]} as React.CSSProperties} onContextMenu={e=>menu('pagesPanel',e)}
    data-drop-kind={drop?dropKind:undefined} onDragEnd={()=>setDrop(null)}>
    <div className="pp-masters">
      <button className="pp-master" draggable={ctx.editable} onDragStart={e=>dragMaster(e,'none')} onDoubleClick={()=>actions.apply(null)} title={tr('선택한 쪽에 적용하거나 쪽으로 끌어 놓으세요')}>
        <span className="pp-micon none"/><span>{tr('[없음]')}</span>
      </button>
      {(info?.masters??[]).map(m=><button className={(actions.parents.includes(m.id)?'pp-master on':'pp-master')+(ctx.openedMaster===m.id?' open':'')} key={m.id} data-parent={m.id} draggable={ctx.editable} onDragStart={e=>dragMaster(e,m.id)}
        onClick={()=>actions.selectParent(m.id)} onContextMenu={e=>{if(!actions.parents.includes(m.id))actions.selectParent(m.id);menu('pagesPages',e);}} onDoubleClick={()=>ctx.openMaster?.(m.id)} title={tr('눌러 고르기 · 두 번 눌러 열기 · 쪽으로 끌어 적용')}>
        <span className="pp-micon">{Array.from({length:Math.min(10,m.pages)},(_,i)=><i key={i}>{m.prefix}</i>)}</span>{labelOf(doc.pageSettings,undefined,m.id)&&<i className="pp-label" style={{background:labelOf(doc.pageSettings,undefined,m.id)}}/>}<span>{m.name}</span>
      </button>)}
    </div>
    <div className="pp-split"/>
    <div className={actions.layout==='horizontal'?'pp-spreads horizontal':'pp-spreads'}>
      {rows.map(row=><div className="pp-spread" key={row[0].spread}>
        <div className="pp-thumbs">{row.map(p=>{
          const id=actions.idOf(p);
          const sec=doc.pageSettings?.sections.find(s=>s.page===id);
          return <PageThumb key={id} size={psize} page={p} rev={ctx.pageRev?.get(p.index)??0} prefix={masterOf.get(info?.pages[p.index]?.master??'')??''} label={labelOf(doc.pageSettings,id,info?.pages[p.index]?.master)}
            section={sec?(sec.marker||sec.prefix):undefined} onSectionMenu={e=>{if(!actions.ids.includes(id))actions.select(p,false,false);menu('pagesSection',e);}}
            selected={actions.ids.includes(id)} drop={drop===id} editable={ctx.editable}
            onClick={e=>actions.select(p,commandKey(e),e.shiftKey)} onDoubleClick={()=>ctx.goToPage(p.index)}
            onContextMenu={e=>{if(!actions.ids.includes(id))actions.select(p,false,false);menu('pagesPages',e);}}
            onDragStart={e=>{const ids=actions.ids.includes(id)?actions.ids:[id];e.dataTransfer.setData('application/x-folio-pages',JSON.stringify(ids));e.dataTransfer.effectAllowed='move';}}
            onDragOver={e=>{if(ctx.editable){e.preventDefault();setDrop(id);setDropKind(e.dataTransfer.types.includes('application/x-folio-parent')?'into':'before');}}}
            onDragLeave={e=>{const to=e.relatedTarget;if(to instanceof Node&&e.currentTarget.contains(to))return;setDrop(d=>d===id?null:d);}} onDrop={e=>receive(e,p)} />;
        })}</div>
        <span className={(row.some(p=>p.index===ctx.current)?'pp-num on':'pp-num')+(doc.pageSettings?.hidden?.[row[0].spread]?' hidden':'')} title={doc.pageSettings?.hidden?.[row[0].spread]?tr('숨긴 펼침면: PDF에 나오지 않습니다'):undefined}>
          {doc.pageSettings?.spreads[row[0].spread]===false?'[':''}{row.length>1?`${row[0].name}–${row.at(-1)!.name}`:row[0].name}{doc.pageSettings?.spreads[row[0].spread]===false?']':''}
        </span>
      </div>)}
      <div className={`pp-drop-end${drop==='end'?' on':''}`} onDragOver={e=>{if(ctx.editable){e.preventDefault();setDrop('end');}}} onDragLeave={()=>setDrop(null)} onDrop={e=>receive(e)}>{tr('끝으로 이동')}</div>
    </div>
    <div className="p-foot">
      <span className="p-foot-text">{tr('{0}페이지, {1}펼침면',{0:doc.pages.length,1:rows.length})}</span>
      <button className="p-foot-btn" disabled={!ctx.editable} title={tr('문서 설정…')} onClick={()=>actions.open('setup')}><Icon name="pages" size={15}/></button>
      <button className="p-foot-btn" disabled={!ctx.editable} title={tr('페이지 삽입…')} onClick={()=>actions.open('insert')}><Icon name="newPage" size={15}/></button>
      <button className="p-foot-btn" disabled={!ctx.editable||actions.ids.length===doc.pages.length} title={tr('선택한 페이지 삭제')} onClick={()=>actions.open('delete')}><Icon name="trash" size={15}/></button>
    </div>
  </div>;
}
/** The colour of a page's (or, without a page, a parent's) Color Label: its own, else its parent's; none when "Nothing". */
function labelOf(settings:DocumentSummary['pageSettings'],page:string|undefined,master:string|null|undefined):string|undefined{
  const own=page?settings?.pageColors?.[page]:settings?.parentColors?.[master??''];
  const name=page&&!own?settings?.parentColors?.[master??'']:own;
  const rgb=name&&name!=='Nothing'?settings?.labelColors?.[name]:undefined;
  return rgb?`rgb(${rgb[0]},${rgb[1]},${rgb[2]})`:undefined;
}
type ThumbProps = {
  page:PageInfo;size:number;rev:number;prefix:string;label?:string;section?:string;onSectionMenu?:React.MouseEventHandler;selected:boolean;drop:boolean;editable:boolean;
  onClick:React.MouseEventHandler;onDoubleClick:()=>void;
  onDragStart:React.DragEventHandler;onDragOver:React.DragEventHandler;onDragLeave?:React.DragEventHandler;onDrop:React.DragEventHandler;onContextMenu?:React.MouseEventHandler;
};
function PageThumb({page,size,rev,prefix,label,section,onSectionMenu,selected,drop,editable,...events}:ThumbProps) {
  const ref=useRef<HTMLCanvasElement>(null);const scale=size/page.height;
  useEffect(()=>{
    const el=ref.current;if(!el)return;let active=true,done=false;
    const io=new IntersectionObserver(([e])=>{
      if(!e.isIntersecting||done)return;done=true;
      queue.render(page.index,scale*(window.devicePixelRatio||1),10).then(b=>{
        if(!active)return;el.width=b.width;el.height=b.height;el.getContext('2d')!.putImageData(new ImageData(b.pixels,b.width,b.height),0,0);
      },()=>{done=false;});
    });
    io.observe(el);return()=>{active=false;io.disconnect();};
  },[page.id,page.index,scale,rev]);
  return <button {...events} className={`pp-page${selected?' on':''}${drop?' pp-drop':''}`} draggable={editable} data-page-id={page.id} title={tr('{0}페이지 (두 번 눌러 이동)',{0:page.name})}>
    <canvas ref={ref} style={{width:Math.round(page.width*scale),height:size}}/>{section!==undefined&&<i className="pp-section" title={section||tr('구역 시작 (오른쪽 단추: 구역 메뉴)')} onContextMenu={onSectionMenu}/>}{prefix&&<i className="pp-prefix">{prefix}</i>}{label&&<i className="pp-label" style={{background:label}}/>}
  </button>;
}

function PageDialog({kind,doc,info,ids,primary,edit,onClose}:{kind:Exclude<Dialog,'loadParents'>;doc:DocumentSummary;info:DocumentInfo;ids:string[];primary:PageInfo;edit:Input['edit'];onClose:()=>void}) {
  const sections=doc.pageSettings?.sections??[];
  const sec=sections.find(s=>s.page===primary.id)??[...sections].reverse().find(s=>doc.pages.findIndex(p=>p.id===s.page)<=primary.index);
  const [count,setCount]=useState(1),[place,setPlace]=useState('after'),[reference,setReference]=useState(primary.id??'');
  const [master,setMaster]=useState(info.pages[primary.index]?.master??'');
  const [start,setStart]=useState(sec?.start??1),[continuing,setContinuing]=useState(sec?.continue??true),[style,setStyle]=useState(sec?.style??'Arabic');
  const [prefix,setPrefix]=useState(sec?.prefix??''),[include,setInclude]=useState(sec?.includePrefix??false);
  const [width,setWidth]=useState(doc.pageSettings?.width??primary.width),[height,setHeight]=useState(doc.pageSettings?.height??primary.height);
  const [allowLocked,setAllowLocked]=useState(true);
  // "Adjust font size" (limits 6 to 324 pt as InDesign opens it): the text of the stretched frames follows the frame; one frame, one column, one font and size
  const [fitFont,setFitFont]=useState(false),[fontMin,setFontMin]=useState(6),[fontMax,setFontMax]=useState(324);
  const [newMargins,setNewMargins]=useState(false),[mrg,setMrg]=useState({top:36,bottom:36,left:36,right:36}),[sided,setSided]=useState(false);
  // "Auto-adjust margins to page size changes": the engine works the margins out (the document's default margins grown like the page sides) and the
  // dialog only shows them; `adjAuto` is what the engine would use for the size typed now, null when the document does not list default margins
  const [auto,setAuto]=useState(false),[adjAuto,setAdjAuto]=useState<{top:number;bottom:number;left:number;right:number}|null>(null);
  const [adjustReady,setAdjustReady]=useState(kind!=='adjust');
  useEffect(()=>{
    if(kind!=='adjust')return;
    let live=true;
    void window.folio.request({cmd:'pageItems',page:primary.index}).then(({header})=>{
      if(!live||!header.ok)return;
      const it=header.items as {margins:[number,number,number,number];side:string};
      const [top,left,bottom,right]=it.margins;setMrg({top,bottom,left,right});setSided(it.side!=='single');
      // the dialog shows the document's default margins (not those of this page) when the document has them
      void window.folio.request({cmd:'layoutAdjustSettings'}).then(({header:h})=>{
        if(!live||!h.ok)return;
        const d=h.docMargins as [number,number,number,number];
        setMrg({top:d[0],bottom:d[1],left:d[2],right:d[3]});setAuto(h.autoAdjustMargins===true);
        setAdjustReady(true);
      });
    });
    return()=>{live=false;};
  },[kind,primary.index]);
  useEffect(()=>{
    if(kind!=='adjust'||!auto)return;
    let live=true;
    void window.folio.request({cmd:'layoutAdjustSettings',width,height}).then(({header})=>{
      if(!live)return;
      if(!header.ok){setAdjAuto(null);return;}
      const m=header.autoMargins as [number,number,number,number];
      setAdjAuto({top:m[0],bottom:m[1],left:m[2],right:m[3]});
    });
    return()=>{live=false;};
  },[kind,auto,width,height]);
  const [busy,setBusy]=useState(false),[error,setError]=useState('');
  const titles:Record<Exclude<Dialog,'loadParents'>,string>={panelOptions:tr('패널 옵션'),flattener:tr('펼침면 병합 설정'),insert:tr('페이지 삽입'),move:tr('페이지 이동'),master:tr('부모 페이지 적용'),section:tr('번호 매기기 및 섹션 옵션'),adjust:tr('레이아웃 조정'),delete:tr('페이지 삭제'),newParent:tr('새 부모'),parentOptions:tr('부모 페이지 옵션')};
  const title=titles[kind];const inputRef=useRef<HTMLDivElement>(null);
  useEffect(()=>{if(adjustReady)inputRef.current?.querySelector<HTMLElement>('input, select, button')?.focus();},[adjustReady]);
  const submit=async(e:React.FormEvent)=>{
    e.preventDefault();if(busy||!adjustReady)return;setError('');
    let commands:EditCommand[]=[];
    const ref=doc.pages.findIndex(p=>p.id===reference);
    const before=place==='start'?doc.pages[0].id:place==='end'?undefined:place==='before'?reference:doc.pages[ref+1]?.id;
    switch(kind){
      case 'insert':commands=[{op:'insertPages',count,before,master:master||null}];break;
      case 'move':
        if(before&&ids.includes(before)){setError(tr('이동할 쪽 밖의 위치를 선택하세요.'));return;}
        commands=[{op:'movePages',pages:ids,before}];break;
      case 'master':commands=[{op:'applyMaster',pages:ids,master:master||null}];break;
      case 'delete':commands=[{op:'deletePages',pages:ids}];break;
      case 'section':commands=[{op:'setSection',page:primary.id!,start:continuing?undefined:start,style,prefix,includePrefix:include,marker:sec?.marker??''}];break;
      case 'adjust':commands=[{op:'adjustLayout',width,height,allow_locked:allowLocked,auto_adjust_margins:auto,adjust_font_size:fitFont,font_size_min:fitFont?fontMin:undefined,font_size_max:fitFont?fontMax:undefined,fit_metrics:undefined,top_margin:!auto&&newMargins?mrg.top:undefined,bottom_margin:!auto&&newMargins?mrg.bottom:undefined,left_margin:!auto&&newMargins?mrg.left:undefined,right_margin:!auto&&newMargins?mrg.right:undefined}];break;
    }
    setBusy(true);
    try{const r=await edit(title,commands);if(r)onClose();else setError(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.'));}
    finally{setBusy(false);}
  };
  return <div className="overlay page-dialog-scrim" onKeyDown={e=>{
    e.stopPropagation();if(e.key==='Escape'&&!busy)onClose();
    if(e.key==='Tab'){
      const all=Array.from(inputRef.current?.querySelectorAll<HTMLElement>('input:not(:disabled), select:not(:disabled), button:not(:disabled)')??[]);
      if(e.shiftKey&&document.activeElement===all[0]){e.preventDefault();all.at(-1)?.focus();}
      else if(!e.shiftKey&&document.activeElement===all.at(-1)){e.preventDefault();all[0]?.focus();}
    }
  }}>
    <div className="modal page-dialog" ref={inputRef} role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2><form onSubmit={submit}>
        {kind==='insert'&&<label>{tr('페이지 수')}<NumberField value={count} onCommit={setCount} min={1} max={9999} step={1} parse={parseInteger} format={formatInteger} decimals={0} label={tr('페이지 수')} width={180} inDialog/></label>}
        {(kind==='insert'||kind==='move')&&<>
          <label>{tr('위치')}<select value={place} onChange={e=>setPlace(e.target.value)}>
            <option value="after">{tr('페이지 뒤')}</option><option value="before">{tr('페이지 앞')}</option><option value="start">{tr('문서 시작')}</option><option value="end">{tr('문서 끝')}</option>
          </select></label>
          {(place==='before'||place==='after')&&<label>{tr('기준 페이지')}<select value={reference} onChange={e=>setReference(e.target.value)}>{doc.pages.map(p=><option key={p.id} value={p.id}>{p.name}</option>)}</select></label>}
        </>}
        {(kind==='insert'||kind==='master')&&<label>{tr('부모 페이지')}<select value={master} onChange={e=>setMaster(e.target.value)}><option value="">{tr('[없음]')}</option>{info.masters.map(m=><option key={m.id} value={m.id}>{m.name}</option>)}</select></label>}
        {kind==='delete'&&<p>{tr('선택한 {0}쪽과 그 쪽의 개체를 삭제합니다. 실행 취소할 수 있습니다.',{0:ids.length})}</p>}
        {kind==='section'&&<>
          <label className="page-check"><input type="checkbox" checked={continuing} onChange={e=>setContinuing(e.target.checked)}/>{tr('자동 페이지 번호 매기기')}</label>
          <label>{tr('시작 페이지 번호')}<NumberField value={start} onCommit={setStart} min={1} max={999999} step={1} parse={parseInteger} format={formatInteger} decimals={0} label={tr('시작 페이지 번호')} width={180} disabled={continuing} inDialog/></label>
          <label>{tr('번호 형식')}<select value={style} onChange={e=>setStyle(e.target.value)}>
            <option value="Arabic">1, 2, 3, 4…</option><option value="LowerRoman">i, ii, iii, iv…</option><option value="UpperRoman">I, II, III, IV…</option><option value="UpperLetters">A, B, C, D…</option><option value="LowerLetters">a, b, c, d…</option>
          </select></label>
          <label>{tr('섹션 접두어')}<input maxLength={8} value={prefix} onChange={e=>setPrefix(e.target.value)}/></label>
          <label className="page-check"><input type="checkbox" checked={include} onChange={e=>setInclude(e.target.checked)}/>{tr('페이지 번호에 접두어 포함')}</label>
        </>}
        {kind==='adjust'&&adjustReady&&<>
          <label>{tr('너비 (pt)')}<NumberField value={width} onCommit={setWidth} min={1} max={15552} step={1} decimals={3} label={tr('너비 (pt)')} width={180} inDialog/></label>
          <label>{tr('높이 (pt)')}<NumberField value={height} onCommit={setHeight} min={1} max={15552} step={1} decimals={3} label={tr('높이 (pt)')} width={180} inDialog/></label>
          <label>{tr('방향')}<select value={width>height?'landscape':'portrait'} onChange={e=>{if((e.target.value==='landscape')!==(width>height)){setWidth(height);setHeight(width);}}}>
            <option value="portrait">{tr('세로')}</option><option value="landscape">{tr('가로')}</option>
          </select></label>
          <label className="page-check"><input type="checkbox" checked={allowLocked} onChange={e=>setAllowLocked(e.target.checked)}/>{tr('잠긴 개체도 조정')}</label>
          <label className="page-check"><input type="checkbox" checked={auto} onChange={e=>setAuto(e.target.checked)}/>{tr('페이지 크기 변경에 맞춰 여백 자동 조정')}</label>
          <label className="page-check"><input type="checkbox" checked={fitFont} onChange={e=>setFitFont(e.target.checked)}/>{tr('글꼴 크기 조정')}</label>
          {fitFont&&<fieldset><legend>{tr('글꼴 크기 한도')}</legend>
            <label>{tr('최소 (pt)')}<NumberField value={fontMin} onCommit={setFontMin} min={0.1} max={fontMax} step={1} decimals={1} label={tr('최소 (pt)')} width={180} inDialog/></label>
            <label>{tr('최대 (pt)')}<NumberField value={fontMax} onCommit={setFontMax} min={fontMin} max={1296} step={1} decimals={1} label={tr('최대 (pt)')} width={180} inDialog/></label>
            <p>{tr('글 상자의 글 크기와 고정 행간을 틀이 늘어난 만큼 바꿉니다. 한 단, 이어지지 않은 한 틀, 한 글꼴·한 크기의 글만 되고 그 밖의 글 상자가 있으면 아무것도 바꾸지 않고 이유를 알립니다.')}</p>
          </fieldset>}
          <label className="page-check"><input type="checkbox" checked={newMargins&&!auto} disabled={auto} onChange={e=>setNewMargins(e.target.checked)}/>{tr('여백도 바꾸기')}</label>
          {((auto&&adjAuto)||(!auto&&newMargins))&&<fieldset disabled={auto}><legend>{tr('여백')}</legend>
            {(()=>{const m=auto&&adjAuto?adjAuto:mrg;return <>
            <label>{tr('위')}<NumberField value={m.top} onCommit={v=>setMrg(o=>({...o,top:v}))} min={0} max={15552} step={1} decimals={3} label={tr('위')} width={180} disabled={auto} inDialog/></label>
            <label>{tr('아래')}<NumberField value={m.bottom} onCommit={v=>setMrg(o=>({...o,bottom:v}))} min={0} max={15552} step={1} decimals={3} label={tr('아래')} width={180} disabled={auto} inDialog/></label>
            <label>{sided?tr('안쪽'):tr('왼쪽')}<NumberField value={m.left} onCommit={v=>setMrg(o=>({...o,left:v}))} min={0} max={15552} step={1} decimals={3} label={sided?tr('안쪽'):tr('왼쪽')} width={180} disabled={auto} inDialog/></label>
            <label>{sided?tr('바깥쪽'):tr('오른쪽')}<NumberField value={m.right} onCommit={v=>setMrg(o=>({...o,right:v}))} min={0} max={15552} step={1} decimals={3} label={sided?tr('바깥쪽'):tr('오른쪽')} width={180} disabled={auto} inDialog/></label></>;})()}
          </fieldset>}
          <p>{tr('모든 페이지와 부모 페이지를 새 크기로 바꾸고, 그 안의 개체를 늘리거나 줄입니다. 여백은 그대로 두거나 새로 정할 수 있고, 새 여백은 문서의 기본 여백과 같은 여백을 쓰는 부모 페이지와 그 부모를 따르는 페이지에 적용됩니다.')}</p>
        </>}
        {error&&<p role="alert" className="error">{error}</p>}
        <div className="actions"><button type="button" disabled={busy} onClick={onClose}>{tr('취소')}</button><button className="primary" type="submit" disabled={busy||!adjustReady}>{busy?tr('적용 중…'):tr('확인')}</button></div>
      </form>
    </div>
  </div>;
}

/** Page Attributes ▸ Spread Flattening ▸ Custom…: the settings kept with the spreads (the first one's are shown). */
function FlattenerDialog({spreads,doc,edit,onClose}:{spreads:string[];doc:DocumentSummary;edit:Input['edit'];onClose:()=>void}) {
  const f=doc.pageSettings?.flattener?.[spreads[0]];
  const [balance,setBalance]=useState(f?.balance??50),[lineArt,setLineArt]=useState(f?.lineArt??300),[gradient,setGradient]=useState(f?.gradient??150);
  const [text,setText]=useState(f?.text??false),[strokes,setStrokes]=useState(f?.strokes??false),[clip,setClip]=useState(f?.clip??false);
  const [busy,setBusy]=useState(false),[error,setError]=useState('');
  const title=tr('펼침면 병합 설정');
  const submit=async(e:React.FormEvent)=>{
    e.preventDefault();if(busy)return;setError('');setBusy(true);
    try{const r=await edit(title,[{op:'setSpreadFlattener',spreads,level:'Custom',balance,lineArt,gradient,clip,strokes,text}]);if(r)onClose();else setError(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.'));}
    finally{setBusy(false);}
  };
  return <div className="overlay page-dialog-scrim" onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape'&&!busy)onClose();}}>
    <div className="modal page-dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2><form onSubmit={submit}>
        <label>{tr('래스터/벡터 균형')}<input type="range" min={0} max={100} step={1} value={balance} onChange={e=>setBalance(e.target.valueAsNumber)}/><span>{balance}</span></label>
        <label>{tr('선화 및 텍스트 해상도 (ppi)')}<NumberField value={lineArt} onCommit={setLineArt} min={1} max={9600} step={1} decimals={3} label={tr('선화 및 텍스트 해상도 (ppi)')} width={180} inDialog/></label>
        <label>{tr('그레이디언트 및 메시 해상도 (ppi)')}<NumberField value={gradient} onCommit={setGradient} min={1} max={1200} step={1} decimals={3} label={tr('그레이디언트 및 메시 해상도 (ppi)')} width={180} inDialog/></label>
        <label className="page-check"><input type="checkbox" checked={text} onChange={e=>setText(e.target.checked)}/>{tr('모든 텍스트를 윤곽선으로')}</label>
        <label className="page-check"><input type="checkbox" checked={strokes} onChange={e=>setStrokes(e.target.checked)}/>{tr('모든 획을 윤곽선으로')}</label>
        <label className="page-check"><input type="checkbox" checked={clip} onChange={e=>setClip(e.target.checked)}/>{tr('복잡한 영역 클립')}</label>
        {error&&<p role="alert" className="error">{error}</p>}
        <div className="actions"><button type="button" disabled={busy} onClick={onClose}>{tr('취소')}</button><button className="primary" type="submit" disabled={busy}>{busy?tr('적용 중…'):tr('확인')}</button></div>
      </form>
    </div>
  </div>;
}

/** The first prefix no parent has (as the engine picks it): A to Z, then AA, AB… */
function freePrefix(info: DocumentInfo): string {
  const taken = new Set(info.masters.map((m) => m.prefix));
  const letters = Array.from({ length: 26 }, (_, k) => String.fromCharCode(65 + k));
  return [...letters, ...letters.flatMap((a) => letters.map((b) => a + b))].find((p) => !taken.has(p)) ?? 'A';
}

/** Pages panel ▸ New Parent… and Parent Options… */
function ParentDialog({kind,info,parent,edit,onClose}:{kind:'newParent'|'parentOptions';info:DocumentInfo;parent:string|null;edit:Input['edit'];onClose:()=>void}) {
  const m=kind==='parentOptions'?info.masters.find(x=>x.id===parent):undefined;
  const [prefix,setPrefix]=useState(m?.prefix??freePrefix(info));
  const [name,setName]=useState(m?.baseName??'Parent');
  const [basedOn,setBasedOn]=useState(m?.basedOn??'n');
  const [pages,setPages]=useState(m?.pages??(info.facingPages?2:1));
  const [busy,setBusy]=useState(false),[error,setError]=useState('');
  const title=kind==='newParent'?tr('새 부모'):tr('부모 페이지 옵션');
  const ok=prefix.trim().length>=1&&prefix.trim().length<=4&&name.trim().length>0;
  const submit=async(e:React.FormEvent)=>{
    e.preventDefault();if(busy||!ok)return;setError('');
    const command:EditCommand=kind==='newParent'
      ? {op:'newParent',pages,prefix:prefix.trim(),name:name.trim(),...(basedOn!=='n'?{basedOn}:{})}
      : {op:'parentOptions',id:parent!,...(prefix.trim()!==m?.prefix?{prefix:prefix.trim()}:{}),...(name.trim()!==m?.baseName?{name:name.trim()}:{}),...(basedOn!==(m?.basedOn??'n')?{basedOn}:{})};
    setBusy(true);
    try{const r=await edit(title,[command]);if(r)onClose();else setError(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.'));}
    finally{setBusy(false);}
  };
  return <div className="overlay page-dialog-scrim" onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape'&&!busy)onClose();}}>
    <div className="modal page-dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2><form onSubmit={submit}>
        <label>{tr('접두어')}<input aria-label={tr('접두어')} maxLength={4} required value={prefix} onChange={e=>setPrefix(e.target.value)}/></label>
        <label>{tr('이름')}<input aria-label={tr('이름')} required value={name} onChange={e=>setName(e.target.value)}/></label>
        <label>{tr('기준 부모')}<select aria-label={tr('기준 부모')} value={basedOn} onChange={e=>setBasedOn(e.target.value)}>
          <option value="n">{tr('[없음]')}</option>{info.masters.filter(x=>x.id!==parent).map(x=><option key={x.id} value={x.id}>{x.name}</option>)}</select></label>
        <label>{tr('페이지 수')}<select aria-label={tr('페이지 수')} value={pages} disabled={kind==='parentOptions'} onChange={e=>setPages(Number(e.target.value))}>
          {Array.from({length:10},(_,k)=><option key={k+1} value={k+1}>{k+1}</option>)}</select></label>
        {error&&<p role="alert" className="error">{error}</p>}
        <div className="actions"><button type="button" disabled={busy} onClick={onClose}>{tr('취소')}</button><button className="primary" type="submit" disabled={busy||!ok}>{busy?tr('적용 중…'):tr('확인')}</button></div>
      </form>
    </div>
  </div>;
}

function PanelOptionsDialog({opts,layout,onApply,onClose}:{opts:PanelOptions;layout:'vertical'|'horizontal';onApply:(o:PanelOptions,layout:'vertical'|'horizontal')=>void;onClose:()=>void}) {
  const [o,setO]=useState(opts),[vertical,setVertical]=useState(layout==='vertical');
  const sizes=[['xs',tr('아주 작게')],['s',tr('작게')],['m',tr('보통')],['l',tr('크게')],['xl',tr('아주 크게')]] as const;
  const title=tr('패널 옵션');
  return <div className="overlay page-dialog-scrim" onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape')onClose();}}>
    <div className="modal page-dialog" role="dialog" aria-modal="true" aria-label={title}>
      <h2>{title}</h2><form onSubmit={e=>{e.preventDefault();onApply(o,vertical?'vertical':'horizontal');onClose();}}>
        <label>{tr('페이지 크기')}<select aria-label={tr('페이지 크기')} value={o.pageSize} onChange={e=>setO({...o,pageSize:e.target.value as PanelOptions['pageSize']})}>
          {sizes.map(([k,l])=><option key={k} value={k}>{l}</option>)}</select></label>
        <label className="page-check"><input type="checkbox" aria-label={tr('세로로 보기')} checked={vertical} onChange={e=>setVertical(e.target.checked)}/>{tr('세로로 보기')}</label>
        <label>{tr('부모 크기')}<select aria-label={tr('부모 크기')} value={o.parentSize} onChange={e=>setO({...o,parentSize:e.target.value as PanelOptions['parentSize']})}>
          <option value="s">{tr('작게')}</option><option value="m">{tr('보통')}</option><option value="l">{tr('크게')}</option></select></label>
        <label>{tr('위에 둘 영역')}<select aria-label={tr('위에 둘 영역')} value={o.top} onChange={e=>setO({...o,top:e.target.value as PanelOptions['top']})}>
          <option value="parents">{tr('부모 페이지')}</option><option value="pages">{tr('페이지')}</option></select></label>
        <label className={'page-check'} title={tr('아이콘: 투명도, 펼침면 회전, 페이지 전환')}><input type="checkbox" disabled aria-label={tr('아이콘 표시')}/>{tr('아이콘 표시')}</label>
        <p>{tr('이 컴퓨터에만 저장됩니다. 문서에는 들어가지 않습니다.')}</p>
        <div className="actions"><button type="button" onClick={onClose}>{tr('취소')}</button><button className="primary" type="submit">{tr('확인')}</button></div>
      </form>
    </div>
  </div>;
}
