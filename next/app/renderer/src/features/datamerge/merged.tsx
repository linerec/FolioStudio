import {useState} from 'react';
import {tr} from '../../i18n';
import type {PanelCtx} from '../../ui/panels';

/** What a merged document remembers (engine info `dataMergeMerged`, build-c2). */
export type MergedInfo={template?:string|null;source?:string|null;sha256?:string|null;updatedAt?:string|null;log:number};
type LogEntry={at:string;result:string;record?:number;field?:string;from?:string;to?:string;updated?:number;left?:number};

/** A log line in the person's words. */
function logText(e:LogEntry):string{
  const when=e.at?new Date(e.at).toLocaleString():'';
  switch(e.result){
    case 'summary':return tr('{0}: 바꾼 필드 {1}개, 그대로 둔 필드 {2}개',{0:when,1:e.updated??0,2:e.left??0});
    case 'updated':return tr('레코드 {0}의 {1}: "{2}" → "{3}"',{0:e.record??'',1:e.field??'',2:e.from??'',3:e.to??''});
    case 'edited':return tr('레코드 {0}의 {1}: 글이 병합 때와 달라 그대로 두었습니다("{2}"(으)로 바꾸지 않음)',{0:e.record??'',1:e.field??'',2:e.to??''});
    case 'noField':return tr('레코드 {0}의 {1}: 데이터 원본에 이 필드가 없어 그대로 두었습니다',{0:e.record??'',1:e.field??''});
    case 'noRecord':return tr('레코드 {0}의 {1}: 데이터 원본에 이 레코드가 없어 그대로 두었습니다',{0:e.record??'',1:e.field??''});
    case 'noStory':return tr('레코드 {0}의 {1}: 글상자가 지워져 그대로 두었습니다',{0:e.record??'',1:e.field??''});
    case 'noFrame':return tr('레코드 {0}의 {1}: QR 코드 틀이 지워져 그대로 두었습니다',{0:e.record??'',1:e.field??''});
    default:return e.result;
  }
}

/** Data Merge ▸ Update Content in Data Fields and Show Log of Update Results for a merged document: the data source is read again
 *  by the engine (`dataMergeUpdateSource`), the edit command `updateMergedFields` replaces the field texts (one undo step), the log is
 *  the engine's (`dataMergeLog`). */
export function useMergedDocument(ctx:PanelCtx){
  const info=(ctx.info as {dataMergeMerged?:MergedInfo|null}|undefined)?.dataMergeMerged??null;
  const [busy,setBusy]=useState(false),[message,setMessage]=useState(''),[error,setError]=useState(''),[log,setLog]=useState<LogEntry[]|null>(null);
  const readLog=async()=>{
    const {header}=await window.folio.request({cmd:'dataMergeLog'});
    if(!header.ok)throw Error(String(header.error));
    return (header.log as LogEntry[])??[];
  };
  const update=async()=>{
    setBusy(true);setError('');setMessage('');
    try{
      const {header}=await window.folio.request({cmd:'dataMergeUpdateSource'});
      if(!header.ok)throw Error(String(header.error));
      await ctx.edit(tr('데이터 필드 내용 업데이트'),[{op:'updateMergedFields',fields:header.fields as string[],records:header.records as string[][],sha256:String(header.sha256),at:new Date().toISOString()}]);
      const entries=await readLog();
      const last=[...entries].reverse().find(e=>e.result==='summary');
      setMessage(last?logText(last):'');
      if(log)setLog(entries);
    }catch(e){setError(String(e instanceof Error?e.message:e));}finally{setBusy(false);}
  };
  const showLog=async()=>{setError('');try{setLog(await readLog());}catch(e){setError(String(e instanceof Error?e.message:e));}};
  const canUpdate=!!info&&ctx.editable&&!busy;
  const block=info&&<fieldset className="datamerge-merged" data-merged-document>
    <legend>{tr('병합 문서')}</legend>
    <p>{tr('서식 문서: {0}',{0:info.template??'—'})}</p>
    <p>{tr('데이터 원본: {0}',{0:info.source??'—'})}</p>
    {info.updatedAt&&<p>{tr('마지막 업데이트: {0}',{0:new Date(info.updatedAt).toLocaleString()})}</p>}
    <div className="datamerge-link-actions">
      <button type="button" data-merge-update disabled={!canUpdate} onClick={()=>void update()}>{tr('데이터 필드 내용 업데이트')}</button>
      <button type="button" data-merge-log onClick={()=>log?setLog(null):void showLog()}>{log?tr('업데이트 결과 기록 닫기'):tr('업데이트 결과 기록 보기')}</button>
    </div>
    <p className="small">{tr('데이터 원본을 다시 읽어 병합이 넣은 글이 그대로인 필드만 새 값으로 바꿉니다. 손으로 고친 글과 다른 부분은 그대로 둡니다.')}</p>
    {message&&<p role="status">{message}</p>}
    {error&&<p role="alert">{tr('업데이트하지 못했습니다: {0}',{0:error})}</p>}
    {log&&<ul className="datamerge-log" aria-label={tr('업데이트 결과 기록')}>
      {log.length===0&&<li>{tr('아직 업데이트한 적이 없습니다.')}</li>}
      {[...log].reverse().slice(0,200).map((e,k)=><li key={k} data-log-result={e.result}>{logText(e)}</li>)}
    </ul>}
  </fieldset>;
  return {info,block,update:canUpdate?()=>void update():undefined,showLog:info?()=>void showLog():undefined};
}
