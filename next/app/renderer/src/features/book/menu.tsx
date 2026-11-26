import {useState} from 'react';
import {engineMessage, type EngineMessage} from '../../i18n/engine-message';
import {tr} from '../../i18n';
import type {Command} from '../../ui/commands';
import {NumberField} from '../../ui/controls/numberfield';
import {formatInteger,parseInteger} from '../../ui/controls/lengthfield';
import {Flyout} from '../../ui/flyout';
import type {Commands} from '../../ui/commands';
import {PANEL_MENUS} from '../../ui/menudef';

type Entry={id:string;path:string;sha256:string};
type Options={repagination:string;insert_blank_page:boolean;automatic:boolean};
type Book={name:string;documents:Entry[];options:Options;style_source?:string|null;sync:string[];merge_layers?:boolean};
type Status={id:string;ok:boolean;pages?:number;error?:string};
type Row={id:string;range:string;start:number;chapterNumber:number;blanks:number;ownStart:boolean;writable:boolean;written:boolean};
type Plan={applied:boolean;documents?:Row[];error?:string};
type Sync={documents:{id:string;source:boolean;changed:boolean;writable:boolean;written:boolean}[];notSynchronized:string[]};
/** Preflight Book…: the preflight panel's report of each document (engine request `book` action `preflight`). */
type Check={id:string;path:string;ok:boolean;error?:string;report?:{summary:{errors:number;warnings:number;infos:number};findings:(EngineMessage & {severity:string;pageName?:string;message:string})[]}};
const KINDS:[string,()=>string][]=[['paragraph',()=>tr('단락 스타일')],['character',()=>tr('문자 스타일')],['object',()=>tr('개체 스타일')],['cell',()=>tr('셀 스타일')],['table',()=>tr('표 스타일')],['swatch',()=>tr('견본')],['numberingList',()=>tr('번호 매기기 목록')],['conditionalText',()=>tr('조건')],['textVariable',()=>tr('텍스트 변수')],['trapStyle',()=>tr('트랩 스타일')],['tableOfContentStyle',()=>tr('목차 스타일')],['crossReferenceFormat',()=>tr('상호 참조 형식')],['masterPage',()=>tr('부모 페이지')]];
const ORDER:[string,()=>string][]=[['nextPage',()=>tr('이전 문서에서 이어서')],['nextOdd',()=>tr('다음 홀수 페이지에서 시작')],['nextEven',()=>tr('다음 짝수 페이지에서 시작')]];
/** A message of the engine, in the user's words (`document_open: path` when someone has the document open). */
const explain=(e:string)=>e.startsWith('document_open')?tr('다른 곳에서 열려 있는 문서가 있어 아무것도 쓰지 않았습니다: {0}',{0:e.replace(/^document_open:\s*/,'')}):e;
/** File ▸ New ▸ Book…: the engine's Save dialog (next/shared/dialogs/newBook.json: a folder of the project, the file name, Replace?) makes an empty
 *  book file in the project; `made` opens the book it answers (the window below, empty), `openExisting` the window to open one. */
export function useBook(root?:string,getCommands:()=>Commands=()=>({})){
  // `panel`: the open book (or the row that opens an existing one)
  const [phase,setPhase]=useState<'closed'|'panel'>('closed'),[path,setPath]=useState(''),[document,setDocument]=useState(''),[selected,setSelected]=useState(''),[replacement,setReplacement]=useState('');
  const [book,setBook]=useState<Book|null>(null),[status,setStatus]=useState<Status[]>([]),[error,setError]=useState(''),[busy,setBusy]=useState(false);
  const [plan,setPlan]=useState<Plan|null>(null),[sync,setSync]=useState<Sync|null>(null),[notice,setNotice]=useState(''),[checks,setChecks]=useState<Check[]|null>(null);
  const [output,setOutput]=useState('book.pdf'),[layered,setLayered]=useState(false),[forPrint,setForPrint]=useState(false),[printBlank,setPrintBlank]=useState(true),[exported,setExported]=useState<{pages:number;path:string}|null>(null);
  const request=async(action:string,extra:Record<string,unknown>={})=>{
    setBusy(true);setError('');
    try{const {header}=await window.folio.request({cmd:'book',root,path,action,...extra});if(!header.ok)throw Error(explain(String(header.error)));setBook(header.book as Book);setStatus(header.status as Status[]);if(header.pagination)setPlan(header.pagination as Plan);if(header.sync)setSync(header.sync as Sync);if(header.preflight)setChecks((header.preflight as {documents:Check[]}).documents);if(header.bookPackage){const p=header.bookPackage as {dir:string;documents:{ok:boolean;path:string;error?:string}[]};setNotice(tr('책 패키지 {0}개를 만들었습니다: {1}',{0:p.documents.filter(d=>d.ok).length,1:p.dir})+p.documents.filter(d=>!d.ok).map(d=>` — ${d.path}: ${d.error}`).join(''));}if(header.crossReferences){const rows=(header.crossReferences as {documents:{written:boolean;error?:string|null}[]}).documents;setNotice(tr('상호 참조를 업데이트한 문서: {0}',{0:rows.filter(r=>r.written).length})+(rows.some(r=>r.error)?` — ${tr('업데이트하지 못한 문서가 있습니다')}`:''));}if(header.created)setSelected(String(header.created));if(header.exported)setExported(header.exported as {pages:number;path:string});}
    catch(e){setError(String(e));}finally{setBusy(false);}
  };
  const [chapter,setChapter]=useState(1),[chapterSource,setChapterSource]=useState('ContinueFromPreviousDocument');
  // Save Book As… (build-c): the book file copied to another project-relative .foliobook, which the window then shows
  const [saveAsPath,setSaveAsPath]=useState('');
  const saveBookAs=async()=>{
    setBusy(true);setError('');setNotice('');
    try{const {header}=await window.folio.request({cmd:'book',root,path,action:'saveAs',to:saveAsPath.trim()});if(!header.ok)throw Error(explain(String(header.error)));
      setPath(String(header.path));setBook(header.book as Book);setStatus(header.status as Status[]);setSaveAsPath('');setNotice(tr('책을 {0}(으)로 저장했습니다.',{0:String(header.path)}));}
    catch(e){setError(String(e));}finally{setBusy(false);}
  };
  const index=book?.documents.findIndex(e=>e.id===selected)??-1;
  const options=book?.options;
  const rows=new Map((plan?.documents??[]).map(r=>[r.id,r]));
  const describe=(e:Entry)=>{const r=rows.get(e.id);if(!r)return e.path;const notes=[tr('장 번호 {0}',{0:r.chapterNumber}),r.ownStart?tr('자체 시작 번호'):'',r.blanks?tr('빈 페이지 {0}쪽 추가',{0:r.blanks}):'',r.writable?'':tr('.idml은 쓰지 않음')].filter(Boolean).join(', ');return `${e.path} — ${tr('페이지 {0}',{0:r.range})}${notes?` (${notes})`:''}`;};
  // the Save dialog's answer (the engine made the file: `book` action `new`): the book opens, empty
  const made=(header:Record<string,unknown>)=>{setPath(String(header.path));setBook(header.book as Book);setStatus(header.status as Status[]);setSelected('');setPlan(null);setSync(null);setExported(null);setError('');setPhase('panel');};
  // the Book panel menu (InDesign's, menutree PANEL_MENUS.book): each entry does what this window's controls do; the ones that ask for a file or a
  // choice take you to the control (the window takes project-relative paths, not a file picker)
  const focus=(label:string)=>{window.setTimeout(()=>window.document.querySelector<HTMLElement>(`[role=dialog] [aria-label="${label}"]`)?.focus(),0);};
  const open=phase==='panel'&&!!book,entry=index>=0?book?.documents[index]:undefined,numbers=open&&!!book?.documents.length?()=>{setNotice('');void request('repaginate');}:undefined;
  const fresh=(f:()=>void)=>()=>{setNotice('');f();};
  // Package Book For Print…: the folder is picked by the person (a package goes where the printer's files go, often outside the project); each document gets a package folder of its own in it
  const packageBook=async()=>{const dir=await window.folio.chooseFolder(tr('책 패키지를 만들 폴더'));if(dir)await request('package',{dir});};
  const handlers:Record<string,Partial<Command>>={'id.file.new.book':{enabled:!!root,why:root?undefined:tr('책 파일은 프로젝트 폴더 안에 만듭니다. 프로젝트를 먼저 여세요.'),run:()=>void window.folio.request({cmd:'openDialog',dialog:'newBook',values:{project:root,book:book?path:'',fileName:tr('책 1')+'.foliobook'}})},
    'id.panel.book.addDocument':{run:open?fresh(()=>focus(tr('문서 상대 경로'))):undefined},
    'id.panel.book.replaceDocument':{run:open&&entry?fresh(()=>focus(tr('다시 연결할 상대 경로'))):undefined},
    'id.panel.book.removeDocument':{run:open&&entry?fresh(()=>void request('remove',{entry:selected})):undefined},
    'id.panel.book.saveBook':{run:open?()=>{void request('read').then(()=>setNotice(tr('책은 바뀔 때마다 바로 저장됩니다. 지금 저장된 상태입니다.')));}:undefined},
    'id.panel.book.saveBookAs':{run:open?fresh(()=>{if(!saveAsPath)setSaveAsPath(path.replace(/\.foliobook$/i,'')+' copy.foliobook');focus(tr('다른 이름으로 저장할 책 경로'));}):undefined},
    'id.panel.book.closeBook':{run:open?()=>setPhase('closed'):undefined},
    'id.panel.book.exportBookToPdf':{run:open&&book?.documents.length&&output?fresh(()=>void request('exportPdf',{output,acrobatLayers:layered})):undefined},
    // Merge Identically Named Layers on Export (build-c2): the book's option (kept in the book file), on by default
    'id.panel.book.mergeIdenticallyNamedLayersOnExport':{run:open?fresh(()=>void request('mergeLayers',{merge:!(book?.merge_layers??true)})):undefined,checked:open&&(book?.merge_layers??true)},
    'id.panel.book.printBook':{run:open&&book?.documents.length&&output?fresh(()=>void request('exportPdf',{output,printingJob:{selection:{kind:'all'},spreads:false,printBlankPages:printBlank}})):undefined},
    'id.panel.book.documentInformation':{run:open&&entry?()=>setNotice(describe(entry)):undefined},
    'id.panel.book.revealInExplorer':{run:open&&entry&&root?()=>void window.folio.reveal(/^win/i.test(navigator.platform)?`${root}/${entry.path}`.split('/').join(String.fromCharCode(92)):`${root}/${entry.path}`):undefined},
    'id.panel.book.updateAllCrossReferences':{run:open&&book?.documents.length?fresh(()=>void request('updateCrossReferences')):undefined},
    'id.panel.book.packageBookForPrint':{run:open&&book?.documents.length?fresh(()=>void packageBook()):undefined},
    'id.panel.book.preflightBook':{run:open&&book?.documents.length?fresh(()=>void request('preflight')):undefined},
    'id.panel.book.synchronizeBook':{run:open&&book&&book.documents.length>1&&book.sync.length?fresh(()=>void request('synchronize')):undefined},
    'id.panel.book.synchronizeOptions':{run:open?fresh(()=>focus(tr('스타일 기준 문서'))):undefined},
    'id.panel.book.updateNumbering.updatePageAndSectionNumbers':{run:numbers},
    'id.panel.book.updateNumbering.updateChapterAndParagraphNumbers':{run:numbers},
    'id.panel.book.updateNumbering.updateAllNumbers':{run:numbers},
    'id.panel.book.bookPageNumberingOptions':{run:open?fresh(()=>focus(tr('문서 시작 위치'))):undefined},
    'id.panel.book.documentNumberingOptions':{run:open?fresh(()=>focus(tr('장 번호 방식'))):undefined},
  };
  const openExisting=()=>{setBook(null);setPlan(null);setSync(null);setError('');setPhase('panel');};
  // a book of the project chosen in the explorer (project-relative): the window opens with it read
  const openFile=(rel:string)=>{setPath(rel);setBook(null);setPlan(null);setSync(null);setSelected('');setExported(null);setPhase('panel');void request('read',{path:rel});};
  return {handlers,made,openExisting,openFile,dialog:phase==='panel'&&<div className="style-modal" role="dialog" aria-label={tr('책')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>e.stopPropagation()}><div className="card dialog style-options" style={{width:640,padding:20,maxHeight:'90vh',overflow:'auto'}}>
    <div style={{display:'flex',alignItems:'center',justifyContent:'space-between'}}><h2>{tr('책')}</h2><Flyout items={PANEL_MENUS.book??[]} getCommands={getCommands} label={tr('책 패널 메뉴')}/></div><p>{tr('프로젝트 문서 목록을 책으로 저장합니다. 책은 Folio 형식(.foliobook)이며 문서의 프로젝트 상대 경로와 SHA-256을 기억합니다. InDesign의 .indb 파일은 읽거나 쓰지 않습니다.')}</p>
    <p><label>{tr('프로젝트 상대 경로')} <input aria-label={tr('프로젝트 상대 경로')} value={path} onChange={e=>{setPath(e.target.value);setBook(null);}}/></label><button disabled={busy||!path} onClick={()=>void request('read')}>{tr('책 열기')}</button></p>
    {book&&options&&<><h3>{book.name}</h3><select size={6} aria-label={tr('책 문서')} style={{width:'100%'}} value={selected} onChange={e=>setSelected(e.target.value)}>{book.documents.map(e=><option key={e.id} value={e.id}>{describe(e)}{status.find(s=>s.id===e.id)?.ok?'':' ⚠'}</option>)}</select>
      <p><label>{tr('다른 이름으로 저장할 책 경로')} <input aria-label={tr('다른 이름으로 저장할 책 경로')} value={saveAsPath} placeholder="chapters/book copy.foliobook" onChange={e=>setSaveAsPath(e.target.value)}/></label><button disabled={busy||!saveAsPath.trim()} onClick={()=>void saveBookAs()}>{tr('책을 다른 이름으로 저장')}</button></p>
      <p><label>{tr('문서 상대 경로')} <input aria-label={tr('문서 상대 경로')} value={document} onChange={e=>setDocument(e.target.value)}/></label><button disabled={busy||!document} onClick={()=>void request('add',{document})}>{tr('책에 문서 추가')}</button></p>
      <button disabled={busy||index<=0} onClick={()=>void request('move',{entry:selected,index:index-1})}>{tr('위로')}</button><button disabled={busy||index<0||index>=book.documents.length-1} onClick={()=>void request('move',{entry:selected,index:index+1})}>{tr('아래로')}</button><button disabled={busy||index<0} onClick={()=>void request('remove',{entry:selected})}>{tr('삭제')}</button>
      <p><input aria-label={tr('다시 연결할 상대 경로')} value={replacement} onChange={e=>setReplacement(e.target.value)}/><button disabled={busy||index<0||!replacement} onClick={()=>void request('relink',{entry:selected,replacement})}>{tr('다시 연결')}</button></p>
      <h3>{tr('페이지 번호')}</h3>
      <p><label>{tr('문서 시작 위치')} <select aria-label={tr('문서 시작 위치')} value={options.repagination} disabled={busy} onChange={e=>void request('options',{repagination:e.target.value})}>{ORDER.map(([v,l])=><option key={v} value={v}>{l()}</option>)}</select></label>
        <label><input type="checkbox" aria-label={tr('빈 페이지 삽입')} checked={options.insert_blank_page} disabled={busy} onChange={e=>void request('options',{insertBlankPage:e.target.checked})}/> {tr('빈 페이지 삽입')}</label>
        <label><input type="checkbox" aria-label={tr('자동 페이지 번호 갱신')} checked={options.automatic} disabled={busy} onChange={e=>void request('options',{automatic:e.target.checked})}/> {tr('자동 페이지 번호 갱신')}</label>
        <button disabled={busy||!book.documents.length} onClick={()=>void request('repaginate')}>{tr('페이지 번호 갱신')}</button></p>
      <p><label>{tr('장 번호 방식')} <select aria-label={tr('장 번호 방식')} value={chapterSource} onChange={e=>setChapterSource(e.target.value)}><option value='ContinueFromPreviousDocument'>{tr('자동')}</option><option value='UserDefined'>{tr('직접 지정')}</option></select></label>
        <label>{tr('장 번호')} <NumberField value={chapter} onCommit={setChapter} min={1} max={1e9} step={1} decimals={0} parse={parseInteger} format={formatInteger} label={tr('장 번호')} width={80} inDialog/></label>
        <button disabled={busy||index<0||!Number.isInteger(chapter)||chapter<1} onClick={()=>void request('chapterNumber',{entry:selected,number:chapter,source:chapterSource})}>{tr('선택 문서 장 번호 설정')}</button></p>
      <p>{tr('페이지 번호를 갱신할 때 장 번호도 갱신합니다. 자동은 앞 문서의 장 번호에 1을 더하고, 직접 지정한 번호는 유지합니다.')}</p>
      {plan?.error&&<p role="alert">{explain(plan.error)}</p>}
      <p>{tr('문서를 더하거나 순서를 바꾸거나 빼면 번호를 다시 매깁니다. 번호를 매기면 책에 든 .folio 문서가 다시 저장되고, 다른 곳에서 열려 있는 문서가 있으면 아무것도 쓰지 않습니다.')}</p>
      <h3>{tr('스타일 동기화')}</h3>
      <p><label>{tr('스타일 기준 문서')} <select aria-label={tr('스타일 기준 문서')} value={book.style_source??''} disabled={busy||!book.documents.length} onChange={e=>void request('styleSource',{entry:e.target.value})}><option value="">{tr('첫 문서')}</option>{book.documents.map(e=><option key={e.id} value={e.id}>{e.path}</option>)}</select></label></p>
      <p>{KINDS.map(([k,l])=><label key={k}><input type="checkbox" aria-label={l()} checked={book.sync.includes(k)} disabled={busy} onChange={e=>void request('syncKinds',{kinds:e.target.checked?[...book.sync,k]:book.sync.filter(x=>x!==k)})}/> {l()} </label>)}
        <button disabled={busy||book.documents.length<2||!book.sync.length} onClick={()=>void request('synchronize')}>{tr('책 동기화')}</button></p>
      <p>{tr('기준 문서의 같은 이름 정의로 바꾸고 새 정의를 더합니다. 대상 문서에만 있는 정의는 유지합니다. 견본([용지] 제외)·조건과 조건 집합·텍스트 변수·목차 스타일·상호 참조 형식을 모두 동기화합니다. 부모 페이지는 기본으로 꺼져 있고, 켜면 같은 이름의 부모 페이지를 기준 문서의 것으로 통째로 바꿉니다. 직접 지정한 조건 표시 색은 가져오지 않고, 두 문서에서 조건이 같은 자리에 있고 UI 색 조건의 수가 같을 때만 문서의 색이 남습니다.')}</p>
      {sync&&<p role="status">{tr('동기화한 문서: {0}',{0:sync.documents.filter(d=>d.written).length})}</p>}
      <h3>{tr('책 사전 검사')}</h3>
      <p>{tr('책의 문서마다 사전 검사 패널과 같은 검사를 합니다. 문서는 이 창과 따로 열어 검사하므로 화면의 문서는 그대로입니다.')}</p>
      <p><button disabled={busy||!book.documents.length} onClick={()=>void request('preflight')}>{tr('책 사전 검사')}</button></p>
      {checks&&<ul aria-label={tr('책 사전 검사 결과')}>{checks.map(c=><li key={c.id}>{c.path}: {c.ok&&c.report?tr('오류 {0} · 경고 {1}',{0:c.report.summary.errors,1:c.report.summary.warnings}):c.error}{c.ok&&c.report?.findings.filter(f=>f.severity!=='info').slice(0,6).map((f,i)=><div key={i} className="small">{f.pageName?`${f.pageName}: `:''}{engineMessage(f)}</div>)}</li>)}</ul>}
      <h3>{tr('인쇄용 책 패키지')}</h3>
      <p>{tr('책의 문서마다 파일 ▸ 패키지와 같은 패키지(문서, 링크, 글꼴, 보고서)를 고른 폴더 안에 문서별 폴더로 만듭니다. 문서마다 따로 완결된 패키지입니다.')}</p>
      <p><button disabled={busy||!book.documents.length} onClick={()=>void packageBook()}>{tr('인쇄용 책 패키지…')}</button></p>
      <h3>{tr('책 전체 PDF 내보내기')}</h3>
      <p>{tr('저장된 문서를 책 순서대로 한 PDF로 내보냅니다. INDB/INDL 네이티브 호환은 하지 않습니다.')}</p>
      <p><label>{tr('책 PDF 상대 경로')} <input aria-label={tr('책 PDF 상대 경로')} value={output} onChange={e=>setOutput(e.target.value)}/></label><button disabled={busy||!book.documents.length||!output} onClick={()=>void request('exportPdf',{output,acrobatLayers:layered&&!forPrint,...(forPrint?{printingJob:{selection:{kind:'all'},spreads:false,printBlankPages:printBlank}}:{})})}>{tr('책 PDF 내보내기')}</button></p>
      <p><label><input type="checkbox" aria-label={tr('Acrobat 레이어 만들기')} checked={layered} disabled={busy||forPrint} onChange={e=>setLayered(e.target.checked)}/> {tr('Acrobat 레이어 만들기')}</label>
        <label><input type="checkbox" aria-label={tr('내보낼 때 같은 이름 레이어 병합')} checked={book.merge_layers??true} disabled={busy} onChange={e=>void request('mergeLayers',{merge:e.target.checked})}/> {tr('내보낼 때 같은 이름 레이어 병합')}</label></p>
      <p><label><input type="checkbox" aria-label={tr('인쇄용 PDF로 내보내기')} checked={forPrint} disabled={busy} onChange={e=>setForPrint(e.target.checked)}/> {tr('인쇄용 PDF로 내보내기')}</label>
        <label><input type="checkbox" aria-label={tr('빈 페이지 인쇄')} checked={printBlank} disabled={busy||!forPrint} onChange={e=>setPrintBlank(e.target.checked)}/> {tr('빈 페이지 인쇄')}</label></p>
      {exported&&<p role='status'>{tr('책 PDF 저장: {0}, {1}쪽',{0:exported.path,1:exported.pages})}</p>}
      <p className="muted small">{tr('Folio 는 PostScript 파일 대신 PDF 로 저장합니다. 책 전체를 프린터로 바로 보내는 기능은 지원하지 않으니 문서를 열어 인쇄하세요.')}</p>
      {status.filter(s=>!s.ok).map(s=><p key={s.id} role="status">{book.documents.find(e=>e.id===s.id)?.path}: {s.error}</p>)}
    </>}
    {notice&&<p role="status">{notice}</p>}{error&&<p role="alert">{error}</p>}<footer><button disabled={busy} onClick={()=>setPhase('closed')}>{tr('닫기')}</button></footer>
  </div></div>};
}
