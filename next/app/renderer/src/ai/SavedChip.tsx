import {useEffect,useState} from 'react';
import {tr} from '../i18n';
import {ai} from './runtime';
import {chipLabel,chipTarget,markDocument,selector,type Snapshot} from './attachments';

type Status='changed'|'missing'|'same';
export function SavedChip({saved,path,documentId,pages,revision}:{saved:Snapshot;path?:string;documentId?:string;pages:number;revision:Map<number,number>}) {
  const [current,setCurrent]=useState<Snapshot|null>(null),[status,setStatus]=useState<Status>('same');
  useEffect(()=>{
    let live=true;
    const spec=selector(saved);
    if(!path||!spec||!documentId||saved.document!==documentId){setCurrent(null);setStatus('same');return;}
    async function check(){
      let snapshot:Snapshot|undefined;
      const candidates=[saved.page??0];
      if(saved.kind==='objects')for(let page=0;page<pages;page++)if(page!==saved.page)candidates.push(page);
      for(const page of candidates){
        if(!live)return;
        if(page>=pages)continue;
        try{
          snapshot=await ai('aiContext',{...spec,page,images:!saved.structureHash && !!saved.image}) as Snapshot;
          break;
        }catch(e){
          if(!/selection is missing from this page|finding is missing|unknown story|invalid text offset/.test(String(e)))throw e;
        }
      }
      if(!live)return;
      setCurrent(snapshot??null);
      const changed=snapshot && (saved.structureHash?snapshot.structureHash!==saved.structureHash:
        saved.image?snapshot.image?.sha256!==saved.image.sha256:JSON.stringify(saved.finding)!==JSON.stringify(snapshot.finding));
      setStatus(!snapshot?'missing':changed?'changed':'same');
    }
    void check().catch(()=>{if(live){setCurrent(null);setStatus('same');}});
    return ()=>{live=false;};
  },[saved,path,documentId,pages,revision]);
  const target=current && chipTarget(current);
  const show=(go=false)=>{if(path && target)markDocument(path,[target],go,current?.text);};
  return <span data-kind={saved.kind} data-status={status}>
    <button disabled={!target} onClick={()=>show(true)} onMouseEnter={()=>show()} onMouseLeave={()=>path&&markDocument(path,[])}
      title={saved.objects?.map(o=>o.name||o.id).join('\n')}>
      {saved.image?.png && <img alt="" src={'data:image/png;base64,'+saved.image.png}/>}
      {chipLabel(saved)} {status==='changed' && <small>{tr('그 뒤 바뀜')}</small>}{status==='missing' && <small>{tr('지금은 없음')}</small>}
    </button>
  </span>;
}
