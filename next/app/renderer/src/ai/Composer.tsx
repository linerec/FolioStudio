import {useEffect,useRef,useState} from 'react';
import {tr} from '../i18n';
import {ai,contextChip,useTurn,type Chip,type Fields,type Reference} from './runtime';

import {chipTarget,markDocument,type Ask} from './attachments';
import {useSampleDraft} from './SampleSuggestions';

function appendChips(existing:Chip[], additions:Chip[]) {
  const key=(c:Chip)=>JSON.stringify(Object.entries(c.spec).sort(([a],[b])=>a.localeCompare(b)));
  const keys=new Set(existing.map(key));
  return [...existing,...additions.filter(c=>{const k=key(c);if(keys.has(k))return false;keys.add(k);return true;})];
}
export function Composer({ask,consumed,path,sessionId,send,command,references,error,disabled,draft}:{ask?:Ask;consumed:(a:Ask)=>void;path?:string;draft:{text:string}|null;sessionId:string;send:(text:string,attachments:Fields[])=>Promise<void>;command:(name:string)=>Promise<void>;
  references:()=>Promise<Reference[]>;error:(s:string)=>void;disabled:boolean}) {
  const [value,setValue]=useState(''), [chips,setChips]=useState<Chip[]>([]), [options,setOptions]=useState<Reference[]>([]);
  useSampleDraft(path,value,setValue);
  const [busy,setBusy]=useState(false), [choice,setChoice]=useState(0);
  const active=useTurn(), area=useRef<HTMLTextAreaElement>(null), composing=useRef(false);
  const refSource=useRef(references);refSource.current=references;
  useEffect(()=>{if(draft){setValue(draft.text);setChips([]);area.current?.focus();}},[draft]);
  useEffect(()=>{
    if(!path)return;
    const show=(targets:NonNullable<ReturnType<typeof chipTarget>>[])=>window.dispatchEvent(new CustomEvent('folio-ai-regions',{detail:{path,targets}}));
    show(chips.flatMap(c=>c.snapshot?.kind==='region' && c.snapshot.region?[{page:c.snapshot.page!,bounds:c.snapshot.region}]:[]));
    return ()=>{show([]);markDocument(path,[]);};
  },[chips,path]);
  const previous=useRef(sessionId);
  useEffect(()=>{
    if(previous.current && previous.current!==sessionId){setChips([]);setValue('');}
    previous.current=sessionId;
  },[sessionId]);
  useEffect(()=>{
    if(!ask)return;
    let live=true;
    if(ask.focus)area.current?.focus();
    const owner=previous.current;
    setBusy(true);
    void (async()=>{
      const added:Chip[]=[];
      for(const spec of ask.specs)added.push(await contextChip('',spec));
      if(live && (!owner || owner===previous.current))setChips(cs=>appendChips(ask.replaceRegions?cs.filter(c=>c.spec.kind!=='region'):cs,added));
    })().catch(e=>{if(live)error(String(e));}).finally(()=>{if(live){setBusy(false);consumed(ask);}});
    return ()=>{live=false;};
  },[ask]);
  function point(chip:Chip,navigate=false) {
    const target=chip.snapshot && chipTarget(chip.snapshot);
    if(path && target)markDocument(path,[target],navigate,chip.snapshot?.text);
  }
  const mention=/(?:^|\s)@([^\s@]*)$/.exec(value)?.[1];
  useEffect(()=>{
    let current=true;setChoice(0);setOptions([]);
    if(mention!==undefined) void refSource.current().then(all=>{
      if(current)setOptions(all.filter(x=>x.label.toLowerCase().includes(mention.toLowerCase())).slice(0,30));
    },e=>{if(current)error(String(e));});
    return ()=>{current=false;};
  },[mention]);
  useEffect(()=>{
    function add(e:Event){const detail=(e as CustomEvent<{sessionId:string;chip:Chip}>).detail;
      if(detail?.sessionId===sessionId && detail.chip?.spec) setChips(prev=>appendChips(prev,[detail.chip]));}
    window.addEventListener('folio-ai-attach',add);
    return ()=>window.removeEventListener('folio-ai-attach',add);
  },[sessionId]);
  async function pick(o:Reference) {
    setBusy(true);
    try {
      if(o.spec) {const chip=await contextChip(o.label,o.spec);setChips(prev=>appendChips(prev,[chip]));}
      setValue(s=>s.replace(/@[^\s@]*$/,o.spec?'':o.label+' '));area.current?.focus();
    }catch(e){error(String(e));}finally{setBusy(false);}
  }
  async function submit() {
    if(!value.trim() || busy || active || disabled) return;
    setBusy(true);
    try {if(['/new','/undo-turn','/export'].includes(value.trim()))await command(value.trim());
      else await send(value,chips.map(c=>c.spec));
      setValue('');setChips([]);
    }catch(e){error(String(e));}finally{setBusy(false);}
  }
  return <div className="ai-compose" data-session-id={sessionId}>
    {!!chips.length && <div className="ai-chips">{chips.map((chip,i)=><span key={i} data-kind={String(chip.spec.kind)}>
      <button type="button" title={chip.snapshot?.objects?.map(o=>o.name||o.id).join('\n')}
        onMouseEnter={()=>point(chip)} onMouseLeave={()=>path&&markDocument(path,[])} onClick={()=>point(chip,true)}>{chip.thumbnail?.startsWith('data:image/png;base64,') && <img src={chip.thumbnail} alt=""/>}{chip.label}</button>
      <button aria-label={tr('첨부 제거')} onClick={()=>setChips(cs=>cs.filter((_,k)=>k!==i))}>×</button></span>)}</div>}
    {options.length>0 && <div role="listbox" className="ai-suggestions">{options.map((o,i)=><button key={i} role="option" aria-selected={choice===i} disabled={busy} onClick={()=>void pick(o)}>{o.label}</button>)}</div>}
    <textarea ref={area} rows={3} placeholder={tr('질문하기 · @ 첨부 · / 명령')} value={value} onChange={e=>setValue(e.target.value)}
      onCompositionStart={()=>{composing.current=true;}} onCompositionEnd={()=>{composing.current=false;}}
      onKeyDown={e=>{
        if(e.nativeEvent.isComposing || composing.current || e.keyCode===229)return;
        if(options.length && ['ArrowDown','ArrowUp'].includes(e.key)){e.preventDefault();setChoice(i=>(i+(e.key==='ArrowDown'?1:-1)+options.length)%options.length);return;}
        if(e.key==='Enter' && !e.shiftKey){e.preventDefault();if(options[choice]&&!busy)void pick(options[choice]);else void submit();}
      }}/>
    <div className="ai-actions">{active ? <button onClick={()=>void ai('aiStop',active).catch(e=>error(String(e)))}>{tr('멈춤(Esc)')}</button> :
      <button className="ai-send" disabled={disabled||busy||!value.trim()} onClick={()=>void submit()}>{tr('보내기')}</button>}</div>
  </div>;
}
