import { paragraphPages } from '../paragraphoptions/fields';
import { useNumberingListChoices } from '../numberinglists';
import { t as tr } from '../../i18n';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand, GrepStyle, NestedLineStyle, NestedStyle, StyleInfo } from '../../engine';
import { GrepFields } from './grep';
import { NestedFields } from './nested';
import { NestedLineFields } from './nested-lines';
import type { PanelCtx } from '../panels';
import { fieldKey } from '../keys';
import { StyleShortcutField } from './shortcutfield';
import { styleAlignments, styleFields, stylePages, type StylePage } from './fields';

import { StyleInputs, numericStyleValues } from './inputs';

type Kind = 'paragraph' | 'character';
type Mode = 'create' | 'edit' | 'delete';
type Values = Record<string, string | number | boolean | null>;

export function StyleDialog({ctx,kind,mode,info,group=[],choose,close}:{ctx:PanelCtx;kind:Kind;mode:Mode;info:StyleInfo;group?:string[];choose:(id:string)=>void;close:()=>void}) {
  const list=(kind==='paragraph'?ctx.info?.paragraphStyles:ctx.info?.characterStyles)??[];
  const selection=ctx.textFormatting.selection;
  const [page,setPage]=useState<StylePage>('general');
  const [grep,setGrep]=useState<GrepStyle[]>(info.grepStyles??[]),[grepDirty,setGrepDirty]=useState(false);
  const [nested,setNested]=useState<NestedStyle[]>(info.nestedStyles??[]),[nestedDirty,setNestedDirty]=useState(false);
  const [lineItems,setLineItems]=useState<NestedLineStyle[]>(info.lineStyles??[]),[linesDirty,setLinesDirty]=useState(false);
  const [shortcut,setShortcut]=useState(mode==='create'?'':info.shortcut??''),[shortcutLabel,setShortcutLabel]=useState(mode==='create'?'':info.shortcutLabel??''),[shortcutDirty,setShortcutDirty]=useState(false);
  const [name,setName]=useState(mode==='create'?(kind==='paragraph'?tr('새 문단 스타일'):tr('새 문자 스타일')):info.name);
  const [values,setValues]=useState<Values>(()=>({
    ...(mode==='create'?selection?.format:info.props),basedOn:mode==='create'?info.id:info.basedOn,nextStyle:mode==='create'?'':info.nextStyle
  }));
  const [dirty,setDirty]=useState<Record<string,string|number|boolean>>({}),[error,setError]=useState(''),[busy,setBusy]=useState(false);
  const [replacement,setReplacement]=useState(list.find(s=>s.id!==info.id)?.id??'');
  const set=(key:string,value:string|number|boolean)=>{setError('');setValues(v=>({...v,[key]:value}));setDirty(v=>({...v,[key]:value}));};
  const lists=useNumberingListChoices();
  const fields=styleFields((ctx.info?.characterStyles??[]).map(s=>({value:s.id,label:s.name})),lists,ctx.info?.swatches).filter(f=>kind==='paragraph'||f.page==='character');
  const submit=async()=>{
    setError('');
    if(info.protected&&mode!=='create'){setError(tr('[스타일 없음]과 [없음]은 고칠 수 없습니다.'));return;}
    const trimmed=name.trim();
    // a style in a group has the groups before its name (`Heads%3aName`)
    const prefix=info.id.slice(0,Math.max(info.id.lastIndexOf('/'),info.id.lastIndexOf('%3a')+2)+1);
    // a built-in style keeps its id: its panel name ([Basic Paragraph]) is not what its id says and its name is not changed (the DOM refuses)
    const nextId=mode==='create'?`${kind==='paragraph'?'ParagraphStyle':'CharacterStyle'}/${[...group,trimmed].join('%3a')}`:info.builtIn?info.id:`${prefix}${trimmed}`;
    if(mode!=='delete'&&(!trimmed||/[\/\x00-\x1f\x7f]/.test(trimmed)||list.some(s=>s.id===nextId&&(mode==='create'||s.id!==info.id)))) {
      setError(tr('겹치지 않는 스타일 이름을 입력하세요.'));return;
    }
    const props=numericStyleValues(fields,dirty);
    if(!props){setError(tr('수치를 확인하세요.'));return;}
    if(props.nextStyle==='')delete props.nextStyle;
    if(mode==='create'&&!selection){setError(tr('먼저 글을 선택하세요.'));return;}
    setBusy(true);
    try {
      let result;
      if(mode==='delete') result=await ctx.edit(tr('스타일 삭제'),[{op:'styleDelete',kind,style:info.id,replacement}]);
      else if(mode==='create') {
        const commands:EditCommand[]=[{op:'styleCreate',kind,name:trimmed,story:selection!.story,start:selection!.start,end:selection!.end,props,...(group.length?{group:group.join(':')}:{})}];
        if(grepDirty)commands.push({op:'setGrepStyles',style:nextId,rules:grep.map(({style,expression})=>({style,expression}))});
        if(nestedDirty)commands.push({op:'setNestedStyles',style:nextId,items:nested.map(x=>({...x,source:undefined}))});
        if(linesDirty)commands.push({op:'setLineStyles',style:nextId,items:lineItems.map(x=>({...x,source:undefined}))});
        if(shortcutDirty&&shortcut)commands.push({op:'setStyleShortcut',kind,style:nextId,shortcut});
        result=await ctx.edit(tr('선택에서 스타일 만들기'),commands);
      }
      else {
        if(trimmed!==info.name&&!info.builtIn)props.name=trimmed;
        const commands:EditCommand[]=[{op:'styleEdit',kind,style:info.id,props}];
        if(grepDirty)commands.push({op:'setGrepStyles',style:nextId,rules:grep});
        if(nestedDirty)commands.push({op:'setNestedStyles',style:nextId,items:nested});
        if(linesDirty)commands.push({op:'setLineStyles',style:nextId,items:lineItems});
        if(shortcutDirty)commands.push({op:'setStyleShortcut',kind,style:nextId,shortcut});
        result=await ctx.edit(tr('스타일 옵션'),commands);
      }
      if(!result){setError(tr('스타일을 저장하지 못했습니다. 입력값을 확인하세요.'));return;}
      choose(mode==='delete'?replacement:nextId);close();
    } catch(e) {setError(String(e));}
    finally {setBusy(false);}
  };
  const summary=fields.filter(f=>values[f.key]!=null&&values[f.key]!=='').map(f=>`${f.label}: ${f.choices?.find(c=>c.value===String(values[f.key]))?.label??(values[f.key]==='auto'?tr('자동'):values[f.key])}`);
  if(kind==='paragraph'&&values.justification)summary.push(`${tr('정렬')}: ${styleAlignments().find(([key])=>key===values.justification)?.[1]??values.justification}`);
  return createPortal(<div className="style-modal" role="dialog" aria-label={mode==='delete'?tr('스타일 삭제'):tr('스타일 옵션')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='cancel'&&!busy)close();}}>
    <form className="card dialog style-options type-style-options" onSubmit={e=>{e.preventDefault();if(!busy)void submit();}}>
      <h2>{mode==='delete'?tr('스타일 삭제'):mode==='create'?tr('선택에서 새 스타일'):tr('스타일 옵션')}</h2>
      {mode!=='delete'&&<div className="type-style-tabs" role="tablist" aria-label={tr('스타일 범주')}>
        {stylePages().filter(p=>kind==='paragraph'||p.id==='general'||p.id==='character').map(p=><button type="button" role="tab" aria-selected={page===p.id} key={p.id} onClick={()=>setPage(p.id)}>{p.label}</button>)}
      </div>}
      {(page==='general'||mode==='delete')&&<>
        <label>{tr('이름')}<input aria-label={tr('스타일 이름')} autoFocus value={name} disabled={mode==='delete'||busy||mode==='edit'&&!!info.builtIn} onChange={e=>{setError('');setName(e.target.value);}}/></label>
        {mode==='delete'?<label>{tr('바꿀 스타일')}<select aria-label={tr('바꿀 스타일')} value={replacement} onChange={e=>setReplacement(e.target.value)}>
          {list.filter(s=>s.id!==info.id).map(s=><option key={s.id} value={s.id}>{s.name}</option>)}</select></label>:<>
          <label>{tr('기준')}<select aria-label={tr('기준 스타일')} value={String(values.basedOn??'')} onChange={e=>set('basedOn',e.target.value)}>
            {list.filter(s=>mode==='create'||s.id!==info.id).map(s=><option key={s.id} value={s.id}>{[...s.group,s.name].join(' / ')}</option>)}</select></label>
          {kind==='paragraph'&&<label>{tr('다음 스타일')}<select aria-label={tr('다음 스타일')} value={String(values.nextStyle??'')} onChange={e=>set('nextStyle',e.target.value)}>
            {mode==='create'&&<option value="">{tr('같은 스타일')}</option>}{list.map(s=><option key={s.id} value={s.id}>{s.id===info.id&&mode==='edit'?name:s.name}</option>)}</select></label>}
          <StyleShortcutField kind={kind} selfId={mode==='edit'?info.id:undefined} spec={shortcut} label={shortcutLabel} disabled={busy||info.protected&&mode!=='create'} onChange={(spec,label)=>{setShortcut(spec);setShortcutLabel(label);setShortcutDirty(true);setError('');}}/>
          <details className="type-style-summary" open><summary>{tr('스타일 설정 요약')}</summary><p>{summary.join(' · ')||tr('(상속)')}</p></details>
        </>}
      </>}
      {mode!=='delete'&&page!=='general'&&<div role="tabpanel">
        {page==='nestedStyles'&&<NestedFields items={nested} styles={ctx.info?.characterStyles??[]} disabled={busy||info.protected&&mode!=='create'} set={items=>{setNested(items);setNestedDirty(true);setError('');}}/>}
        {page==='nestedStyles'&&<NestedLineFields items={lineItems} styles={ctx.info?.characterStyles??[]} disabled={busy||info.protected&&mode!=='create'} set={items=>{setLineItems(items);setLinesDirty(true);setError('');}}/>}
        {page==='grepStyles'&&<GrepFields rules={grep} styles={ctx.info?.characterStyles??[]} disabled={busy||info.protected&&mode!=='create'} set={rules=>{setGrep(rules);setGrepDirty(true);setError('');}}/>}
        {paragraphPages().filter(p=>p.id===page&&p.note).map(p=><p className="differs-note" data-note={p.id} key={p.id}>{p.note}</p>)}
        <StyleInputs fields={fields.filter(f=>f.page===page)} values={values} set={set}/>
        {page==='paragraph'&&<label>{tr('정렬')}<select aria-label={tr('스타일 정렬')} value={String(values.justification??'LEFT_ALIGN')} onChange={e=>set('justification',e.target.value)}>
          {styleAlignments().map(([value,label])=><option key={value} value={value}>{label}</option>)}</select></label>}
      </div>}
      {error&&<p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy||!ctx.editable||mode==='delete'&&!replacement}>{tr('확인')}</button></div>
    </form>
  </div>,document.body);
}
