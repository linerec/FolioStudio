import { useEffect, useState } from 'react';
import { tr } from '../../i18n';
import type { PanelCtx } from '../../ui/panels';
import { declarePanelCommands, usePanelCommands } from '../../ui/panelbus';
import { NumberField } from '../../ui/controls/numberfield';
import type { TrapPreset, TrapSettings } from './types';

// Editing and page assignment are engine commands (saveTrapPreset, duplicateTrapPreset, deleteTrapPreset,
// assignTrapPreset); this panel only gathers the values. Drawing the traps in separations is not built and InDesign
// could not produce a trapped separation on this computer to compare with: the panel says both (grey note, no red mark).
export const TRAP_PANELS = {
  trapPresets: {get title(){return tr('트랩 사전 설정');},icon:'separations',width:340,height:400},
};
const DEFAULT_ID = 'TrapPreset/$ID/kDefaultTrapStyleName', NONE_ID = 'TrapPreset/$ID/k[No Trap Preset]';
const builtIn = (id?: string) => id === DEFAULT_ID || id === NONE_ID;
function presetName(name?: string) {
  return name==='$ID/kDefaultTrapStyleName'?tr('기본값'):name==='$ID/k[No Trap Preset]'?tr('트랩 없음'):name??'';
}
type Choice = [string, string];
const joins = (): Choice[] => [['MiterEndJoin', tr('마이터')], ['RoundEndJoin', tr('둥글게')], ['BevelEndJoin', tr('모따기')]];
const ends = (): Choice[] => [['MiterTrapEnds', tr('마이터')], ['OverlapTrapEnds', tr('겹침')]];
const placements = (): Choice[] => [['CenterEdges', tr('가운데')], ['Choke', tr('초크')], ['ImageNeutralDensity', tr('중성 밀도')], ['ImagesOverSpread', tr('스프레드 위 이미지')]];
type Num = 'default_width' | 'black_width' | 'step_threshold' | 'black_color_threshold' | 'black_density' | 'sliding_threshold' | 'color_reduction';
const numbers = (): [Num, string][] => [
  ['default_width', tr('기본 폭(pt)')], ['black_width', tr('검정 폭(pt)')], ['step_threshold', tr('단계 한계(%)')],
  ['black_color_threshold', tr('검정 색 한계(%)')], ['black_density', tr('검정 밀도')], ['sliding_threshold', tr('슬라이딩 트랩 한계(%)')],
  ['color_reduction', tr('트랩 색 감소(%)')],
];
type Flag = 'objects_to_images' | 'images_to_images' | 'internal_images' | 'one_bit_images';
const flags = (): [Flag, string][] => [
  ['objects_to_images', tr('개체를 이미지에 트랩')], ['images_to_images', tr('이미지를 이미지에 트랩')],
  ['internal_images', tr('이미지 내부 트랩')], ['one_bit_images', tr('1비트 이미지 트랩')],
];

function Options({draft,setDraft,existing}:{draft:TrapPreset;setDraft:(p:TrapPreset)=>void;existing:boolean}) {
  const select=(label:string,key:'join'|'end'|'image_placement',choices:Choice[])=>
    <label style={{display:'block'}}>{label} <select aria-label={label} value={draft[key]??''} onChange={e=>setDraft({...draft,[key]:e.target.value})}>
      {choices.map(([v,l])=><option key={v} value={v}>{l}</option>)}</select></label>;
  return <fieldset>
    <label style={{display:'block'}}>{tr('이름')} <input aria-label={tr('트랩 설정 이름')} value={draft.name??''} disabled={existing&&builtIn(draft.id)} onChange={e=>setDraft({...draft,name:e.target.value})}/></label>
    {/* an emptied box is a value of its own (null: the engine keeps the stored value or takes InDesign's default), as the old boxes had it */}
    {numbers().map(([key,label])=><label key={key} style={{display:'block'}}>{label} <NumberField value={draft[key]??null} onCommit={v=>setDraft({...draft,[key]:v})}
      min={-1e9} max={1e9} step={1} decimals={4} label={label} width={110} inDialog onClear={()=>setDraft({...draft,[key]:null})}/></label>)}
    {select(tr('연결 모양'),'join',joins())}
    {select(tr('끝 모양'),'end',ends())}
    {select(tr('이미지 트랩 위치'),'image_placement',placements())}
    {flags().map(([key,label])=><label key={key} style={{display:'block'}}><input type="checkbox" aria-label={label} checked={!!draft[key]} onChange={e=>setDraft({...draft,[key]:e.target.checked})}/> {label}</label>)}
  </fieldset>;
}

// The panel menu (menuaudit-c): what each item does, by command id.
const TP = 'id.panel.trapPresets.';
declarePanelCommands([TP+'newPreset', TP+'duplicatePreset', TP+'deletePresets', TP+'presetOptions', TP+'assignTrapPreset',
  // build-c: Load Trap Presets, Select All Unused, Small Panel Rows
  TP+'loadTrapPresets', TP+'selectAllUnused', TP+'smallPanelRows']);
/** Small Panel Rows: a view setting of this computer, kept in the user settings by the engine (`userSettings` values). */
const SMALL_ROWS = 'folio.trapPresets.smallRows';
// a new preset starts from InDesign's defaults
const NEW_PRESET:TrapPreset = {name:'',default_width:0.25,black_width:0.5,join:'MiterEndJoin',end:'MiterTrapEnds',objects_to_images:true,images_to_images:true,
  internal_images:false,one_bit_images:true,image_placement:'CenterEdges',step_threshold:10,black_color_threshold:100,black_density:1.6,sliding_threshold:70,color_reduction:100};

export function TrapPresetsPanel({ctx}:{ctx:PanelCtx}) {
  const [settings,setSettings]=useState<TrapSettings>({presets:[],assignments:[]});
  const [error,setError]=useState('');
  const [selected,setSelected]=useState('');
  const [draft,setDraft]=useState<TrapPreset|null>(null);
  const [replacement,setReplacement]=useState<string|null>(null);
  const [range,setRange]=useState('');
  // build-c: the presets Select All Unused marked (Delete Presets deletes them together), the Load form, Small Panel Rows
  const [marked,setMarked]=useState<string[]>([]);
  const [loadPath,setLoadPath]=useState<string|null>(null);
  const [small,setSmall]=useState(false);
  useEffect(()=>{let on=true;window.folio?.settings?.().then(s=>{if(on)setSmall(s.preferences?.[SMALL_ROWS]===true);}).catch(()=>{});return()=>{on=false;};},[]);
  const toggleSmall=()=>{const next=!small;setSmall(next);void window.folio.request({cmd:'userSettings',values:{[SMALL_ROWS]:next}}).catch(()=>{});};
  const reload=()=>window.folio.request({cmd:'trapPresets'}).then(r=>{
    if(r.header.ok)setSettings(r.header.traps as TrapSettings);else setError(String(r.header.error));
  }).catch(e=>setError(String(e)));
  useEffect(()=>{
    let active=true;setSettings({presets:[],assignments:[]});setError('');
    if(ctx.doc)void window.folio.request({cmd:'trapPresets'}).then(r=>{
      if(!active)return;
      if(r.header.ok)setSettings(r.header.traps as TrapSettings);else setError(String(r.header.error));
    }).catch(e=>{if(active)setError(String(e));});
    return ()=>{active=false;};
  },[ctx.doc,ctx.info]);
  const doc=ctx.doc;
  const current=settings.presets.find(p=>p.id===selected)??null;
  const define=async(label:string,commands:Parameters<PanelCtx['edit']>[1]):Promise<boolean>=>{
    setError('');
    try{const done=await ctx.edit(label,commands);if(!done){setError(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.'));return false;}await reload();return true;}
    catch(e){setError(String(e));return false;}
  };
  const run=async(label:string,command:Parameters<PanelCtx['edit']>[1][number]):Promise<boolean>=>{
    setError('');
    try{const done=await ctx.edit(label,[command]);if(!done){setError(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.'));return false;}await reload();return true;}
    catch(e){setError(String(e));return false;}
  };
  const pagesOf=(id:string)=>settings.assignments.filter(a=>a.preset===id).map(a=>doc?.pages.find(page=>page.id===a.page)?.name??'—').join(', ')||'—';
  const assign=async()=>{
    if(!current?.id||!doc)return;
    setError('');
    try{
      const ids=range.trim()===''?doc.pages.map(p=>p.id).filter((x):x is string=>!!x):await window.folio.request({cmd:'pageRange',range:range.trim()}).then(r=>{
        if(!r.header.ok)throw Error(String(r.header.error));return r.header.pages as string[];});
      await run(tr('트랩 설정 할당'),{op:'assignTrapPreset',pages:ids,preset:current.id});
    }catch(e){setError(String(e));}
  };
  const startEdit=(p:TrapPreset)=>{setError('');setReplacement(null);setDraft({...p});};
  const idle=ctx.editable&&!draft&&replacement===null;
  const duplicate=()=>current&&void run(tr('트랩 설정 복제'),{op:'duplicateTrapPreset',id:current.id??'',name:''});
  // presets no page uses (the two built-in ones cannot be deleted, so they are never marked)
  const unused=settings.presets.filter(p=>p.id&&!builtIn(p.id)&&!settings.assignments.some(a=>a.preset===p.id)).map(p=>p.id as string);
  const deleteMarked=async()=>{if(await define(tr('트랩 설정 삭제'),marked.map(id=>({op:'deleteTrapPreset' as const,id,replacement:DEFAULT_ID})))){setMarked([]);setSelected('');}};
  const loadPresets=async()=>{
    if(!loadPath?.trim())return;
    setError('');
    try{
      const {header}=await window.folio.request({cmd:'trapPresetsFile',path:loadPath.trim()});
      if(!header.ok){setError(String(header.error));return;}
      if(await define(tr('트랩 설정 불러오기'),[{op:'loadTrapPresets',parts:header.parts as Record<string,string>}]))setLoadPath(null);
    }catch(e){setError(String(e));}
  };
  usePanelCommands('trapPresets',{
    [TP+'loadTrapPresets']:idle&&ctx.doc?.folio?()=>setLoadPath(''):undefined,
    [TP+'selectAllUnused']:unused.length?()=>{setMarked(unused);setSelected(unused[0]);}:undefined,
    [TP+'smallPanelRows']:{run:toggleSmall,checked:small},
    [TP+'newPreset']:idle?()=>startEdit(NEW_PRESET):undefined,
    [TP+'duplicatePreset']:idle&&current?duplicate:undefined,
    [TP+'deletePresets']:idle&&marked.length>1?()=>void deleteMarked():idle&&current&&!builtIn(current.id)?()=>{setError('');setReplacement(DEFAULT_ID);}:undefined,
    [TP+'presetOptions']:idle&&current?()=>startEdit(current):undefined,
    [TP+'assignTrapPreset']:ctx.editable&&current?()=>void assign():undefined,
  });
  if(!doc)return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  return <section aria-label={tr('트랩 사전 설정')} style={{padding:8}}>
    <p>{tr('설정과 페이지 할당을 바꿀 수 있습니다.')}</p>
    <p className="muted small" data-trap-note>{tr('트랩 설정과 페이지 할당은 문서에 저장되고 IDML 로 함께 내보냅니다. 분판 출력에 트랩을 그리는 기능은 지원하지 않습니다.')}</p>
    {error && <p role="alert">{error}</p>}
    {!settings.presets.length && !error && <p>{tr('문서에 트랩 사전 설정이 없습니다.')}</p>}
    {loadPath!==null && <form data-trap-load="" onSubmit={e=>{e.preventDefault();void loadPresets();}}>
      <label>{tr('트랩 설정을 불러올 프로젝트 문서 (.folio, .idml)')} <input autoFocus aria-label={tr('트랩 설정을 불러올 문서 경로')} value={loadPath} onChange={e=>setLoadPath(e.target.value)}/></label>
      <button type="submit" disabled={!loadPath.trim()}>{tr('불러오기')}</button>
      <button type="button" onClick={()=>setLoadPath(null)}>{tr('취소')}</button>
    </form>}
    {marked.length>1 && <p role="status" data-trap-marked={marked.length}>{tr('안 쓰는 설정 {0}개를 골랐습니다. 설정 삭제로 함께 지웁니다.',{0:marked.length})}
      {ctx.editable && <button type="button" onClick={()=>void deleteMarked()}>{tr('설정 삭제')}</button>}</p>}
    <div data-trap-rows={small?'small':'normal'}>
    {settings.presets.map(p=><article key={p.id} aria-current={p.id===selected} tabIndex={0}
      style={p.id===selected||marked.includes(p.id??'')?{outline:'2px solid var(--accent)',...(small?{padding:'2px 4px'}:{})}:small?{padding:'2px 4px'}:undefined}
      onClick={()=>{setSelected(p.id??'');setMarked([]);}} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();setSelected(p.id??'');setMarked([]);}}}>
      <strong>{presetName(p.name)}</strong>
      {small?<span> · {pagesOf(p.id??'')}</span>:<>
      <div>{tr('기본 폭: {0} pt',{0:p.default_width??'—'})}</div>
      <div>{tr('검정 폭: {0} pt',{0:p.black_width??'—'})}</div>
      <div>{tr('적용된 페이지: {0}',{0:pagesOf(p.id??'')})}</div></>}
    </article>)}
    </div>
    {ctx.editable && !draft && replacement===null && <p>
      <button type="button" onClick={()=>startEdit(NEW_PRESET)}>{tr('새 설정')}</button>
      <button type="button" disabled={!current} onClick={()=>current&&startEdit(current)}>{tr('설정 옵션')}</button>
      <button type="button" disabled={!current} onClick={duplicate}>{tr('설정 복제')}</button>
      <button type="button" disabled={!current||builtIn(current.id)} onClick={()=>{setError('');setReplacement(DEFAULT_ID);}}>{tr('설정 삭제')}</button>
    </p>}
    {draft && <form aria-label={tr('트랩 설정 옵션')} onSubmit={async e=>{e.preventDefault();if(await run(draft.id?tr('트랩 설정 편집'):tr('새 트랩 설정'),{op:'saveTrapPreset',preset:draft}))setDraft(null);}}>
      <Options draft={draft} setDraft={setDraft} existing={!!draft.id}/>
      <button type="submit" disabled={!(draft.name??'').trim()}>{tr('확인')}</button>
      <button type="button" onClick={()=>{setDraft(null);setError('');}}>{tr('취소')}</button>
    </form>}
    {replacement!==null && current && <p>
      <label>{tr('대체할 설정')} <select aria-label={tr('대체할 설정')} value={replacement} onChange={e=>setReplacement(e.target.value)}>
        {settings.presets.filter(p=>p.id!==current.id).map(p=><option key={p.id} value={p.id}>{presetName(p.name)}</option>)}</select></label>
      <button type="button" onClick={async()=>{if(await run(tr('트랩 설정 삭제'),{op:'deleteTrapPreset',id:current.id??'',replacement})){setReplacement(null);setSelected('');}}}>{tr('삭제')}</button>
      <button type="button" onClick={()=>setReplacement(null)}>{tr('취소')}</button>
    </p>}
    {ctx.editable && <p>
      <label>{tr('페이지 범위(비우면 전체)')} <input aria-label={tr('트랩 할당 페이지 범위')} value={range} placeholder="+2-+4" onChange={e=>setRange(e.target.value)}/></label>
      <button type="button" disabled={!current} onClick={()=>void assign()}>{tr('선택한 설정 할당')}</button>
    </p>}
  </section>;
}
