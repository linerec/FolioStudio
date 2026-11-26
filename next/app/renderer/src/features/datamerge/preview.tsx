// Data Merge ▸ Preview and Export to PDF (build-c). The preview is the engine's preview session (edit/preview.rs): the document's
// placeholders show one record's values (`fillMergeRecord`), taken back exactly when the preview ends and never in the undo history.
// Any other edit, Undo or Redo takes the preview back in the engine first; the switch here follows it.
import {useContext,useEffect,useRef,useState} from 'react';
import {tr} from '../../i18n';
import type {PanelCtx} from '../../ui/panels';
import {PreviewEditContext} from '../../ui/controls/usePreviewEdit';
import type {DataMergeState} from './state';

/** The record the preview shows (from 1), kept inside the source's records. */
export const shownRecord=(state:DataMergeState)=>{
  const n=state.catalogue?.source.records.length??0;
  return Math.min(Math.max(state.previewRecord,1),Math.max(n,1));
};

/** Keeps the document's preview in step with the panel's switch and record; `stop` ends it before work that reads the document. */
export function useMergePreview(state:DataMergeState|undefined,ctx:PanelCtx) {
  const session=useContext(PreviewEditContext);
  const catalogue=state?.catalogue;
  const on=!!state?.mergePreview&&!!catalogue&&ctx.editable&&!!session;
  const record=state?shownRecord(state):1;
  const blank=state?.options.removeBlankLines??false;
  useEffect(()=>{
    if(!on||!session||!catalogue||!state)return;
    const fields=catalogue.source.fields.map(f=>f.name);
    session.apply(tr('데이터 병합 미리 보기'),[{op:'fillMergeRecord',fields,record:catalogue.source.records[record-1]??[],remove_blank_lines:blank}],reply=>{if(!reply)state.setMergePreview(false);});
  },[on,record,catalogue,blank]);
  // switched off: the document is given back exactly
  const was=useRef(false);
  useEffect(()=>{if(was.current&&!on&&session?.open)void session.cancel();was.current=on;},[on]);
  // another edit, Undo or Redo: the engine has taken the preview back already
  const history=JSON.stringify(ctx.history??null),seen=useRef(history);
  useEffect(()=>{
    if(seen.current===history)return;
    seen.current=history;
    if(state?.mergePreview){session?.discard();was.current=false;state.setMergePreview(false);}
  },[history]);
  // the panel closes: the preview ends with it
  const live=useRef({on,session,state});
  live.current={on,session,state};
  useEffect(()=>()=>{const l=live.current;if(l.on){void l.session?.cancel();l.state?.setMergePreview(false);}},[]);
  const stop=async()=>{
    if(!state?.mergePreview)return;
    was.current=false;state.setMergePreview(false);
    if(session?.open)await session.cancel();
  };
  return {on,record,stop,available:!!session&&ctx.editable};
}
export type MergePreview=ReturnType<typeof useMergePreview>;

/** The panel's preview switch and the record it shows (first, previous, number, next, last). */
export function PreviewBlock({state,preview}:{state:DataMergeState;preview:MergePreview}) {
  const records=state.catalogue?.source.records.length??0;
  const [typed,setTyped]=useState<string|null>(null);
  const go=(to:number)=>{setTyped(null);state.setPreviewRecord(Math.min(Math.max(Math.round(to)||1,1),Math.max(records,1)));};
  const record=preview.record;
  return <fieldset className="datamerge-preview" data-merge-preview={preview.on?'on':'off'}>
    <legend>{tr('미리 보기')}</legend>
    <label className="check"><input type="checkbox" checked={state.mergePreview} disabled={!preview.available||!records}
      onChange={e=>{if(e.target.checked)state.setMergePreview(true);else void preview.stop();}}/>{tr('문서에서 레코드 미리 보기')}</label>
    <div className="datamerge-record" role="group" aria-label={tr('미리 보기 레코드')}>
      <button type="button" aria-label={tr('첫 레코드')} disabled={record<=1} onClick={()=>go(1)}>«</button>
      <button type="button" aria-label={tr('이전 레코드')} disabled={record<=1} onClick={()=>go(record-1)}>‹</button>
      <input aria-label={tr('레코드 번호')} data-merge-record={record} inputMode="numeric" value={typed??String(record)}
        onChange={e=>setTyped(e.target.value)} onBlur={()=>{if(typed!==null)go(Number(typed));}}
        onKeyDown={e=>{if(e.key==='Enter')go(Number(typed??record));else if(e.key==='Escape')setTyped(null);}}/>
      <span>{tr('/ {0}',{0:records})}</span>
      <button type="button" aria-label={tr('다음 레코드')} disabled={record>=records} onClick={()=>go(record+1)}>›</button>
      <button type="button" aria-label={tr('마지막 레코드')} disabled={record>=records} onClick={()=>go(records)}>»</button>
    </div>
  </fieldset>;
}

/** The project-relative PDF Export to PDF writes by default: next to the document, `<document> merged.pdf`. */
export function defaultPdfPath(ctx:PanelCtx):string {
  const path=ctx.doc?.folio?.document.path??'';
  const slash=path.lastIndexOf('/');
  const folder=slash>=0?path.slice(0,slash+1):'';
  const stem=(slash>=0?path.slice(slash+1):path).replace(/\.folio$/i,'')||'Untitled';
  return `${folder}${stem} merged.pdf`;
}

/** Data Merge ▸ Export to PDF: the merged records straight to a PDF in the project (no merged document is kept). */
export function ExportBlock({state,ctx,preview,pathInput}:{state:DataMergeState;ctx:PanelCtx;preview:MergePreview;pathInput:React.RefObject<HTMLInputElement|null>}) {
  const output=state.pdfPath||defaultPdfPath(ctx);
  const run=async()=>{await preview.stop();await state.exportPdf(output);};
  return <div className="datamerge-export" data-merge-export="">
    <label>{tr('프로젝트 상대 PDF 경로')}<input ref={pathInput} aria-label={tr('프로젝트 상대 PDF 경로')} value={output} disabled={state.exporting}
      onChange={e=>state.setPdfPath(e.target.value)}/></label>
    <button type="button" disabled={state.exporting||!output.trim()} onClick={()=>void run()}>{state.exporting?tr('내보내는 중…'):tr('PDF로 내보내기')}</button>
    {state.exportError&&<p role="alert">{tr('PDF로 내보내지 못했습니다: {0}',{0:state.exportError})}</p>}
    {state.exported&&<p role="status" data-merged-pdf={state.exported.path}>{tr('PDF를 내보냈습니다: {0} ({1}쪽)',{0:state.exported.path,1:state.exported.pages})}</p>}
    {!!state.exported?.missing.length&&<p role="status">{tr('찾지 못한 이미지는 빈 프레임으로 남았습니다: {0}',{0:state.exported.missing.join(', ')})}</p>}
  </div>;
}
