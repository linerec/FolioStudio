import {useCallback,useRef} from 'react';
import {tr} from '../../i18n';
import {EditError,type EditResult,editDocument} from '../../engine';
import {readDocumentComments} from './query';
import type {PdfCommentsState} from './state';

/** The path of `file` under the project folder (`reviews/a.pdf`), or null when it is outside it. The comparison ignores case where the OS does. */
export function insideProject(root:string,file:string):string|null {
  const slashes=(p:string)=>p.replace(/\\/g,'/').replace(/\/+$/,'');
  const [r,f]=[slashes(root),slashes(file)];
  const fold=/^[A-Za-z]:|^\/\//.test(r)||/^\/(Users|Volumes)\//.test(r);
  const head=r+'/';
  return (fold?f.toLowerCase().startsWith(head.toLowerCase()):f.startsWith(head))?f.slice(head.length):null;
}

type Params={state:PdfCommentsState;editable:boolean;projectRoot?:string;apply:(r:EditResult)=>void;flash:(text:string)=>void;showPanel:()=>void};

/**
 * File ▸ Import PDF Comments…. InDesign opens a file window and imports what the PDF holds; the checks it makes first
 * (no comments, a corrupt PDF, the same file name imported before) are alerts, and so is a PDF that is not inside the project (Folio:
 * the project holds what it refers to, so the PDF is copied in once the person agrees). The result notice is "mapped/all imported".
 */
export function usePdfCommentsImport({state,editable,projectRoot,apply,flash,showPanel}:Params) {
  const busy=useRef(false);
  const doImport=useCallback(async(path:string,again:boolean)=>{
    if(busy.current)return;
    busy.current=true;
    try {
      const r=await editDocument(tr('PDF 주석 가져오기'),[{op:'importPdfComments',path,again,source:undefined,comments:undefined}]);
      apply(r);
      showPanel();
      const made=(await readDocumentComments()).comments.filter(c=>r.created?.includes(c.id));
      state.setToast(tr('{0}/{1}개 주석을 가져왔습니다',{0:made.filter(c=>!c.orphan).length,1:made.length}));
      state.setAlert(null);
    } catch(e) {
      const code=(e as EditError).code;
      if(code==='noComments'||code==='corrupt'||code==='notInDesign'||code==='otherDocument'||code==='duplicate')state.setAlert({kind:code,path});
      else flash(`${tr('PDF 주석 가져오기')}: ${String((e as Error).message??e)}`);
    } finally {busy.current=false;}
  },[apply,flash,showPanel,state]);
  /** Picks a PDF (`chosen`: given, for the next try after "Select another"). */
  const pick=useCallback(async()=>{
    if(!editable||!projectRoot)return;
    state.setAlert(null);
    const file=await window.folio.pdfCommentsDialog(projectRoot);
    if(!file)return;
    const inside=insideProject(projectRoot,file);
    if(inside)await doImport(inside,false);
    else state.setAlert({kind:'copy',path:file});
  },[editable,projectRoot,doImport,state]);
  const copyAndImport=useCallback(async(file:string)=>{
    if(!projectRoot)return;
    const copied=await window.folio.copyPdfIntoProject(file,projectRoot);
    if(!copied.ok){state.setAlert(null);flash(tr('PDF를 프로젝트로 복사하지 못했습니다.'));return;}
    await doImport(copied.path,false);
  },[projectRoot,doImport,state,flash]);
  const answer=useCallback((accept:boolean)=>{
    const a=state.alert;
    if(!a)return;
    if(!accept){state.setAlert(null);return;}
    if(a.kind==='copy')void copyAndImport(a.path);
    else if(a.kind==='duplicate')void doImport(a.path,true);
    else void pick();
  },[state,copyAndImport,doImport,pick]);
  return {start:()=>void pick(),answer};
}
