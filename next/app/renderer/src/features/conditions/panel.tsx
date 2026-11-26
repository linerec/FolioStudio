import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../../i18n';
import type { PanelCtx } from '../../ui/panels';
import { declarePanelCommands, usePanelCommands } from '../../ui/panelbus';
import { Dropdown } from '../../ui/menu';
import type { Commands } from '../../ui/commands';

/** A condition set: its id, name and the visibility it gives each condition (condition id → visible). */
type ConditionSet = { id:string; name:string; conditions:Record<string,boolean> };

type Condition = { id:string; name:string; visible:boolean; indicator_method?:string; underline_appearance?:string; indicator_color?:string };
type Draft = { name:string; method:string; appearance:string; color:string; custom:string };
const CUSTOM = '__custom';
const draftOf = (c:Condition):Draft => {
  const color = c.indicator_color??'';
  return { name:c.name, method:c.indicator_method||'UseUnderline', appearance:c.underline_appearance||'Wavy', color:color.includes(',')?CUSTOM:color, custom:color.includes(',')?color:'0,0,0' };
};
type Indicators = 'HIDE_INDICATORS'|'SHOW_INDICATORS'|'SHOW_AND_PRINT_INDICATORS';

export const CONDITION_PANELS = {
  conditionalText: { get title(){return tr('조건부 텍스트');}, icon:'type', width:320, height:360 },
};

// The panel menu (menuaudit-c): what each item does, by command id.
const K = 'id.panel.conditionalText.';
declarePanelCommands([K+'newCondition', K+'deleteCondition', K+'conditionOptions', K+'showAll', K+'hideAll',
  // build-c: Load Conditions (and Sets), Select All Unused
  K+'loadConditions', K+'loadConditionsAndSets', K+'selectAllUnused']);

export function ConditionsPanel({ctx}:{ctx:PanelCtx}) {
  const [conditions, setConditions] = useState<Condition[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [indicators, setIndicators] = useState<Indicators>('SHOW_INDICATORS');
  const [name, setName] = useState('');
  const [colors, setColors] = useState<string[]>([]);
  const [draft, setDraft] = useState<Draft|null>(null);
  const nameInput = useRef<HTMLInputElement>(null), optionsBox = useRef<HTMLFieldSetElement>(null);
  // condition sets and the set in use, which conditions the text uses (Select All Unused), the Load form and the row's right-click menu (build-c)
  const [sets, setSets] = useState<ConditionSet[]>([]);
  const [activeSet, setActiveSet] = useState('n');
  const [usage, setUsage] = useState<Record<string,number>>({});
  const [newSetName, setNewSetName] = useState('');
  const [load, setLoad] = useState<null|{sets:boolean}>(null);
  const [loadPath, setLoadPath] = useState('');
  const [rowMenu, setRowMenu] = useState<null|{x:number;y:number;id:string}>(null);
  useEffect(()=>{
    let current = true;
    setConditions([]); setSelected([]); setError('');
    if(ctx.doc) void window.folio.request({cmd:'conditions'}).then(r=>{
      if(!current)return;
      if(r.header.ok) { setConditions(r.header.conditions as Condition[]); setIndicators((r.header.indicators as Indicators|undefined)??'SHOW_INDICATORS'); setColors((r.header.colors as string[]|undefined)??[]);
        setSets((r.header.sets as ConditionSet[]|undefined)??[]); setActiveSet(String(r.header.activeSet??'n')); setUsage((r.header.usage as Record<string,number>|undefined)??{}); }
      else setError(String(r.header.error));
    }).catch(e=>{if(current)setError(String(e));});
    return ()=>{current=false;};
  },[ctx.doc,ctx.info]);
  const range = ctx.textFormatting.selection;
  const canApply = ctx.editable && !ctx.readOnly && range && range.start < range.end && !range.nested && !busy;
  const apply = async(ids:string[])=>{
    if(!canApply || !range)return;
    setBusy(true);
    try { await ctx.edit(tr('조건 적용'),[{op:'applyConditions',story:range.story,start:range.start,end:range.end,conditions:ids}]); }
    finally { setBusy(false); }
  };
  const define = async(label:string, ops:Parameters<PanelCtx['edit']>[1]):Promise<boolean>=>{
    if(!ctx.editable || ctx.readOnly || busy)return false;
    setBusy(true); setError('');
    try { const r = await ctx.edit(label, ops); if(!r) setError(tr('조건을 바꾸지 못했습니다.')); return !!r; }
    catch(e) { setError(String(e)); return false; }
    finally { setBusy(false); }
  };
  // Condition Options: the one checked condition
  const only = selected.length===1 ? conditions.find(c=>c.id===selected[0]) : undefined;
  useEffect(()=>{ setDraft(only?draftOf(only):null); },[only?.id,only?.name,only?.indicator_method,only?.underline_appearance,only?.indicator_color]);
  const optionProps = ():Record<string,string>=>{
    if(!only || !draft) return {};
    const props:Record<string,string> = {};
    const color = (draft.color===CUSTOM?draft.custom:draft.color).replace(/\s/g,'');
    if(draft.name!==only.name) props.name = draft.name;
    if(draft.method!==(only.indicator_method||'UseUnderline')) props.method = draft.method;
    if(draft.appearance!==(only.underline_appearance||'Wavy')) props.appearance = draft.appearance;
    if(color!==(only.indicator_color??'')) props.color = color;
    return props;
  };
  const changes = optionProps();
  const applyOptions = async()=>{
    if(!only || !Object.keys(changes).length)return;
    if(await define(tr('조건 옵션'),[{op:'setConditionOptions',name:only.id,props:changes}]) && changes.name) setSelected(['Condition/'+changes.name]);
  };
  const canDefine = ctx.editable && !ctx.readOnly && !busy;
  const createNamed = ()=>void define(tr('조건 만들기'),[{op:'createCondition',name}]).then(ok=>{ if(ok) setName(''); });
  const deleteSelected = ()=>void define(tr('조건 삭제'),selected.map(id=>({op:'deleteCondition' as const,name:id}))).then(ok=>{ if(ok) setSelected([]); });
  // Show All / Hide All: every condition that is not yet so, in one step
  const setAllVisible = (visible:boolean)=>{
    const ops = conditions.filter(c=>c.visible!==visible).map(c=>({op:'setConditionVisible' as const,name:c.id,visible}));
    return canDefine && ops.length ? ()=>void define(visible?tr('모든 조건 보이기'):tr('모든 조건 숨기기'),ops) : undefined;
  };
  // Load Conditions (and Sets): the other project document is read by the engine (conditionsFile), then one edit loads its definitions
  const runLoad = async()=>{
    if(!load || !loadPath.trim())return;
    setError('');
    try {
      const {header} = await window.folio.request({cmd:'conditionsFile',path:loadPath.trim()});
      if(!header.ok) { setError(String(header.error)); return; }
      if(await define(load.sets?tr('조건 및 세트 불러오기'):tr('조건 불러오기'),[{op:'loadConditions',parts:header.parts as Record<string,string>,sets:load.sets}])) setLoad(null);
    } catch(e) { setError(String(e)); }
  };
  const unused = conditions.filter(c=>!usage[c.id]).map(c=>c.id);
  // the set in use; when a condition's visibility no longer matches it, the box says so (choosing the set again applies it)
  const active = sets.find(s=>s.id===activeSet);
  const shownSet = !active ? 'n' : conditions.every(c=>active.conditions[c.id]===undefined||active.conditions[c.id]===c.visible) ? active.id : '+';
  const rowCommands = (id:string):Commands=>{
    const c = conditions.find(x=>x.id===id);
    const label = c?.name??id;
    return {
      'cond.unapply': { label: tr('"{0}" 해제',{0:label}), run: canApply && range ? ()=>void define(tr('조건 해제'),[{op:'unapplyCondition',story:range.story,start:range.start,end:range.end,condition:id}]) : undefined },
      'cond.edit': { label: tr('"{0}" 편집…',{0:label}), run: ()=>{ setSelected([id]); requestAnimationFrame(()=>optionsBox.current?.querySelector('input')?.focus()); } },
      'cond.delete': { label: tr('"{0}" 삭제',{0:label}), run: canDefine ? ()=>void define(tr('조건 삭제'),[{op:'deleteCondition',name:id}]).then(ok=>{ if(ok) setSelected(old=>old.filter(x=>x!==id)); }) : undefined },
    };
  };
  usePanelCommands('conditionalText', {
    [K+'loadConditions']: canDefine && ctx.doc?.folio ? ()=>setLoad({sets:false}) : undefined,
    [K+'loadConditionsAndSets']: canDefine && ctx.doc?.folio ? ()=>setLoad({sets:true}) : undefined,
    [K+'selectAllUnused']: unused.length ? ()=>setSelected(unused) : undefined,
    [K+'newCondition']: canDefine ? (name.trim() ? createNamed : ()=>nameInput.current?.focus()) : undefined,
    [K+'deleteCondition']: canDefine && selected.length ? deleteSelected : undefined,
    [K+'conditionOptions']: only && draft ? ()=>optionsBox.current?.querySelector('input')?.focus() : undefined,
    [K+'showAll']: setAllVisible(true),
    [K+'hideAll']: setAllVisible(false),
  });
  if(!ctx.doc)return <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
  return <section aria-label={tr('조건부 텍스트')} style={{padding:8}}>
    {error && <p role="alert">{error}</p>}
    {!conditions.length && !error && <p>{tr('문서에 조건이 없습니다.')}</p>}
    <div>{conditions.map(c=><label key={c.id} style={{display:'block'}} data-condition={c.id} onContextMenu={e=>{e.preventDefault();e.stopPropagation();setRowMenu({x:e.clientX,y:e.clientY,id:c.id});}}>
      <input type="checkbox" checked={selected.includes(c.id)} onChange={e=>setSelected(old=>e.target.checked?[...old,c.id]:old.filter(id=>id!==c.id))}/>
      {c.name} — {c.visible?tr('표시'):tr('숨김')}
      {' '}<button disabled={!canDefine} onClick={()=>void define(c.visible?tr('조건 숨기기'):tr('조건 보이기'),[{op:'setConditionVisible',name:c.id,visible:!c.visible}])}>{c.visible?tr('조건 숨기기'):tr('조건 보이기')}</button>
    </label>)}</div>
    <div>
      <input ref={nameInput} type="text" value={name} aria-label={tr('새 조건 이름')} onChange={e=>setName(e.target.value)}/>
      <button disabled={!canDefine || !name} onClick={createNamed}>{tr('조건 만들기')}</button>
      <button disabled={!canDefine || !selected.length} onClick={deleteSelected}>{tr('조건 삭제')}</button>
    </div>
    <fieldset className="cond-sets" data-condition-sets={sets.length} data-active-set={activeSet}>
      <legend>{tr('조건 세트')}</legend>
      <label>{tr('세트')}{' '}<select aria-label={tr('조건 세트')} value={shownSet} disabled={!canDefine || !sets.length}
        onChange={e=>{ if(e.target.value!=='n'&&e.target.value!=='+') void define(tr('조건 세트 적용'),[{op:'applyConditionSet',name:e.target.value}]); }}>
        <option value="n">{tr('[이름 없음]')}</option>{active&&shownSet==='+'&&<option value="+">{tr('{0} (바뀜)',{0:active.name})}</option>}
        {sets.map(s=><option key={s.id} value={s.id}>{s.name}</option>)}
      </select></label>
      <div>
        <input type="text" value={newSetName} aria-label={tr('새 세트 이름')} onChange={e=>setNewSetName(e.target.value)}/>
        <button disabled={!canDefine || !newSetName.trim() || !conditions.length} onClick={()=>void define(tr('새 조건 세트'),[{op:'createConditionSet',name:newSetName}]).then(ok=>{ if(ok) setNewSetName(''); })}>{tr('새 세트')}</button>
        <button disabled={!canDefine || activeSet==='n'} onClick={()=>void define(tr('조건 세트 다시 정의'),[{op:'redefineConditionSet',name:activeSet}])}>{tr('다시 정의')}</button>
        <button disabled={!canDefine || activeSet==='n'} onClick={()=>void define(tr('조건 세트 삭제'),[{op:'deleteConditionSet',name:activeSet}])}>{tr('세트 삭제')}</button>
      </div>
    </fieldset>
    {load && <form className="cond-load" data-condition-load={load.sets?'sets':'conditions'} onSubmit={e=>{e.preventDefault();void runLoad();}}>
      <label style={{display:'block'}}>{tr('조건을 불러올 프로젝트 문서 (.folio, .idml)')}{' '}<input type="text" autoFocus value={loadPath} aria-label={tr('조건을 불러올 문서 경로')} onChange={e=>setLoadPath(e.target.value)}/></label>
      <button type="submit" disabled={!loadPath.trim() || busy}>{load.sets?tr('조건 및 세트 불러오기'):tr('조건 불러오기')}</button>
      <button type="button" onClick={()=>setLoad(null)}>{tr('취소')}</button>
    </form>}
    <label style={{display:'block'}}>{tr('조건 표시자')}{' '}
      <select value={indicators} disabled={!canDefine} onChange={e=>void define(tr('조건 표시자'),[{op:'setConditionIndicators',mode:e.target.value}])}>
        <option value="HIDE_INDICATORS">{tr('표시자 숨기기')}</option>
        <option value="SHOW_INDICATORS">{tr('표시자 보이기')}</option>
        <option value="SHOW_AND_PRINT_INDICATORS">{tr('표시자 보이기와 인쇄')}</option>
      </select>
    </label>
    {only && draft && <fieldset ref={optionsBox} aria-label={tr('조건 옵션')}>
      <legend>{tr('조건 옵션')}</legend>
      <label style={{display:'block'}}>{tr('이름')}{' '}<input type="text" value={draft.name} onChange={e=>setDraft({...draft,name:e.target.value})}/></label>
      <label style={{display:'block'}}>{tr('표시 방법')}{' '}
        <select value={draft.method} onChange={e=>setDraft({...draft,method:e.target.value})}>
          <option value="UseUnderline">{tr('밑줄')}</option>
          <option value="UseHighlight">{tr('강조')}</option>
        </select></label>
      <label style={{display:'block'}}>{tr('밑줄 모양')}{' '}
        <select value={draft.appearance} disabled={draft.method!=='UseUnderline'} onChange={e=>setDraft({...draft,appearance:e.target.value})}>
          <option value="Solid">{tr('실선')}</option>
          <option value="Wavy">{tr('물결')}</option>
          <option value="Dashed">{tr('파선')}</option>
        </select></label>
      <label style={{display:'block'}}>{tr('색')}{' '}
        <select value={draft.color} onChange={e=>setDraft({...draft,color:e.target.value})}>
          {colors.map(c=><option key={c} value={c}>{c}</option>)}
          <option value={CUSTOM}>{tr('사용자 색')}</option>
        </select>
        {draft.color===CUSTOM && <input type="text" value={draft.custom} aria-label={tr('사용자 색 (r,g,b)')} onChange={e=>setDraft({...draft,custom:e.target.value})}/>}
      </label>
      <button disabled={!canDefine || !draft.name.trim() || !Object.keys(changes).length} onClick={()=>void applyOptions()}>{tr('조건 옵션 적용')}</button>
    </fieldset>}
    <p>{tr('선택한 글의 조건을 고른 조건으로 바꿉니다.')}</p>
    <button disabled={!canApply || !selected.some(id=>conditions.some(c=>c.id===id && c.visible))} onClick={()=>void apply(selected)}>{tr('조건 적용')}</button>
    <button disabled={!canApply} onClick={()=>void apply([])}>{tr('조건 해제')}</button>
    {rowMenu && createPortal(<Dropdown items={[{cmd:'cond.unapply'},{cmd:'cond.edit'},{cmd:'cond.delete'}]} commands={rowCommands(rowMenu.id)} x={rowMenu.x} y={rowMenu.y} onClose={()=>setRowMenu(null)}/>, document.body)}
  </section>;
}
