// Glyphs panel ▸ glyph sets (build-c). The sets are the person's (the engine keeps them in the user settings: `glyphSets`); the panel
// picks one in its "Glyph Set" box and the menu items and buttons work on that set: add the chosen glyph, edit (name, insert order,
// Remember Font per glyph, remove glyphs), view (the Show list shows it), delete; the sets go to and come from a `.folioglyphs` file in the project.
import {useEffect,useState} from 'react';
import {tr} from '../../i18n';
import {glyphSets,type GlyphSet,type SetGlyph} from './query';

export const SET_SHOW='set:';
export type SetsState=ReturnType<typeof useGlyphSets>;

export function useGlyphSets() {
  const [sets,setSets]=useState<GlyphSet[]>([]);
  const [target,setTarget]=useState('');
  const [error,setError]=useState('');
  const [form,setForm]=useState<null|'new'|'edit'|'import'|'export'>(null);
  useEffect(()=>{let on=true;glyphSets({op:'list'}).then(r=>{if(on)setSets(r.sets);}).catch(()=>{});return()=>{on=false;};},[]);
  // the chosen set stays a set that exists
  useEffect(()=>{if(!sets.some(s=>s.name===target))setTarget(sets[0]?.name??'');},[sets]);
  const run=async(request:Record<string,unknown>)=>{
    setError('');
    try {const r=await glyphSets(request);setSets(r.sets);return r;}
    catch(e){setError(String((e as Error).message??e));return null;}
  };
  const current=sets.find(s=>s.name===target)??null;
  return {sets,target,setTarget,current,error,form,setForm,run};
}

const ORDERS:[GlyphSet['order'],()=>string][]=[['front',()=>tr('앞에 넣기')],['end',()=>tr('끝에 붙이기')],['unicode',()=>tr('Unicode 순서')]];

function NewSetForm({state}:{state:SetsState}) {
  const [name,setName]=useState(tr('글리프 세트 {0}',{0:state.sets.length+1})),[order,setOrder]=useState<GlyphSet['order']>('front');
  const ok=async()=>{const r=await state.run({op:'create',name,order});if(r?.created){state.setTarget(r.created);state.setForm(null);}};
  return <form className="glyph-set-form" data-glyph-set-form="new" onSubmit={e=>{e.preventDefault();void ok();}}>
    <label>{tr('이름')}<input aria-label={tr('글리프 세트 이름')} value={name} autoFocus onChange={e=>setName(e.target.value)}/></label>
    <label>{tr('삽입 순서')}<select aria-label={tr('삽입 순서')} value={order} onChange={e=>setOrder(e.target.value as GlyphSet['order'])}>
      {ORDERS.map(([v,l])=><option key={v} value={v}>{l()}</option>)}</select></label>
    <div className="glyph-set-actions"><button type="submit" disabled={!name.trim()}>{tr('확인')}</button><button type="button" onClick={()=>state.setForm(null)}>{tr('취소')}</button></div>
  </form>;
}

function EditSetForm({state,set}:{state:SetsState;set:GlyphSet}) {
  const [name,setName]=useState(set.name),[order,setOrder]=useState(set.order),[glyphs,setGlyphs]=useState<SetGlyph[]>(set.glyphs);
  const ok=async()=>{const r=await state.run({op:'edit',name:set.name,newName:name,order,glyphs});if(r){state.setTarget(name.trim());state.setForm(null);}};
  return <form className="glyph-set-form" data-glyph-set-form="edit" onSubmit={e=>{e.preventDefault();void ok();}}>
    <label>{tr('이름')}<input aria-label={tr('글리프 세트 이름')} value={name} onChange={e=>setName(e.target.value)}/></label>
    <label>{tr('삽입 순서')}<select aria-label={tr('삽입 순서')} value={order} onChange={e=>setOrder(e.target.value as GlyphSet['order'])}>
      {ORDERS.map(([v,l])=><option key={v} value={v}>{l()}</option>)}</select></label>
    <ul className="glyph-set-glyphs" aria-label={tr('세트의 글리프')}>
      {glyphs.map((g,i)=><li key={`${g.family}\t${g.style}\t${g.id}`} data-set-glyph={g.id}>
        <span>{g.cp!==null?String.fromCodePoint(g.cp):'□'} · GID {g.id} · {g.family} {g.style}</span>
        <label className="check"><input type="checkbox" checked={g.rememberFont} onChange={e=>setGlyphs(glyphs.map((x,k)=>k===i?{...x,rememberFont:e.target.checked}:x))}/>{tr('글리프와 함께 글꼴 기억')}</label>
        <button type="button" aria-label={tr('세트에서 글리프 삭제')} onClick={()=>setGlyphs(glyphs.filter((_,k)=>k!==i))}>×</button>
      </li>)}
      {!glyphs.length&&<li>{tr('세트에 글리프가 없습니다.')}</li>}
    </ul>
    <div className="glyph-set-actions"><button type="submit" disabled={!name.trim()}>{tr('확인')}</button><button type="button" onClick={()=>state.setForm(null)}>{tr('취소')}</button></div>
  </form>;
}

function FileForm({state,kind}:{state:SetsState;kind:'import'|'export'}) {
  const [path,setPath]=useState('glyphs.folioglyphs'),[done,setDone]=useState('');
  const ok=async()=>{
    const r=await state.run(kind==='export'?{op:'export',path}:{op:'import',path});
    if(r?.written)setDone(tr('글리프 세트 {0}개를 {1}에 내보냈습니다.',{0:r.written.sets,1:r.written.path}));
    if(r?.imported){setDone(tr('글리프 세트를 불러왔습니다: {0}',{0:r.imported.join(', ')}));if(r.imported[0])state.setTarget(r.imported[0]);}
  };
  return <form className="glyph-set-form" data-glyph-set-form={kind} onSubmit={e=>{e.preventDefault();void ok();}}>
    <label>{tr('프로젝트 상대 .folioglyphs 경로')}<input aria-label={tr('프로젝트 상대 .folioglyphs 경로')} value={path} autoFocus onChange={e=>setPath(e.target.value)}/></label>
    <div className="glyph-set-actions"><button type="submit" disabled={!path.trim()}>{kind==='export'?tr('내보내기'):tr('불러오기')}</button><button type="button" onClick={()=>state.setForm(null)}>{tr('닫기')}</button></div>
    {done&&<p role="status">{done}</p>}
  </form>;
}

/** The panel's Glyph Set box and buttons, and the open form (new, edit, load, export). */
export function GlyphSetsBlock({state,canAdd,onAdd,onView}:{state:SetsState;canAdd:boolean;onAdd:()=>void;onView:()=>void}) {
  return <fieldset className="glyph-sets" data-glyph-sets={state.sets.length}>
    <legend>{tr('글리프 세트')}</legend>
    <div className="glyph-set-row">
      <select aria-label={tr('글리프 세트')} value={state.target} disabled={!state.sets.length} onChange={e=>state.setTarget(e.target.value)}>
        {!state.sets.length&&<option value="">{tr('세트 없음')}</option>}
        {state.sets.map(s=><option key={s.name} value={s.name}>{tr('{0} ({1})',{0:s.name,1:s.glyphs.length})}</option>)}
      </select>
      <button type="button" onClick={()=>state.setForm('new')}>{tr('새 세트…')}</button>
      <button type="button" disabled={!canAdd||!state.current} onClick={onAdd}>{tr('세트에 추가')}</button>
      <button type="button" disabled={!state.current} onClick={onView}>{tr('보기')}</button>
      <button type="button" disabled={!state.current} onClick={()=>state.setForm('edit')}>{tr('편집…')}</button>
    </div>
    {state.form==='new'&&<NewSetForm state={state}/>}
    {state.form==='edit'&&state.current&&<EditSetForm key={state.current.name} state={state} set={state.current}/>}
    {(state.form==='import'||state.form==='export')&&<FileForm key={state.form} state={state} kind={state.form}/>}
    {state.error&&<p role="alert">{tr('글리프 세트: {0}',{0:state.error})}</p>}
  </fieldset>;
}
