import { useEffect, useRef, useState } from 'react';
import { tr } from '../../i18n';
import type { PanelCtx } from '../../ui/panels';
import { GenerateIndex } from './generate';
import { declarePanelCommands, usePanelCommands } from '../../ui/panelbus';
import {NumberField} from '../../ui/controls/numberfield';
import {formatInteger,parseInteger} from '../../ui/controls/lengthfield';
import {Modal} from '../../ui/tables';
import {SortOptionsDialog, type SortGroup} from './sortoptions';

type Ref = {id:string;story:string;kind:string;limit:string;at:number;nested:boolean};
type Cross = {id:string;kind:string;custom:string;referenced:string};
type Topic = {id:string;name:string;sort_order:string;topics:Topic[];references:Ref[];cross_references?:Cross[]};
type Index = {id:string;topics:Topic[]};
export const INDEX_PANELS = {
  index: {get title(){return tr('색인');},icon:'type',width:320,height:420},
};
// The page reference kinds InDesign measured (index-c1). "To next use of style" is not measured for creation: it needs a style argument, no command makes it yet (generation of such markers is done, index-c3).
const KINDS = ['CurrentPage','ToNextStyleChange','ToEndOfStory','ToEndOfDocument','ToEndOfSection','ForNextNParagraphs','ForNextNPages','SuppressPageNumbers'] as const;
// 'ToNextUseOfStyle' needs a paragraph style: the command addIndexStyleReference makes it (index-c6)
type Kind = typeof KINDS[number] | 'ToNextUseOfStyle';
const COUNTED: Kind[] = ['ForNextNParagraphs','ForNextNPages'];
// The cross reference kinds InDesign measured (index-c6); the generated line reads "See Target", "See also Target", ...
const CROSS_KINDS = ['See','SeeAlso','SeeHerein','SeeAlsoHerein','SeeOrAlsoBracket','CustomCrossReferenceBefore','CustomCrossReferenceAfter'] as const;
const CUSTOM_CROSS = ['CustomCrossReferenceBefore','CustomCrossReferenceAfter'];
function crossLabel(kind:string):string {
  switch(kind) {
    case 'See': return tr('See (참조)');
    case 'SeeAlso': return tr('See also (참조도)');
    case 'SeeHerein': return tr('See herein (여기를 참조)');
    case 'SeeAlsoHerein': return tr('See also herein (여기도 참조)');
    case 'SeeOrAlsoBracket': return tr('See [also] (쪽 번호가 있으면 See also)');
    case 'CustomCrossReferenceBefore': return tr('사용자 글, 주제 앞');
    case 'CustomCrossReferenceAfter': return tr('사용자 글, 주제 뒤');
    default: return kind;
  }
}
/** Every topic with its name path (`Parent: Child`), the way a cross reference names the topic it points to. */
function flatten(topics:Topic[],prefix=''):{id:string;path:string}[] {
  return topics.flatMap(t=>{const path=prefix?`${prefix}: ${t.name}`:t.name;return [{id:t.id,path},...flatten(t.topics,path)];});
}
function kindLabel(kind:string):string {
  switch(kind) {
    case 'CurrentPage': return tr('현재 페이지');
    case 'ToNextStyleChange': return tr('다음 스타일 변경까지');
    case 'ToEndOfStory': return tr('스토리 끝까지');
    case 'ToEndOfDocument': return tr('문서 끝까지');
    case 'ToEndOfSection': return tr('섹션 끝까지');
    case 'ForNextNParagraphs': return tr('다음 문단 수만큼');
    case 'ForNextNPages': return tr('다음 페이지 수만큼');
    case 'SuppressPageNumbers': return tr('페이지 번호 숨김');
    case 'ToNextUseOfStyle': return tr('다음 스타일 사용까지');
    default: return kind;
  }
}
type Actions = {
  selected:string; select:(id:string)=>void; busy:boolean; editable:boolean;
  selectedRef:string; selectRef:(id:string)=>void;
  remove:(ref:Ref)=>void; move:(ref:Ref)=>void;
  removeCross:(c:Cross)=>void; paths:Map<string,string>;
};
function Topics({topics,a}:{topics:Topic[];a:Actions}) {
  return <ul>{topics.map(t=><li key={t.id}>
    <button type="button" aria-pressed={a.selected===t.id} data-topic={t.name} onClick={()=>a.select(a.selected===t.id?'':t.id)}
      style={{fontWeight:a.selected===t.id?'bold':undefined}}>{t.name}</button>{' '}
    <small>{tr('참조 {0}개',{0:t.references.length})}</small>
    {t.sort_order && <div>{tr('정렬 이름: {0}',{0:t.sort_order})}</div>}
    {t.references.map(r=><div key={r.id} data-reference={r.id}>
      <button type="button" className="p-link" aria-pressed={a.selectedRef===r.id} data-reference-select={r.id} onClick={()=>a.selectRef(a.selectedRef===r.id?'':r.id)}
        style={{fontWeight:a.selectedRef===r.id?'bold':undefined}}>{kindLabel(r.kind)}{r.limit?` (${r.limit})`:''}</button>{' '}
      <small>{r.nested?tr('각주·표 안'):tr('위치 {0}',{0:r.at})}</small>{' '}
      <button type="button" disabled={!a.editable||a.busy} onClick={()=>a.remove(r)}>{tr('참조 삭제')}</button>{' '}
      <button type="button" disabled={!a.editable||a.busy||!a.selected||a.selected===t.id} onClick={()=>a.move(r)}>{tr('선택한 주제로 옮기기')}</button>
    </div>)}
    {(t.cross_references??[]).map(c=><div key={c.id} data-cross-reference={c.id}>
      {crossLabel(c.kind)}{c.custom?` “${c.custom}”`:''}{' '}{a.paths.get(c.referenced)??c.referenced}{' '}
      <button type="button" disabled={!a.editable||a.busy} onClick={()=>a.removeCross(c)}>{tr('상호 참조 삭제')}</button>
    </div>)}
    {!!t.topics.length && <Topics topics={t.topics} a={a}/>}
  </li>)}</ul>;
}
/** The topic that holds the page reference `id`, with the reference. */
function owner(topics:Topic[],id:string):{topic:Topic;ref:Ref}|null {
  for(const t of topics) { const ref=t.references.find(r=>r.id===id); if(ref)return {topic:t,ref}; const inner=owner(t.topics,id); if(inner)return inner; }
  return null;
}
function find(topics:Topic[],id:string):Topic|null {
  for(const t of topics) { if(t.id===id)return t; const inner=find(t.topics,id); if(inner)return inner; }
  return null;
}
// The panel menu (menuaudit-c): what each item does, by command id. Generate Index is offered by GenerateIndex (its dialog's state is there).
const I = 'id.panel.index.';
declarePanelCommands([I+'new', I+'delete', I+'removeUnusedTopics', I+'goToSelectedMarker', I+'capitalize', I+'updatePreview', I+'showFindField', I+'showUnusedTopics', I+'duplicate', I+'options', I+'importTopics', I+'sortOptions']);
// Capitalize…: the scopes of InDesign's dialog (the engine's capitalizeIndexTopics)
const CAP_SCOPES = ['selected','subentries','level1','all'] as const;
function capLabel(scope:typeof CAP_SCOPES[number]):string {
  switch(scope) {
    case 'selected': return tr('선택한 주제');
    case 'subentries': return tr('선택한 주제와 모든 하위 주제');
    case 'level1': return tr('모든 1단계 주제');
    default: return tr('모든 주제');
  }
}
/** The topics whose name (or a subtopic's name) holds `text`, with the path down to them; every topic when `text` is empty. */
function filtered(topics:Topic[],text:string):Topic[] {
  if(!text)return topics;
  const t=text.toLocaleLowerCase();
  return topics.flatMap(x=>{const inner=filtered(x.topics,text);return x.name.toLocaleLowerCase().includes(t)||inner.length?[{...x,topics:x.name.toLocaleLowerCase().includes(t)?x.topics:inner}]:[];});
}
/** The topics without the unused ones (Show Unused Topics off). */
function withoutUnused(topics:Topic[],unused:Set<string>):Topic[] {
  return topics.filter(t=>!unused.has(t.id)).map(t=>({...t,topics:withoutUnused(t.topics,unused)}));
}
/** The topics nothing points to: no reference, no cross reference, no subtopic that has one, and no cross reference of another topic aiming at it (the topmost ones). */
function unusedTopics(all:Topic[]):Topic[] {
  const aimed=new Set<string>();
  const collect=(ts:Topic[])=>ts.forEach(t=>{(t.cross_references??[]).forEach(c=>aimed.add(c.referenced));collect(t.topics);});
  collect(all);
  const keep=(t:Topic):boolean=>t.references.length>0||(t.cross_references??[]).length>0||aimed.has(t.id)||t.topics.some(keep);
  const walk=(ts:Topic[]):Topic[]=>ts.flatMap(t=>keep(t)?walk(t.topics):[t]);
  return walk(all);
}

export function IndexPanel({ctx}:{ctx:PanelCtx}) {
  const [indexes,setIndexes]=useState<Index[]>([]);
  const [error,setError]=useState('');
  const [selected,setSelected]=useState('');
  const [name,setName]=useState('');
  const [kind,setKind]=useState<Kind>('CurrentPage');
  const [limit,setLimit]=useState(1);
  const [busy,setBusy]=useState(false);
  const [crossKind,setCrossKind]=useState<typeof CROSS_KINDS[number]>('See');
  const [crossTarget,setCrossTarget]=useState('');
  const [crossText,setCrossText]=useState('');
  const [style,setStyle]=useState('');
  const topicInput=useRef<HTMLInputElement>(null);
  // the panel menu's view options (this panel's own state while it is open) and Update Preview's reload
  const [findField,setFindField]=useState(false);
  const [findText,setFindText]=useState('');
  const [showUnused,setShowUnused]=useState(true);
  const [reload,setReload]=useState(0);
  const [capOpen,setCapOpen]=useState(false);
  const [capScope,setCapScope]=useState<typeof CAP_SCOPES[number]>('all');
  // Duplicate… (a page reference: a new marker at the same place, into the topic chosen) and Options… (the topic's name and sort order)
  const [selectedRef,setSelectedRef]=useState('');
  const [dupOpen,setDupOpen]=useState(false);
  const [dupTopic,setDupTopic]=useState('');
  const [optOpen,setOptOpen]=useState(false);
  const [optName,setOptName]=useState('');
  const [optSort,setOptSort]=useState('');
  // Sort Options…: the document's language groups (document info indexSortOptions; the engine's setIndexSortOptions)
  const [sortOpen,setSortOpen]=useState(false);
  const sortGroups=((ctx.info as unknown as {indexSortOptions?:SortGroup[]}|undefined)?.indexSortOptions)??[];
  useEffect(()=>{
    let active=true;setIndexes([]);setError('');
    if(ctx.doc)void window.folio.request({cmd:'indexes'}).then(r=>{
      if(!active)return;
      if(r.header.ok)setIndexes(r.header.indexes as Index[]);else setError(String(r.header.error));
    }).catch(e=>{if(active)setError(String(e));});
    return ()=>{active=false;};
  },[ctx.doc,ctx.info,reload]);
  const all=indexes.flatMap(i=>i.topics);
  const topic=selected?find(all,selected):null;
  const canEdit=ctx.editable&&!ctx.readOnly&&!busy;
  const range=ctx.textFormatting.selection;
  const canReference=canEdit&&!!topic&&!!range&&!range.nested;
  const run=async(label:string,ops:Parameters<PanelCtx['edit']>[1],after?:()=>void)=>{
    if(!canEdit)return;
    setBusy(true);setError('');
    try { const r=await ctx.edit(label,ops); if(!r)setError(tr('색인을 바꾸지 못했습니다.')); else after?.(); }
    catch(e){ setError(String(e)); }
    finally { setBusy(false); }
  };
  const addReference=()=>range&&void run(tr('색인 참조 추가'),kind==='ToNextUseOfStyle'
    ?[{op:'addIndexStyleReference',story:range.story,at:range.start,topic:selected,style}]
    :[{op:'addIndexReference',story:range.story,at:range.start,topic:selected,kind,limit:COUNTED.includes(kind)?limit:0}]);
  const unused=unusedTopics(all);
  const marker=topic?.references.find(r=>!r.nested);
  const picked=selectedRef?owner(all,selectedRef):null;
  const duplicate=()=>{
    if(!picked)return;
    const {ref}=picked;
    void run(tr('색인 참조 복제'),ref.kind==='ToNextUseOfStyle'
      ?[{op:'addIndexStyleReference',story:ref.story,at:ref.at,topic:dupTopic,style:ref.limit}]
      :[{op:'addIndexReference',story:ref.story,at:ref.at,topic:dupTopic,kind:ref.kind,limit:COUNTED.includes(ref.kind as Kind)?(parseInt(ref.limit,10)||1):0}]);
  };
  usePanelCommands('index',{
    // New...: a reference to the picked topic at the cursor (the button's work); with no topic picked, the topic name box gets the focus
    [I+'new']: canEdit ? (canReference&&!(kind==='ToNextUseOfStyle'&&!style) ? addReference : ()=>topicInput.current?.focus()) : undefined,
    [I+'delete']: canEdit&&topic ? ()=>void run(tr('색인 주제 삭제'),[{op:'deleteIndexTopic',id:selected}],()=>setSelected('')) : undefined,
    [I+'removeUnusedTopics']: canEdit&&unused.length ? ()=>void run(tr('안 쓰는 색인 주제 제거'),unused.map(t=>({op:'deleteIndexTopic' as const,id:t.id})),()=>setSelected('')) : undefined,
    [I+'goToSelectedMarker']: marker&&ctx.revealText ? ()=>ctx.revealText?.({story:marker.story,start:marker.at,end:marker.at}) : undefined,
    [I+'capitalize']: canEdit&&all.length ? ()=>{setCapScope(topic?'selected':'all');setCapOpen(true);} : undefined,
    [I+'updatePreview']: ctx.doc ? ()=>setReload(n=>n+1) : undefined,
    [I+'showFindField']: {run:()=>{setFindField(v=>!v);setFindText('');},checked:findField},
    [I+'showUnusedTopics']: {run:()=>setShowUnused(v=>!v),checked:showUnused},
    [I+'duplicate']: canEdit&&picked&&!picked.ref.nested ? ()=>{setDupTopic(picked.topic.id);setDupOpen(true);} : undefined,
    [I+'options']: canEdit&&topic ? ()=>{setOptName(topic.name);setOptSort(topic.sort_order);setOptOpen(true);} : undefined,
    // Import Topics…: the engine's dialog (next/shared/dialogs/importIndexTopics.json) lists the project's other documents
    [I+'importTopics']: canEdit ? ()=>void window.folio.request({cmd:'openDialog',dialog:'importIndexTopics'}).catch(()=>{}) : undefined,
    [I+'sortOptions']: canEdit&&sortGroups.length ? ()=>setSortOpen(true) : undefined,
  });
  if(!ctx.doc)return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  const actions:Actions={selected,select:setSelected,selectedRef,selectRef:setSelectedRef,busy,editable:ctx.editable&&!ctx.readOnly,
    remove:r=>void run(tr('색인 참조 삭제'),[{op:'deleteIndexReference',id:r.id}]),
    move:r=>void run(tr('색인 참조 옮기기'),[{op:'moveIndexReference',id:r.id,topic:selected}]),
    removeCross:c=>void run(tr('색인 상호 참조 삭제'),[{op:'deleteIndexCrossReference',id:c.id}]),
    paths:new Map(flatten(all).map(p=>[p.id,p.path]))};
  const targets=flatten(all).filter(p=>p.id!==selected);
  const styles=ctx.info?.paragraphStyles??[];
  const customKind=CUSTOM_CROSS.includes(crossKind);
  const canCross=canEdit&&!!topic&&!!crossTarget&&targets.some(t=>t.id===crossTarget)&&(!customKind||!!crossText.trim());
  return <section aria-label={tr('색인')} style={{padding:8}}>
    <GenerateIndex ctx={ctx} disabled={!canEdit||indexes.length!==1}/>
    {error && <p role="alert">{error}</p>}
    {!indexes.length && !error && <p>{tr('문서에 색인이 없습니다.')}</p>}
    {findField && <input type="search" value={findText} aria-label={tr('주제 찾기')} placeholder={tr('주제 찾기')} onChange={e=>setFindText(e.target.value)}/>}
    {indexes.map(i=><Topics key={i.id} topics={filtered(showUnused?i.topics:withoutUnused(i.topics,new Set(unused.map(u=>u.id))),findText.trim())} a={actions}/>)}
    {capOpen && <Modal title={tr('대문자 처리')} close={()=>setCapOpen(false)} ok={capScope==='level1'||capScope==='all'||!!topic} submit={()=>void run(tr('대문자 처리'),[{op:'capitalizeIndexTopics',scope:capScope,topic:topic?.id??''}])}>
      <fieldset role="radiogroup" aria-label={tr('대문자로 바꿀 주제')} style={{display:'flex',flexDirection:'column',gap:8}}>{CAP_SCOPES.map(sc=><label key={sc} className="check" style={{display:'flex',alignItems:'center',gap:8,justifyContent:'flex-start',gridTemplateColumns:'none'}}><input type="radio" name="cap-scope" style={{width:'auto',flex:'none',margin:0}} checked={capScope===sc} disabled={(sc==='selected'||sc==='subentries')&&!topic} onChange={()=>setCapScope(sc)}/><span style={{flex:1,textAlign:'left'}}>{capLabel(sc)}</span></label>)}</fieldset>
    </Modal>}
    {dupOpen && picked && <Modal title={tr('참조 복제')} close={()=>setDupOpen(false)} ok={!!dupTopic} submit={duplicate}>
      <label>{tr('주제')} <select value={dupTopic} aria-label={tr('복제한 참조의 주제')} onChange={e=>setDupTopic(e.target.value)}>
        {flatten(all).map(x=><option key={x.id} value={x.id}>{x.path}</option>)}
      </select></label>
      <p>{tr('같은 자리에 {0} 참조를 하나 더 만듭니다.',{0:kindLabel(picked.ref.kind)})}</p>
    </Modal>}
    {sortOpen && <SortOptionsDialog groups={sortGroups} close={()=>setSortOpen(false)} apply={rows=>void run(tr('색인 정렬 옵션'),[{op:'setIndexSortOptions',options:rows.map(g=>({name:g.name,include:g.include,priority:g.priority,header_type:g.header_type}))}],()=>setSortOpen(false))}/>}
    {optOpen && topic && <Modal title={tr('주제 옵션')} close={()=>setOptOpen(false)} ok={!!optName.trim()} submit={()=>void run(tr('색인 주제 옵션'),[{op:'setIndexTopic',id:topic.id,name:optName.trim(),sort_order:optSort.trim()}],()=>setSelected(''))}>
      <label>{tr('주제 이름')} <input type="text" value={optName} aria-label={tr('주제 옵션 이름')} onChange={e=>setOptName(e.target.value)}/></label>
      <label>{tr('정렬 기준')} <input type="text" value={optSort} aria-label={tr('주제 옵션 정렬 기준')} onChange={e=>setOptSort(e.target.value)}/></label>
    </Modal>}
    <div>
      <input ref={topicInput} type="text" value={name} aria-label={tr('주제 이름')} onChange={e=>setName(e.target.value)}/>
      <button type="button" disabled={!canEdit||!name.trim()} onClick={()=>void run(tr('색인 주제 추가'),[{op:'addIndexTopic',parent:'',name,sort_order:''}],()=>setName(''))}>{tr('주제 추가')}</button>
      <button type="button" disabled={!canEdit||!name.trim()||!topic} onClick={()=>void run(tr('색인 하위 주제 추가'),[{op:'addIndexTopic',parent:selected,name,sort_order:''}],()=>setName(''))}>{tr('하위 주제 추가')}</button>
      <button type="button" disabled={!canEdit||!name.trim()||!topic} onClick={()=>topic&&void run(tr('색인 주제 이름 바꾸기'),[{op:'setIndexTopic',id:topic.id,name,sort_order:topic.sort_order}],()=>setSelected(''))}>{tr('이름 바꾸기')}</button>
      <button type="button" disabled={!canEdit||!topic} onClick={()=>void run(tr('색인 주제 삭제'),[{op:'deleteIndexTopic',id:selected}],()=>setSelected(''))}>{tr('주제 삭제')}</button>
    </div>
    <div>
      <select value={kind} aria-label={tr('참조 종류')} onChange={e=>setKind(e.target.value as Kind)}>
        {[...KINDS,'ToNextUseOfStyle' as const].map(k=><option key={k} value={k}>{kindLabel(k)}</option>)}
      </select>
      {COUNTED.includes(kind) && <NumberField value={limit} onCommit={setLimit} min={1} max={1e9} step={1} decimals={0} parse={parseInteger} format={formatInteger} label={tr('참조 개수')} width={72}/>}
      {kind==='ToNextUseOfStyle' && <select value={style} aria-label={tr('문단 스타일')} onChange={e=>setStyle(e.target.value)}>
        <option value="">{tr('스타일 고르기')}</option>
        {styles.map(st=><option key={st.id} value={st.id}>{[...st.group,st.name].join(' / ')}</option>)}
      </select>}
      <button type="button" disabled={!canReference||(kind==='ToNextUseOfStyle'&&!style)} onClick={addReference}>{tr('커서 위치에 참조 추가')}</button>
    </div>
    <div>
      <select value={crossKind} aria-label={tr('상호 참조 종류')} onChange={e=>setCrossKind(e.target.value as typeof CROSS_KINDS[number])}>
        {CROSS_KINDS.map(k=><option key={k} value={k}>{crossLabel(k)}</option>)}
      </select>
      {customKind && <input type="text" value={crossText} aria-label={tr('상호 참조 글')} onChange={e=>setCrossText(e.target.value)}/>}
      <select value={crossTarget} aria-label={tr('가리킬 주제')} onChange={e=>setCrossTarget(e.target.value)}>
        <option value="">{tr('가리킬 주제 고르기')}</option>
        {targets.map(t=><option key={t.id} value={t.id}>{t.path}</option>)}
      </select>
      <button type="button" disabled={!canCross} onClick={()=>void run(tr('색인 상호 참조 추가'),[{op:'addIndexCrossReference',topic:selected,referenced:crossTarget,kind:crossKind,custom:customKind?crossText.trim():''}],()=>setCrossText(''))}>{tr('선택한 주제에 상호 참조 추가')}</button>
    </div>
    <p>{tr('주제를 고르고 글에 커서를 둔 뒤 참조를 추가합니다. 단어 가운데에 두면 그 단어 앞에 놓입니다.')}</p>
  </section>;
}
