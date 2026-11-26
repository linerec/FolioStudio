import { useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import type { PanelCtx } from './panels';
import { fieldKey } from './keys';
import { boxDecorationPages, decorationFields, decorationPages, ruleDecorationPages, type DecorationPage } from './textstyles/decorations';
import { StyleInputs, numericStyleValues, type StyleValues } from './textstyles/inputs';

export type DecorationsMode='rules'|'boxes';
export function ParagraphDecorationsDialog({ctx,close,mode='rules'}:{ctx:PanelCtx;close:()=>void;mode?:DecorationsMode}) {
  const [selection]=useState(ctx.textFormatting.selection);
  const pages=mode==='rules'?ruleDecorationPages:boxDecorationPages;
  const title=mode==='rules'?tr('단락 경계선'):tr('단락 테두리 및 음영');
  const [page,setPage]=useState<DecorationPage>(pages[0]);
  const [values,setValues]=useState<StyleValues>(()=>Object.fromEntries(Object.entries(selection?.format??{}).filter(([,value])=>!Array.isArray(value))) as StyleValues);
  const [dirty,setDirty]=useState<Record<string,string|number|boolean>>({});
  const [busy,setBusy]=useState(false),[error,setError]=useState('');
  const fields=decorationFields(ctx.info?.swatches);
  const set=(key:string,value:string|number|boolean)=>{setError('');setValues(v=>({...v,[key]:value}));setDirty(v=>({...v,[key]:value}));};
  const submit=async()=>{
    if(!selection||!ctx.editable)return;
    const props=numericStyleValues(fields,dirty);
    if(!props){setError(tr('수치를 확인하세요.'));return;}
    if(!Object.keys(props).length){close();return;}
    setBusy(true);
    try {
      const result=await ctx.edit(title,[{op:'textFormat',story:selection.story,start:selection.start,end:selection.end,props}]);
      if(result)close();else setError(tr('서식을 저장하지 못했습니다. 입력값을 확인하세요.'));
    } catch(e){setError(String(e));} finally{setBusy(false);}
  };
  return createPortal(<div className="style-modal" role="dialog" aria-label={title} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='cancel'&&!busy)close();}}>
    <form className="card dialog style-options type-style-options" onSubmit={e=>{e.preventDefault();if(!busy)void submit();}}>
      <h2>{title}</h2>
      <div className="type-style-tabs" role="tablist">{decorationPages().filter(p=>pages.includes(p.id)).map(p=><button type="button" role="tab" aria-selected={p.id===page} key={p.id} onClick={()=>setPage(p.id)}>{p.label}</button>)}</div>
      <div role="tabpanel"><StyleInputs fields={fields.filter(f=>f.page===page)} values={values} set={set}/></div>
      <p className="muted">{mode==='rules'?tr('프레임을 넘어가는 단락의 경계선은 미완성입니다.'):tr('모서리 옵션, 인쇄 제외, 중복 인쇄는 미완성입니다.')}</p>
      {error&&<p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy||!ctx.editable}>{tr('확인')}</button></div>
    </form>
  </div>,document.body);
}
