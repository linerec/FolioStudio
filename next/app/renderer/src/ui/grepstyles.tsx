import { useEffect, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import { getStyleInfo, type EditCommand, type EditResult, type GrepStyle, type StyleInfo } from '../engine';
import type { TextFormatting } from './textformat';
import type { Command } from './commands';
import { fieldKey } from './keys';
import { GrepFields } from './textstyles/grep';

type Context={path?:string;edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>};
export function useGrepStyles(control:TextFormatting,context:Context) {
  const [info,setInfo]=useState<StyleInfo|null>(null);
  useEffect(()=>setInfo(null),[context.path]);
  const style=control.selection?.format.paragraphStyle;
  const handlers:Record<string,Partial<Command>>={
    'id.panel.paragraph.grepStyles':{enabled:typeof style==='string'&&!!style,
      run:async()=>{if(typeof style==='string'&&style){const result=await getStyleInfo('paragraph',style);setInfo(result.style);}}},
  };
  return {handlers,dialog:info&&<GrepDialog key={info.id} info={info} styles={control.info?.characterStyles??[]} context={context} close={()=>setInfo(null)}/>};
}
function GrepDialog({info,styles,context,close}:{info:StyleInfo;styles:{id:string;name:string}[];context:Context;close:()=>void}) {
  const [rules,setRules]=useState<GrepStyle[]>(info.grepStyles??[]),[busy,setBusy]=useState(false),[error,setError]=useState('');
  const save=async()=>{setBusy(true);setError('');try{const result=await context.edit(tr('GREP 스타일'),[{op:'setGrepStyles',style:info.id,rules}]);if(result)close();else setError(tr('스타일을 저장하지 못했습니다. 입력값을 확인하세요.'));}catch(e){setError(String(e));}finally{setBusy(false);}};
  return createPortal(<div className="style-modal" role="dialog" aria-label={tr('GREP 스타일')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(fieldKey(e)==='cancel'&&!busy)close();}}>
    <form className="card dialog style-options type-style-options" onSubmit={e=>{e.preventDefault();if(!busy&&!info.protected)void save();}}>
      <h2>{tr('GREP 스타일')}</h2><p>{tr('단락 스타일 전체에 적용: {name}',{name:info.name})}</p>
      <GrepFields rules={rules} styles={styles} set={setRules} disabled={busy||info.protected}/>
      {info.protected&&<p>{tr('[스타일 없음]과 [없음]은 고칠 수 없습니다.')}</p>}
      {error&&<p role="alert">{error}</p>}
      <div className="dialog-actions"><button type="button" disabled={busy} onClick={close}>{tr('취소')}</button><button type="submit" disabled={busy||info.protected}>{tr('확인')}</button></div>
    </form>
  </div>,document.body);
}
