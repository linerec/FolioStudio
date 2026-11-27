import {useEffect,useRef,useState} from 'react';
import {tr} from '../i18n';
import {ai} from './runtime';
import { AgentFields } from './AgentFields';
import './settings.css';

type Scalar = string | boolean | number;
type Values = Record<string, unknown>;
type Connection = {id:string;name:string;kind:string;baseUrl:string;model:string;effort:string;sendImages:boolean;budgetWarning?:number;hasKey?:boolean;command?:string;args?:string[];env?:Record<string,string>};
type Template = Pick<Connection,'kind'|'baseUrl'|'model'> & {available:boolean};
type Props = {values:Values;change:(k:string,v:Scalar)=>void;flush:()=>Promise<void>;close:()=>void;error:string};
const changed = () => window.dispatchEvent(new Event('folio-ai-connections'));
const names:Record<string,string>={anthropic:'Claude',openai:'OpenAI',gemini:'Gemini',openrouter:'OpenRouter',ollama:'Ollama',lmStudio:'LM Studio'};
function kindName(kind:string){return kind==='acp'?tr('외부 에이전트'):names[kind]??(kind==='local'?tr('로컬 연결'):kind==='custom'?tr('직접 입력'):kind);}
export function AiSettings({values,change,flush,close,error}:Props) {
  const [rows,setRows]=useState<Connection[]>([]),[templates,setTemplates]=useState<Template[]>([]),[defaultId,setDefault]=useState('');
  const [models,setModels]=useState<string[]>([]),[notice,setNotice]=useState(''),[busy,setBusy]=useState(false);
  const [confirm,setConfirm]=useState(''),[keyOpen,setKeyOpen]=useState(false);
  const key=useRef<HTMLInputElement>(null), mounted=useRef(true), current=useRef(values);current.current=values;
  const id=String(values.connectionId??''), saved=rows.find(c=>c.id===id);
  function select(row:Connection) {
    setKeyOpen(false);if(key.current)key.current.value='';setModels([]);setNotice('');setConfirm('');
    for(const [k,v] of Object.entries({connectionId:row.id,name:row.name,kind:row.kind,baseUrl:row.baseUrl,model:row.model,effort:row.effort,sendImages:row.sendImages,budgetWarning:row.budgetWarning??0,command:row.command??'',args:JSON.stringify(row.args??[]),env:JSON.stringify(row.env??{})}))change(k,v);
  }
  function blank(template:Template|undefined) {
    if(template)select({id:crypto.randomUUID(),name:'',kind:template.kind,baseUrl:template.baseUrl,model:template.model,effort:'medium',sendImages:true});
  }
  async function load(initial=false){
    const res=await ai('aiConnections');
    if(!mounted.current)return;
    const connections:Connection[]=res.connections, options:Template[]=res.templates??[];
    setRows(connections);setTemplates(options);setDefault(res.defaultConnection??'');
    if(initial && !current.current.connectionId){
      const row=connections.find(c=>c.id===res.defaultConnection)??connections[0];
      if(row)select(row);else blank(options.find(t=>t.available));
    }
  }
  useEffect(()=>{
    mounted.current=true;
    void load(true).catch(e=>setNotice(String(e)));
    void ai('aiSettings').then(r=>{if(mounted.current)for(const k of ['mode','highlightChanges','localOnly']){
      const v=r.settings['folio.ai.'+k];if(v!=null)change(k,v);
    }}).catch(e=>setNotice(String(e)));
    return ()=>{mounted.current=false;};
  },[]);
  const run=(fn:()=>Promise<void>)=>async()=>{if(busy)return;setBusy(true);setNotice('');try{await flush();await fn();}catch(e){if(mounted.current)setNotice(String(e));}finally{if(mounted.current)setBusy(false);}};
  async function apply(action:string){
    await ai('applyDialog',{scope:'app',action});changed();
  }
  async function credential(clear:boolean){
    const secret=clear?'':key.current?.value;
    if(!saved || (!clear && !secret))return;
    if(key.current)key.current.value='';
    let storage:string|undefined;
    try{storage=(await ai('aiSetCredential',{connectionId:saved.id,key:secret})).storage;}catch{throw Error(tr('키를 저장하지 못했습니다.'));}
    setKeyOpen(false);setConfirm('');await load();setNotice(clear?tr('키를 지웠습니다.'):storage==='session'?tr('이번 실행에만 키를 사용합니다.'):tr('키를 저장했습니다.'));
  }
  const textField=(k:string,label:string,extra:Record<string,string>={})=><label>{label}<input data-field={k} {...extra} value={String(values[k]??'')} onChange={e=>change(k,e.target.value)}/></label>;
  const check=(k:string,label:string)=><label className="ai-check"><input type="checkbox" checked={!!values[k]} onChange={e=>change(k,e.target.checked)}/>{label}</label>;
  const save=run(async()=>{if(!rows.length && confirm!=='privacy'){setConfirm('privacy');return;}await apply('ok');});
  return <div className="ai-settings-shade" onKeyDown={e=>{if(e.key==='Escape'){e.stopPropagation();close();}}}>
    <section className="ai-settings" role="dialog" aria-modal="true" aria-label={tr('AI 설정')}>
      <header><h2>{tr('AI 설정')}</h2><button onClick={close} aria-label={tr('닫기')}>×</button></header>
      <div className="ai-settings-body"><nav aria-label={tr('연결 목록')}>{rows.map(c=><button key={c.id} aria-pressed={id===c.id} onClick={()=>select(c)}>{c.name}{c.id===defaultId?' · '+tr('기본'):''}</button>)}
        <button disabled={!templates.some(t=>t.available)||busy} onClick={()=>blank(templates.find(t=>t.available))}>{tr('연결 추가')}</button>
        <button disabled={!saved||busy} onClick={()=>saved && select({...saved,id:crypto.randomUUID(),name:saved.name+' '+tr('사본')})}>{tr('복제')}</button>
        <button disabled={!saved||busy} onClick={()=>setConfirm('delete')}>{tr('삭제')}</button></nav>
      <fieldset disabled={!id||busy}>
        {textField('name',tr('이름'))}<label>{tr('연결 종류')}<select value={String(values.kind??'')} onChange={e=>{const t=templates.find(x=>x.kind===e.target.value);if(t){change('kind',t.kind);change('baseUrl',t.baseUrl);change('model',t.model);change('command','');change('args','[]');change('env','{}');setModels([]);}}}>{templates.map(t=><option disabled={!t.available} key={t.kind} value={t.kind}>{kindName(t.kind)}</option>)}</select></label>
        {values.kind==='acp'?<AgentFields values={values} change={change}/>:textField('baseUrl',tr('주소'))}
        <div className="ai-settings-row"><span>{tr('API 키')} · {saved?.hasKey?tr('저장됨'):tr('등록 전')}</span><button disabled={!saved} onClick={()=>setKeyOpen(v=>!v)}>{tr('바꾸기')}</button><button disabled={!saved?.hasKey} onClick={()=>setConfirm('key')}>{tr('지우기')}</button></div>
        {keyOpen && <div className="ai-settings-row"><input type="password" ref={key} autoComplete="new-password" aria-label={tr('API 키')}/><button onClick={run(()=>credential(false))}>{tr('키 저장')}</button></div>}
        {values.kind!=='acp' && <>{textField('model',tr('모델'),{list:'ai-tested-models'})}<datalist id="ai-tested-models">{models.map(m=><option key={m}>{m}</option>)}</datalist>
        <label>{tr('생각 깊이')}<select value={String(values.effort)} onChange={e=>change('effort',e.target.value)}>{['low','medium','high','xhigh','max'].map(x=><option key={x}>{x}</option>)}</select></label>
        <label>{tr('예산 경고')} (USD)<input type="number" min="0" step="any" value={Number(values.budgetWarning??0)} onChange={e=>change('budgetWarning',Number(e.target.value))}/></label></>}
        {check('sendImages',tr('그림 보내기'))}
        <div className="ai-settings-row"><button disabled={!saved} onClick={()=>setConfirm('test')}>{tr('연결 시험')}</button><button disabled={!saved||id===defaultId} onClick={run(async()=>{await ai('aiSetDefaultConnection',{connectionId:id});await load();changed();})}>{tr('기본으로 정하기')}</button></div>
      </fieldset></div>
      <fieldset disabled={busy}><legend>{tr('공통 설정')}</legend><label>{tr('권한 기본값')}<select value={String(values.mode)} onChange={e=>change('mode',e.target.value)}>{[['readOnly',tr('읽기 전용')],['ask',tr('편집 전 확인')],['direct',tr('직접 편집')]].map(([v,l])=><option key={v} value={v}>{l}</option>)}</select></label>
      {check('highlightChanges',tr('변경 강조'))}{check('localOnly',tr('로컬 연결만'))}<button onClick={run(()=>apply('common'))}>{tr('공통 설정 저장')}</button></fieldset>
      {confirm && <div className="ai-settings-row" role="alert">{confirm==='privacy'?tr('선택한 내용과 문서 정보가 {0}로 보내집니다.',{0:kindName(String(values.kind))}):confirm==='test'?(saved?.kind==='acp'?tr('실행 파일을 찾을까요?'):tr('저장한 연결 주소로 모델 목록을 요청할까요?')):tr('지울까요?')}
        <button onClick={run(async()=>{if(confirm==='privacy'){await apply('ok');return;}if(confirm==='key')await credential(true);
          if(confirm==='delete'){await ai('aiDeleteConnection',{connectionId:id});const other=rows.find(c=>c.id!==id);if(other)select(other);else blank(templates.find(t=>t.available));await load();changed();}
          if(confirm==='test'){const r=await ai('aiTestConnection',{connectionId:id});if(r.available===false)throw Error(tr('연결할 수 없습니다.'));const m=r.models??(await ai('aiModels',{connectionId:id})).models;
            setModels((m??[]).map((x:string|{id:string})=>typeof x==='string'?x:x.id));setNotice(tr('연결을 확인했습니다.'));}setConfirm('');
        })}>{tr('확인')}</button><button onClick={()=>setConfirm('')}>{tr('취소')}</button></div>}
      {(notice||error) && <p role="status">{notice||error}</p>}
      <footer><button onClick={close}>{tr('닫기')}</button><button disabled={busy||!String(values.name??'').trim()||!id} onClick={save}>{tr('연결 저장')}</button></footer>
    </section></div>;
}
