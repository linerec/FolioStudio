import { useEffect, useRef, useState } from 'react';
import { tr } from '../i18n';
import { ai, useTurn, type Entry } from './runtime';
import {SavedChip} from './SavedChip';
import { Markdown } from './Markdown';

type Actions = { refresh:()=>Promise<void>; error:(e:string)=>void; reveal:(doc:string,id:string)=>Promise<void>; label:(id:string)=>string; canRetry:(turn:string)=>boolean; retry:(turn:string,user?:Entry)=>void };
const pretty = (value:unknown) => JSON.stringify(value,null,2);
export function Changes({event,restored,actions,documentId,documentName}:{event:Entry;restored:Set<string>;actions:Actions;documentId?:string;documentName?:string}) {
  const [plan,setPlan] = useState<{cmd:string;rev:number;steps:string[]}|null>(null);
  const [kept,setKept] = useState(false);
  const [busy,setBusy] = useState(false);
  const active=useTurn();
  async function restore(cmd:string, confirmation?:number) {
    const fields={sessionId:event.sessionId,turnId:event.turnId,edge:'start'};
    setBusy(true);
    try {
      const preview=await ai(cmd,{...fields,dryRun:true});
      if (confirmation===undefined && preview.needsConfirmation || confirmation!==undefined && preview.rev!==confirmation) {
        setPlan({cmd,rev:preview.rev,steps:preview.steps}); return;
      }
      const result=await ai(cmd,{...fields,...(confirmation===undefined ? {} : {confirmedRev:confirmation})});
      if (result.needsConfirmation) setPlan({cmd,rev:result.rev,steps:result.steps});
      else {setPlan(null); await actions.refresh();}
    } catch(e) {actions.error(String(e));} finally {setBusy(false);}
  }
  const documents=(event.documents??[]).filter(d=>d.summary.commands.length>0);
  return <>{documents.map(d=>{
    const current=documentId===d.document;
    const done=restored.has(d.document);
    return <section className="ai-card ai-change" key={d.document}>
      <strong>{tr('문서 변경')}</strong><small title={d.document}>{current && documentName ? documentName : d.document}</small>
      <div title={d.summary.commands.join(', ')}>{tr('명령 {0}개 · 변경 항목 {1}개',{0:d.summary.commands.length,1:d.summary.changedIds.length})}</div>{d.summary.changedIds.map(id=><button key={id} data-object-id={id} title={id} disabled={!current||done} onClick={()=>void actions.reveal(d.document,id).catch(e=>actions.error(String(e)))}>{current?actions.label(id):id}</button>)}
      {done ? <p>{tr('되돌렸습니다.')}</p> : <div className="ai-actions"><button disabled={kept||busy} onClick={()=>setKept(true)}>{tr('유지')}</button>
        <button data-ai-revert disabled={!current||!!active||busy} onClick={()=>void restore('aiRevertTurn')}>{tr('되돌리기')}</button>
        <button disabled={!current||!!active||busy} onClick={()=>void restore('aiRestoreCheckpoint')}>{tr('여기로 되돌리기')}</button></div>}
      {current && plan && <div role="alert"><strong>{tr('함께 되돌릴 편집')}</strong><ol>{plan.steps.map((s,i)=><li key={i}>{s}</li>)}</ol>
        <button disabled={busy} onClick={()=>void restore(plan.cmd,plan.rev)}>{tr('확인')}</button><button onClick={()=>setPlan(null)}>{tr('취소')}</button></div>}
    </section>;
  })}</>;
}

function Permission({e,live,error}:{e:Entry;live:boolean;error:(s:string)=>void}) {
  const [sent,setSent]=useState(false);
  async function respond(decision:string) {
    setSent(true);
    try {await ai('aiRespond',{sessionId:e.sessionId,turnId:e.turnId,permissionId:e.permissionId,decision});}
    catch(err){setSent(false);error(String(err));}
  }
  return <section className="ai-card ai-permission"><strong>{tr('허락 요청')}</strong> {e.tool}{e.destination && <p>{tr('전송 주소 {0}',{0:e.destination})}</p>}
    {e.outside && <p>{tr('이 일은 Folio 밖에서 일어납니다')}</p>}
    {e.sessionTool && <p>{tr('이번 세션 동안을 고르면 이 요청은 한 번 허락하고 이 대화에서 {0} 편집을 허락합니다.',{0:e.sessionTool})}</p>}
    {e.args!=null && <details><summary>{tr('인수')}</summary><pre>{pretty(e.args)}</pre></details>}
    {live && !sent && <div className="ai-actions">{e.outside && <button autoFocus onClick={()=>void respond('deny')}>{tr('거절')}</button>}<button onClick={()=>void respond('allow')}>{tr('허락')}</button>
    <button disabled={!e.allowedDecisions?.includes('session')} onClick={()=>void respond('session')}>{tr('이번 세션 동안')}</button>
    {!e.outside && <button onClick={()=>void respond('deny')}>{tr('거절')}</button>}</div>}</section>;
}
export function Transcript({revision,pages,path,records,actions,documentId,documentName,previews}:{revision:Map<number,number>;pages:number;path?:string;records:Entry[];actions:Actions;documentId?:string;documentName?:string;previews:Entry[]}) {
  const active=useTurn(), end=useRef<HTMLDivElement>(null), follow=useRef(true);
  useEffect(()=>{if(follow.current && end.current) end.current.scrollTop=end.current.scrollHeight;},[records,previews]);
  const results=new Map(records.filter(r=>r.kind==='tool_result').map(r=>[r.turnId+'/'+r.toolUseId,r]));
  const users=new Map(records.filter(r=>r.kind==='user').map(r=>[r.turnId,r]));
  const combined:Entry[]=[];
  for(const record of records) {
    const last=combined[combined.length-1];
    if (['text','thinking'].includes(record.kind) && last?.kind===record.kind && last.turnId===record.turnId)
      combined[combined.length-1]={...last,text:(last.text??'')+(record.text??'')};
    else combined.push(record);
  }
  const latest=records.at(-1);
  return <div className="ai-transcript" ref={end} onScroll={e=>{const el=e.currentTarget; follow.current=el.scrollHeight-el.scrollTop-el.clientHeight<45;}}>
    {combined.map((r,i)=>{
      const key=r.turnId+'-'+(r.seq??i);
      switch(r.kind) {
      case 'user': return <div className="ai-user" key={key}>{r.text}{!!r.attachments?.length && <div className="ai-chips">{r.attachments.map((saved,i)=><SavedChip key={i} saved={saved} path={path} revision={revision} pages={pages} documentId={documentId}/>)}</div>}</div>;
      case 'text': return <div className="ai-answer" key={key}><Markdown text={r.text??''}/></div>;
      case 'thinking': return <details key={key}><summary>{tr('생각 과정')}</summary><Markdown text={r.text??''}/></details>;
      case 'plan': return <section className="ai-card" key={key}><strong>{tr('작업 계획')}</strong><ul>{r.entries?.map((entry,j)=><li key={j}>{entry.status==='completed'?'✓ ':''}{entry.content}</li>)}</ul></section>;
      case 'agentTool': return <div className="ai-card" key={key}>{r.title??r.toolCallId} {r.status}</div>;
      case 'tool_use': {
        const result=results.get(r.turnId+'/'+r.id);
        return <details className="ai-card ai-tool" key={key}><summary>{r.name} {result ? (result.isError?'×':'✓'):'…'}</summary>
          <b>{tr('인수')}</b><pre>{pretty(r.args)}</pre>{result && <><b>{tr('결과')}</b><pre>{pretty(result.content)}</pre></>}</details>;
      }
      case 'permission': return <Permission key={key} e={r} live={active?.turnId===r.turnId && latest===r} error={actions.error}/>;
      case 'end': return <div key={key}>{r.error && <aside role="alert">{r.error}<button disabled={!!active||!users.has(r.turnId)&&!actions.canRetry(r.turnId)} onClick={()=>actions.retry(r.turnId,users.get(r.turnId))}>{tr('다시 해 보기')}</button></aside>}
        {r.status==='stopped' && <small>{tr('작업을 멈췄습니다.')}</small>}
        <Changes event={r} documentId={documentId} documentName={documentName} actions={actions} restored={new Set(records.filter(x=>x.kind==='restore' && x.turnId===r.turnId).map(x=>String(x.result?.document)))}/></div>;
      default:return null;
      }
    })}
    {previews.map(p=>p.blockType==='thinking'?<details key={p.block}><summary>{tr('생각 과정')}</summary><Markdown text={p.text??''}/></details>:<div className="ai-answer" key={p.block}><Markdown text={p.text??''}/></div>)}
  </div>;
}
