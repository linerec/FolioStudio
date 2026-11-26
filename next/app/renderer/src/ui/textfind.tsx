import { t as tr } from '../i18n';
import {useRef,useState} from 'react';
import {findText,type FindQuery,type FoundText,type TextProperties,type EditCommand,type EditResult} from '../engine';
import type {TextSelectionState} from './textedit';
import {nextTextMatch,type TextFindSession} from './textfindsession';
import {findGlyph,glyphOfUnicode,glyphReplaceCommand,type GlyphFindState,type GlyphScope,type GlyphSpec} from './glyphfind';
import {NumberField} from './controls/numberfield';

type Complete=GlyphSpec&{glyphId:number};
const complete=(g:GlyphSpec):g is Complete=>!!g.family&&!!g.style&&g.glyphId!==null;
/** One side of the Glyph tab: face, style and the glyph (by id, or by Unicode value through the face's cmap). */
function GlyphFields({label,spec,fonts,set,report}:{label:string;spec:GlyphSpec;fonts:Record<string,string[]>;set:(g:GlyphSpec)=>void;report:(m:string)=>void}) {
  const [mode,setMode]=useState<'id'|'unicode'>('id'),[uni,setUni]=useState('');
  const styles=fonts[spec.family]??[];
  const toUnicode=async(text:string)=>{
    setUni(text);
    const cp=parseInt(text.trim().replace(/^u\+/i,''),16);
    if(!text.trim()||!Number.isFinite(cp)||!spec.family||!spec.style){set({...spec,glyphId:null});return;}
    const id=await glyphOfUnicode(spec.family,spec.style,cp).catch(()=>null);
    set({...spec,glyphId:id});
    if(id===null)report(tr('이 글꼴에 없는 유니코드입니다.'));
  };
  return <fieldset className="glyph-find-side"><legend>{label}</legend>
    <label>{tr('글꼴 이름')}<input aria-label={label+' '+tr('글꼴 이름')} list={'glyph-fonts-'+label} value={spec.family}
      onChange={e=>{const family=e.target.value,known=fonts[family];set({...spec,family,style:known?(known.includes(spec.style)?spec.style:known.includes('Regular')?'Regular':known[0]):spec.style});}}/></label>
    <datalist id={'glyph-fonts-'+label}>{Object.keys(fonts).map(f=><option key={f} value={f}/>)}</datalist>
    <label>{tr('글꼴 스타일')}<input aria-label={label+' '+tr('글꼴 스타일')} list={'glyph-styles-'+label} value={spec.style} onChange={e=>set({...spec,style:e.target.value})}/></label>
    <datalist id={'glyph-styles-'+label}>{styles.map(f=><option key={f} value={f}/>)}</datalist>
    <label>{tr('글리프 입력 방식')}<select aria-label={label+' '+tr('글리프 입력 방식')} value={mode} onChange={e=>setMode(e.target.value as 'id'|'unicode')}>
      <option value="id">{tr('글리프 ID')}</option><option value="unicode">{tr('유니코드')}</option></select></label>
    {mode==='id'
      ?<label>{tr('글리프 ID')}<NumberField value={spec.glyphId} label={label+' '+tr('글리프 ID')} min={0} max={1e9} step={1} decimals={0} width={96} inDialog
          parse={t=>{const n=t.replace(/\D/g,'');return n===''?null:Number(n);}} onCommit={n=>set({...spec,glyphId:n})} onClear={()=>set({...spec,glyphId:null})}/></label>
      :<label>{tr('유니코드')}<input aria-label={label+' '+tr('유니코드')} placeholder="U+0041" value={uni} onChange={e=>void toUnicode(e.target.value)}/>
        {spec.glyphId!==null&&<output aria-label={label+' '+tr('글리프 ID')}>{tr('글리프 ID')} {spec.glyphId}</output>}</label>}
  </fieldset>;
}

/** A find session keeps its original selection scope while results are selected. */
export function TextFindDialog({selection,edit,reveal,close,session,remember,glyph}:{selection:TextSelectionState|null;
  edit:(label:string,commands:EditCommand[])=>Promise<EditResult|null>;reveal:(m:FoundText)=>void;close:()=>void;
  session:TextFindSession|null;remember:(session:TextFindSession)=>void;glyph:GlyphFindState}) {
  const initial=session?.query;
  const [origin,setOrigin]=useState(selection);
  const [text,setText]=useState(initial?.text??''),[replacement,setReplacement]=useState(session?.change??'');
  const [scope,setScope]=useState<FindQuery['scope']>(origin?(initial?.scope??'story'):'document');
  const [caseSensitive,setCase]=useState(initial?.caseSensitive??false),[wholeWord,setWhole]=useState(initial?.wholeWord??false);
  const [findField,setFindField]=useState(Object.keys(initial?.props??{})[0]??''),[findValue,setFindValue]=useState(String(Object.values(initial?.props??{})[0]??''));
  const [changeField,setChangeField]=useState(''),[changeValue,setChangeValue]=useState('');
  const [formatOnly,setFormatOnly]=useState(false),[busy,setBusy]=useState(false),[status,setStatus]=useState('');
  // the search direction (InDesign's Forward / Backward buttons; Ctrl+Alt+Enter turns it): the engine gives the matches in that order
  const [backward,setBackward]=useState(!!initial?.backward);
  const last=useRef<FoundText|null>(null),queryKey=useRef('');
  const input=useRef<HTMLInputElement>(null);
  const props=(field:string,value:string):TextProperties=>field?{[field]:field==='pointSize'?Number(value):value}:{};
  const query=():FindQuery=>({text,scope,story:origin?.story,start:origin?.start,end:origin?.end,
    caseSensitive,wholeWord,props:props(findField,findValue),...(backward?{backward:true}:{})});
  const run=async(how:'find'|'one'|'all')=>{
    setBusy(true);
    try {
      const q=query(),key=JSON.stringify(q);
      if(key!==queryKey.current){last.current=null;queryKey.current=key;}
      const {matches}=await findText(q);
      remember({change:replacement,query:q,last:last.current});
      if(!matches.length){last.current=null;remember({change:replacement,query:q,last:null});setStatus(tr('찾은 글이 없습니다.'));return;}
      if(how==='find'){
        const m=nextTextMatch(matches,last.current,backward)!;last.current=m;remember({change:replacement,query:q,last:m});reveal(m);
        setStatus(`${matches.indexOf(m)+1} / ${matches.length}`);
        window.setTimeout(()=>input.current?.focus(),200);return;
      }
      const m=last.current&&matches.find(m=>m.story===last.current!.story&&m.start===last.current!.start);
      if(how==='one'&&!m){const first=matches[0];last.current=first;remember({change:replacement,query:q,last:first});reveal(first);setStatus(tr('찾은 글을 선택했습니다. 바꾸기를 다시 누르세요.'));return;}
      const selected=how==='one'?{...q,scope:'selection' as const,story:m!.story,start:m!.start,end:m!.end}:q;
      const r=await edit(how==='all'?tr('모두 바꾸기'):tr('글 바꾸기'),[{op:'findReplace',query:selected,
        ...(!formatOnly?{text:replacement}:{}),props:props(changeField,changeValue)}]);
      if(r){
        let remembered=q;
        if(scope==='selection'&&origin&&!formatOnly) {
          const changed=how==='all'?matches:[m!],count=Array.from(replacement).length;
          const end=origin.end+changed.reduce((n,m)=>n+count-(m.end-m.start),0);
          setOrigin({...origin,end});remembered={...q,end};
        }
        remember({change:replacement,query:remembered,last:null});
        setStatus(tr('{0}곳을 바꿨습니다.', { 0: how==='all'?matches.length:1 }));last.current=null;
      }
    }catch(e){setStatus(String(e));}finally{setBusy(false);}
  };
  const lastGlyph=useRef<FoundText|null>(null);
  const glyphWhere=():GlyphScope=>scope==='document'||!origin?{scope:'document'}:{scope,story:origin.story,start:origin.start,end:origin.end};
  /** Glyph tab: Find Next / Change / Change All; the same engine query and command a script would use. */
  const runGlyph=async(how:'find'|'one'|'all')=>{
    const {find,change}=glyph.prefs;
    if(!complete(find)){setStatus(tr('찾을 글리프의 글꼴·스타일·글리프 ID를 정하세요.'));return;}
    setBusy(true);
    try {
      const where=glyphWhere(),matches=await findGlyph(find,where);
      if(!matches.length){lastGlyph.current=null;setStatus(tr('찾은 글이 없습니다.'));return;}
      if(how==='find'){
        const m=nextTextMatch(matches,lastGlyph.current)!;lastGlyph.current=m;reveal(m);
        setStatus(`${matches.indexOf(m)+1} / ${matches.length}`);return;
      }
      if(!complete(change)){setStatus(tr('바꿀 글리프의 글꼴·스타일·글리프 ID를 정하세요.'));return;}
      const m=lastGlyph.current&&matches.find(x=>x.story===lastGlyph.current!.story&&x.start===lastGlyph.current!.start);
      if(how==='one'&&!m){lastGlyph.current=matches[0];reveal(matches[0]);setStatus(tr('찾은 글을 선택했습니다. 바꾸기를 다시 누르세요.'));return;}
      const target:GlyphScope=how==='one'?{scope:'selection',story:m!.story,start:m!.start,end:m!.end}:where;
      const r=await edit(how==='all'?tr('모두 바꾸기'):tr('글 바꾸기'),[glyphReplaceCommand(find,change,target)]);
      if(r){setStatus(tr('{0}곳을 바꿨습니다.', { 0: how==='all'?matches.length:1 }));lastGlyph.current=null;}
    }catch(e){setStatus(String(e));}finally{setBusy(false);}
  };
  const fonts=selection?.fonts??{};
  const field=(label:string,value:string,set:(v:string)=>void)=><select aria-label={label} value={value} onChange={e=>set(e.target.value)}>
    <option value="">{tr('서식 제한 없음')}</option><option value="paragraphStyle">{tr('문단 스타일')}</option><option value="characterStyle">{tr('글자 스타일')}</option>
    <option value="fontStyle">{tr('글꼴 스타일')}</option><option value="pointSize">{tr('글자 크기')}</option></select>;
  const scopeSelect=(reset:()=>void)=><label>{tr('범위')}<select aria-label={tr('찾기 범위')} value={scope} onChange={e=>{setScope(e.target.value as FindQuery['scope']);reset();}}>
    <option value="document">{tr('문서')}</option><option value="story" disabled={!origin}>{tr('기사')}</option><option value="selection" disabled={!origin||origin.start===origin.end}>{tr('선택')}</option></select></label>;
  const direction=<div className="find-direction" role="group" aria-label={tr('찾기 방향')}>
    {([false,true] as const).map(b=><button key={String(b)} type="button" data-direction={b?'backward':'forward'} aria-pressed={backward===b}
      title={tr('Ctrl+Alt+Enter 로 방향을 바꿉니다')} onClick={()=>{setBackward(b);last.current=null;}}>{b?tr('뒤로'):tr('앞으로')}</button>)}
  </div>;
  return <div className="style-modal" role="dialog" aria-label={tr('찾기/바꾸기')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape')close();
      if(e.key==='Enter'&&(e.ctrlKey||e.metaKey)&&e.altKey){e.preventDefault();setBackward(b=>!b);last.current=null;}}}>
    <form className="card dialog style-options" onSubmit={e=>{e.preventDefault();void (glyph.tab==='glyph'?runGlyph('find'):run('find'));}}>
      <h2>{tr('찾기/바꾸기')}</h2>
      <div role="tablist" aria-label={tr('찾기/바꾸기 탭')} className="find-tabs">
        {(['text','glyph'] as const).map(id=><button key={id} type="button" role="tab" aria-selected={glyph.tab===id} onClick={()=>glyph.setTab(id)}>{id==='text'?tr('텍스트'):tr('글리프')}</button>)}
      </div>
      {glyph.tab==='glyph'?<>
        <GlyphFields label={tr('찾을 글리프')} spec={glyph.prefs.find} fonts={fonts} set={g=>glyph.setPrefs(p=>({...p,find:g}))} report={setStatus}/>
        <GlyphFields label={tr('바꿀 글리프')} spec={glyph.prefs.change} fonts={fonts} set={g=>glyph.setPrefs(p=>({...p,change:g}))} report={setStatus}/>
        {scopeSelect(()=>{lastGlyph.current=null;})}
        <p role="status">{status}</p>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('닫기')}</button>
          <button type="submit" disabled={busy}>{tr('다음 찾기')}</button><button type="button" disabled={busy} onClick={()=>void runGlyph('one')}>{tr('바꾸기')}</button>
          <button type="button" disabled={busy} onClick={()=>void runGlyph('all')}>{tr('모두 바꾸기')}</button></div>
      </>:<>
      <label>{tr('찾을 내용')}<input ref={input} autoFocus aria-label={tr('찾을 내용')} value={text} onChange={e=>setText(e.target.value)}/></label>
      <label>{tr('바꿀 내용')}<input aria-label={tr('바꿀 내용')} value={replacement} disabled={formatOnly} onChange={e=>setReplacement(e.target.value)}/></label>
      {scopeSelect(()=>{last.current=null;})}
      {direction}
      <label><input type="checkbox" checked={caseSensitive} onChange={e=>setCase(e.target.checked)}/>{tr('대소문자 구분')}</label>
      <label><input type="checkbox" checked={wholeWord} onChange={e=>setWhole(e.target.checked)}/>{tr('단어 단위')}</label>
      <label>{tr('찾을 서식')}{field(tr('찾을 서식'),findField,setFindField)}<input aria-label={tr('찾을 서식 값')} disabled={!findField} value={findValue} onChange={e=>setFindValue(e.target.value)}/></label>
      <label>{tr('바꿀 서식')}{field(tr('바꿀 서식'),changeField,setChangeField)}<input aria-label={tr('바꿀 서식 값')} disabled={!changeField} value={changeValue} onChange={e=>setChangeValue(e.target.value)}/></label>
      <label><input type="checkbox" checked={formatOnly} onChange={e=>setFormatOnly(e.target.checked)}/>{tr('내용을 유지하고 서식만 바꾸기')}</label>
      <p role="status">{status}</p>
      <div className="dialog-actions"><button type="button" onClick={close}>{tr('닫기')}</button>
        <button type="submit" disabled={busy}>{tr('다음 찾기')}</button><button type="button" disabled={busy} onClick={()=>void run('one')}>{tr('바꾸기')}</button>
        <button type="button" disabled={busy} onClick={()=>void run('all')}>{tr('모두 바꾸기')}</button></div>
      </>}
    </form>
  </div>;
}
