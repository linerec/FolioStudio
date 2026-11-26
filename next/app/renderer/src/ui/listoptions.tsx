import { useNumberingListChoices } from './numberinglists';
import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { textFormat, textStory, type EditCommand, type EditResult, type TextProperties } from '../engine';
import type { TextFormatting } from './textformat';
import type { TableSelection } from './textedit';
import type { Command } from './commands';
import { fieldKey } from './keys';
import { styleFields } from './textstyles/fields';
import { TextNumberField, formatNumeric, parseNumeric } from './controls/lengthfield';

type Context = {path?:string;editable:boolean;table:TableSelection|null;stories:string[];edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>};
type Target = {story:string;start:number;end:number};
export function useListOptions(control:TextFormatting,ctx:Context) {
  const [dialog,setDialog]=useState<{targets:Target[];values:Record<string,unknown>}|null>(null);
  useEffect(()=>setDialog(null),[ctx.path]);
  const available=ctx.editable&&!!(control.selection||ctx.table);
  const targets=async():Promise<Target[]>=>{
    if(control.selection){const {story,start,end}=control.selection;return [{story,start,end}];}
    const table=ctx.table;
    const ids=table?table.info.cells.filter(c=>c.cell[0]>=table.from[0]&&c.cell[0]<=table.to[0]&&c.cell[1]>=table.from[1]&&c.cell[1]<=table.to[1]).map(c=>c.id):ctx.stories;
    return Promise.all([...new Set(ids)].map(async story=>({story,start:0,end:(await textStory(story)).length})));
  };
  const open=async()=>{
    if(control.selection){const s=control.selection;setDialog({targets:[{story:s.story,start:s.start,end:s.end}],values:s.format});return;}
    const table=ctx.table;if(!table)return;
    const cells=table.info.cells.filter(c=>c.cell[0]>=table.from[0]&&c.cell[0]<=table.to[0]&&c.cell[1]>=table.from[1]&&c.cell[1]<=table.to[1]);
    const targets=await Promise.all(cells.map(async c=>({story:c.id,start:0,end:[...(await textStory(c.id)).text].length})));
    const rows=await Promise.all(targets.map(t=>textFormat(t.story,t.start,t.end)));
    const values={...rows[0]?.format};
    for(const row of rows.slice(1))for(const key of Object.keys(values))if(JSON.stringify(values[key])!==JSON.stringify(row.format[key]))values[key]=null;
    setDialog({targets,values});
  };
  const handlers:Record<string,Partial<Command>>={
    'id.panel.paragraph.bulletsAndNumbering':{run:available?()=>void open():undefined},
    'id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText':{run:ctx.editable&&(control.selection||ctx.table||ctx.stories.length)?()=>void targets().then(ts=>ctx.edit(tr('글머리 기호 및 번호 매기기를 텍스트로 변환'),ts.map(t=>({op:'convertListsToText',...t,labels:{}})))):undefined},
  };
  return {handlers,dialog:dialog&&<ListOptionsDialog key={JSON.stringify(dialog.targets)} initial={dialog.values} control={control}
    apply={async props=>!!await ctx.edit(tr('글머리 기호 및 번호 매기기'),dialog.targets.map(t=>({op:'textFormat',...t,props})))} close={()=>setDialog(null)}/>};
}
function ListOptionsDialog({initial,control,apply,close}:{initial:Record<string,unknown>;control:TextFormatting;apply:(props:TextProperties)=>Promise<boolean>;close:()=>void}) {
  const [values,setValues]=useState(initial),[dirty,setDirty]=useState<Record<string,string|boolean>>({}),[error,setError]=useState(''),[busy,setBusy]=useState(false);
  const lists=useNumberingListChoices();
  const fields=styleFields((control.info?.characterStyles??[]).map(s=>({value:s.id,label:s.name})),lists).filter(f=>f.page==='lists'||['leftIndent','firstLineIndent'].includes(f.key));
  const change=(key:string,value:string|boolean)=>{setValues(v=>({...v,[key]:value}));setDirty(v=>({...v,[key]:value}));};
  const submit=async()=>{
    const props:TextProperties={...dirty};
    for(const f of fields.filter(f=>!f.text&&!f.choices))if(f.key in props){const v=String(props[f.key]).trim();if(!v||!Number.isFinite(Number(v))){setError(tr('수치를 확인하세요.'));return;}props[f.key]=Number(v);}
    setBusy(true);setError('');try{if(await apply(props))close();else setError(tr('입력값을 확인하세요.'));}catch(e){setError(String(e));}finally{setBusy(false);}
  };
  return createPortal(<div className="style-modal" role="dialog" aria-label={tr('글머리 기호 및 번호 매기기')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='cancel'&&!busy)close();}}>
    <form className="card dialog style-options type-style-options" onSubmit={e=>{e.preventDefault();if(!busy)void submit();}}>
      <h2>{tr('글머리 기호 및 번호 매기기')}</h2>
      <p>{tr('^#은 번호, ^t는 탭입니다. 탭 위치는 탭 패널에서 설정합니다.')}</p>
      <div role="tabpanel">{fields.map(f=><label key={f.key}>{f.label}{f.choices?<select aria-label={f.label} value={String(values[f.key]??'')} onChange={e=>change(f.key,f.boolean?e.target.value==='true':e.target.value)}>
        <option value="" disabled>{tr('(혼합)')}</option>{f.choices.map(c=><option key={c.value} value={c.value}>{c.label}</option>)}
      </select>:f.text
        ?<input aria-label={f.label} value={String(values[f.key]??'')} placeholder={values[f.key]==null?tr('(혼합)'):undefined} onChange={e=>change(f.key,e.target.value)}/>
        :<TextNumberField label={f.label} width={160} placeholder={values[f.key]==null?tr('(혼합)'):undefined} text={String(values[f.key]??'')} onText={t=>change(f.key,t)} format={formatNumeric} parse={parseNumeric}/>}</label>)}</div>
      {error&&<p role="alert">{error}</p>}<div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy}>{tr('확인')}</button></div>
    </form>
  </div>,document.body);
}
