import {useEffect,useRef,useState} from 'react';
import {textSelection,type DocumentSummary} from '../engine';
import type {Target} from '../print';
import {changeTargets} from './changeTargets';
import {ai,type Entry} from './runtime';
import type {Marks} from './attachments';

export function useDocumentId(doc:DocumentSummary|null) {
  const nativeId=doc?.folio?.document.id,path=doc?.path;
  const [fallback,setFallback]=useState<{path:string;id:string}>();
  useEffect(()=>{
    let live=true;
    if(path && !nativeId)void crypto.subtle.digest('SHA-256',new TextEncoder().encode(path)).then(hash=>{
      if(live)setFallback({path,id:Array.from(new Uint8Array(hash),b=>b.toString(16).padStart(2,'0')).join('')});
    });
    return ()=>{live=false;};
  },[nativeId,path]);
  return nativeId??(path && fallback?.path===path?fallback.id:undefined);
}
export function useDocumentMarks(doc:DocumentSummary|null,documentId:string|undefined,go:(t:Target)=>void,regionTool:boolean) {
  const [hover,setHover]=useState<Target[]>([]),[pulse,setPulse]=useState<Target[]>([]),[regions,setRegions]=useState<Target[]>([]);
  const navigate=useRef(go);navigate.current=go;
  useEffect(()=>{
    setHover([]);setPulse([]);setRegions([]);
    if(!doc)return;
    let live=true,timer=0,sequence=0,markSequence=0;
    function flash(targets:Target[]){
      clearTimeout(timer);setPulse(targets);timer=window.setTimeout(()=>setPulse([]),2200);
    }
    function mark(e:Event) {
      const m=(e as CustomEvent<Marks & {navigate?:boolean}>).detail;
      if(m.path!==doc?.path)return;
      const id=++markSequence;
      function display(targets:Target[]) {
        if(!live || markSequence!==id)return;
        if(m.navigate && targets.length){navigate.current(targets[0]);flash(targets);}
        else setHover(targets);
      }
      if(m.text){
        const {story,start,end}=m.text;
        void textSelection(story,start,end).then(g=>display(g.rects.length?
          g.rects.map(r=>({page:r.page,bounds:r.rect})):m.targets),()=>display(m.targets));
      } else display(m.targets);
    }
    const drawn=(e:Event)=>{const m=(e as CustomEvent<Marks>).detail;if(m.path===doc.path)setRegions(m.targets);};
    window.addEventListener('folio-ai-mark',mark);window.addEventListener('folio-ai-regions',drawn);
    const off=window.folio.onEvent(raw=>{
      const e=raw as unknown as Entry & {event:string};
      if(e.event!=='aiEvent'||e.kind!=='end')return;
      const ids=e.documents?.find(d=>d.document===documentId)?.summary.changedIds;
      if(!ids?.length)return;
      const turn=++sequence;
      void (async()=>{
        if((await ai('aiSettings')).settings['folio.ai.highlightChanges']===false)return;
        const targets=await changeTargets(doc.pages,ids,async page=>(await ai('pageItems',{page})).items,()=>live && sequence===turn);
        if(live && sequence===turn)flash(targets);
      })().catch(()=>{});
    });
    return ()=>{live=false;clearTimeout(timer);off();window.removeEventListener('folio-ai-mark',mark);window.removeEventListener('folio-ai-regions',drawn);};
  },[doc?.path,documentId,doc?.pages.length]);
  return [...hover,...pulse,...(regionTool?regions:[])];
}
