import {useEffect,useRef,useState} from 'react';
import {currentLanguage,tr} from '../i18n';
import {ai,useTurn,type Entry,type Fields,type Reference} from './runtime';
import {Transcript} from './Transcript';
import {Composer} from './Composer';
import type {Ask} from './attachments';
import {SampleSuggestions} from './SampleSuggestions';
import './panel.css';

type Session={sessionId:string;name:string};
export type Connection={id:string;name:string;model:string;kind:string};

type Props={revision:Map<number,number>;pages:number;ask?:Ask;consumed:(a:Ask)=>void;path?:string;documentId?:string;label:(id:string)=>string;reveal:(doc:string,id:string)=>Promise<void>;references:()=>Promise<Reference[]>;settings:()=>void};
export function AiPanel({ask,consumed,revision,pages,documentId,path,reveal,references,settings,label}:Props) {
  const [list,setList]=useState<Session[]>([]),[sid,setSid]=useState(''),[records,setRecords]=useState<Entry[]>([]);
  const [connections,setConnections]=useState<Connection[]>([]),[defaultId,setDefault]=useState('');
  const [library,setLibrary]=useState(false),[query,setQuery]=useState(''),[name,setName]=useState('');
  const [error,setError]=useState(''),[config,setConfig]=useState(false),[deleting,setDeleting]=useState('');
  const [retry,setRetry]=useState<(()=>Promise<void>)|null>(null);
  const selected=useRef(''),request=useRef(0),live=useRef<Entry[]>([]),active=useTurn();
  const sent = useRef(new Map<string,{text:string;attachments:Fields[]}>());
  const [retryDraft,setRetryDraft]=useState<{text:string}|null>(null);
  const [previews,setPreviews] = useState<Entry[]>([]);
  const [newRequired,setNewRequired] = useState(false);
  const header=records.find(x=>x.kind==='session'), connection=connections.find(x=>x.id===header?.connectionId);
  async function reloadSessions(){const res=await ai('aiSessions');setList(res.sessions);return res.sessions as Session[];}
  async function open(id:string) {
    const token=++request.current, changed=selected.current!==id;
    selected.current=id;setSid(id);if(changed){setRetryDraft(null);sent.current.clear();setRecords([]);setPreviews([]);setNewRequired(false);setError('');setRetry(null);}
    live.current=[];
    if(!id)return;
    const data=await ai('aiSession',{sessionId:id,includeImages:true});
    if(token!==request.current)return;
    const saved=data.records as Entry[],ids=new Set(saved.filter(r=>r.seq!=null).map(r=>r.turnId+'/'+r.seq));
    setRecords([...saved,...live.current.filter(r=>!ids.has(r.turnId+'/'+r.seq))]);
    setName([...saved].reverse().find(r=>['rename','session'].includes(r.kind))?.name??'');
  }
  const fail=(e:unknown)=>{const s=String(e);setError(s);if(s.includes('aiNotConfigured'))setConfig(true);if(s.includes('aiNewSessionRequired'))setNewRequired(true);};
  async function loadConnections() {
    const res=await ai('aiConnections');setConnections(res.connections);setDefault(res.defaultConnection??res.connections[0]?.id??'');setConfig(!res.connections.length);
  }
  useEffect(()=>{
    let mounted=true;
    const off=window.folio.onEvent(raw=>{
      const r=raw as unknown as Entry & {event:string};
      if(r.event!=='aiEvent' || r.sessionId!==selected.current)return;
      if(r.kind==='preview') {
        setPreviews(prev=>[...prev.filter(p=>p.turnId===r.turnId && p.afterSeq===r.afterSeq && p.block!==r.block),r].sort((a,b)=>(a.block??0)-(b.block??0)));
        return;
      }
      setPreviews([]);
      if(r.kind==='user' && r.attachments?.some(a=>a.image && !a.image.png))void open(r.sessionId).catch(fail);
      live.current.push(r);setRecords(prev=>prev.some(x=>x.turnId===r.turnId && x.seq===r.seq)?prev:[...prev,r]);
    });
    void reloadSessions().then(rows=>{if(mounted && !selected.current && rows.length)return open(active?.sessionId??rows[0].sessionId);}).catch(fail);
    const sync=()=>void loadConnections().catch(e=>{setConfig(true);fail(e);});
    sync();window.addEventListener('folio-ai-connections',sync);
    return ()=>{mounted=false;request.current++;off();window.removeEventListener('folio-ai-connections',sync);};
  },[]);
  async function create(connectionId=defaultId) {
    if(!connectionId) {settings();return;}
    const res=await ai('aiNewSession',{connectionId,name:tr('새 대화'),language:currentLanguage()});
    await reloadSessions();await open(res.sessionId);setLibrary(false);
  }
  async function exportSession(id:string) {
    const res=await ai('aiExportSession',{sessionId:id});
    await window.folio.saveText((list.find(x=>x.sessionId===id)?.name??'Folio').replace(/[<>:"/\\|?*\u0000-\u001f]/g,'_')+'.md',res.markdown);
  }
  async function send(text:string,attachments:Fields[]) {
    const id=selected.current, attempt=async()=>{const res=await ai('aiSend',{sessionId:id,text,attachments});sent.current.set(res.turnId,{text,attachments});setError('');setRetry(null);};
    try{await attempt();}catch(e){setRetry(()=>attempt);throw e;}
  }
  function repeat(turnId:string, user?:Entry) {
    const original=sent.current.get(turnId);
    if(original) return void send(original.text,original.attachments).catch(fail);
    if(!user) return;
    if(user.attachments?.length){
      setRetryDraft({text:user.text??''});setError(tr('원래 첨부를 다시 선택한 뒤 보내세요.'));return;
    }
    void send(user.text??'',[]).catch(fail);
  }
  async function command(cmd:string){
    if(cmd==='/new')return create();
    if(cmd==='/export')return exportSession(sid);
    const cards=Array.from(document.querySelectorAll<HTMLButtonElement>('[data-ai-revert]:not(:disabled)'));
    if(!cards.length)throw Error(tr('되돌릴 차례가 없습니다.'));
    cards[cards.length-1].click();
  }
  const safe=(fn:()=>Promise<unknown>)=>()=>void fn().catch(fail);
  const usage=records.filter(e=>e.kind==='end' && e.usage).map(e=>e.usage!);
  const lastCost=records.filter(r=>r.kind==='end' && (r.usage || r.estimatedUsd!=null)).at(-1);
  const tokens=usage.length?usage.reduce((n,u)=>n+(u.inputTokens??0)+(u.outputTokens??0),0):'—';
  const totalCost=lastCost?.sessionEstimatedUsd;
  const cacheRead=usage.reduce((n,u)=>n+(u.cacheReadTokens??0),0),cacheWrite=usage.reduce((n,u)=>n+(u.cacheWriteTokens??0),0);
  return <section className="ai-panel" aria-label={tr('AI 대화')}>
    <header><div className="ai-heading"><input aria-label={tr('세션 이름')} disabled={!sid} value={name} onChange={e=>setName(e.target.value)} onKeyDown={e=>{if(e.key==='Enter' && !e.nativeEvent.isComposing)e.currentTarget.blur();}}
      onBlur={()=>{if(name.trim() && sid && list.find(s=>s.sessionId===sid)?.name!==name.trim())void ai('aiRenameSession',{sessionId:sid,name:name.trim()}).then(reloadSessions).catch(fail);}}/>
      <button title={tr('새 세션')} aria-label={tr('새 세션')} disabled={!!active||!connections.length} onClick={safe(()=>create())}>＋</button>
      <button aria-label={tr('세션 목록')} title={tr('세션 목록')} aria-expanded={library} onClick={()=>setLibrary(!library)}>☰</button>
      <button aria-label={tr('AI 설정')} title={tr('AI 설정')} onClick={settings}>⚙</button></div>
      <select aria-label={tr('연결 선택')} disabled={!!active||!connections.length} value={header?.connectionId??defaultId} onChange={e=>void create(e.target.value).catch(fail)}>
        {!connections.length && <option>{tr('설정된 연결 없음')}</option>}{connections.map(c=><option key={c.id} value={c.id}>{c.name}</option>)}</select>
      {header && <small>{header.model} · {connection?.name??header.provider} · {header.mode==='readOnly'?tr('읽기 전용'):header.mode==='ask'?tr('편집 전 확인'):tr('직접 편집')}</small>}
    </header>
    {library && <div className="ai-library"><input aria-label={tr('세션 검색')} placeholder={tr('세션 검색')} value={query} onChange={e=>setQuery(e.target.value)}/>
      {list.filter(s=>s.name.toLowerCase().includes(query.toLowerCase())).map(s=><div key={s.sessionId}><button disabled={!!active && active.sessionId!==s.sessionId} onClick={safe(async()=>{await open(s.sessionId);setLibrary(false);})}>{s.name}</button>
      <button title={tr('Markdown 내보내기')} onClick={safe(()=>exportSession(s.sessionId))}>⇩</button>
      <button disabled={!!active} title={tr('세션 삭제')} onClick={()=>setDeleting(s.sessionId)}>×</button></div>)}
      {deleting && <div>{tr('이 대화를 삭제할까요?')}<button onClick={safe(async()=>{await ai('aiDeleteSession',{sessionId:deleting});const rows=await reloadSessions();if(sid===deleting)await open(rows[0]?.sessionId??'');setDeleting('');})}>{tr('삭제')}</button><button onClick={()=>setDeleting('')}>{tr('취소')}</button></div>}
    </div>}
    {error && <aside role="alert">{error}{retry && <button onClick={safe(retry)}>{tr('다시 해 보기')}</button>}</aside>}
    {newRequired && <button disabled={!!active} onClick={safe(()=>create(header?.connectionId??defaultId))}>{tr('새 세션')}</button>}
    {(config || !sid) && <div className="ai-empty">{config ? <button onClick={settings}>{tr('AI 설정하기')}</button> : <button disabled={!connections.length} onClick={safe(()=>create())}>{tr('새 대화')}</button>}</div>}
    {!path && !records.some(r=>r.kind==='user') && <SampleSuggestions choose={text=>setRetryDraft({text})}/>}
    <Transcript revision={revision} pages={pages} path={path} records={records} previews={previews} documentName={path?.split(/[\\/]/).pop()} documentId={documentId} actions={{refresh:()=>open(sid),error:fail,reveal,label,retry:repeat,canRetry:id=>sent.current.has(id)}}/>
    <footer>{tr('토큰 {0} · 비용 {1}',{0:tokens,1:totalCost==null?'—':'$'+totalCost.toFixed(4)})}
      {(cacheRead>0||cacheWrite>0) && <div>{tr('캐시 읽기 {0} · 쓰기 {1}',{0:cacheRead,1:cacheWrite})}</div>}
      {lastCost?.budgetWarning && <div role="status">{tr('예산 경고')}</div>}
      {records.some(r=>r.kind==='assistant' && r.questionsOnly) && <div>{tr('질문 전용 연결')}</div>}
    </footer>
    <Composer ask={ask} consumed={consumed} path={path} draft={retryDraft} sessionId={sid} disabled={!sid||config} references={references} command={command} send={send} error={fail}/>
  </section>;
}
