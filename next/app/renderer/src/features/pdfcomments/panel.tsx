import {useEffect,useMemo,useState} from 'react';
import {createPortal} from 'react-dom';
import {tr} from '../../i18n';
import type {PanelCtx} from '../../ui/panels';
import {type DocPdfComment,type DocPdfComments,acceptable,readDocumentComments,shortType} from './query';
import {type ImportAlert} from './state';
import {type Filters,NO_FILTERS,byDate,filterChoices,footerAction,menuHasStatus,passes,pdfLabel,showPdfGroup,showReviewerGroup} from './rules';
import './panel.css';

// Done (no red mark): what InDesign draws on the layout for each kind of comment (the line, arrow, oval, rectangle, polygon, connected lines, pencil and
// cloud comments are not drawn at all; marks.tsx), the eye button, the order within a page, the filter groups, the buttons and Accept, and what a click on
// an item does on the layout (fit page + the blue frame; marks.tsx): next/oracle/rules/pdfcomments.ko.md and pdfcomments-c5.ko.md.
export const PDF_COMMENT_PANELS={
  pdfComments:{get title(){return tr('PDF 주석');},icon:'info',width:370,height:540},
};
// InDesign's names for CommentTypeEnum (its panel's own Korean terms).
const types:Record<string,()=>string>={
  sticky:()=>tr('스티커 노트'),highlight:()=>tr('강조 표시된 텍스트'),underline:()=>tr('밑줄이 적용된 텍스트'),squiggly:()=>tr('자유선 텍스트'),
  strike:()=>tr('취소선이 적용된 텍스트'),replace:()=>tr('텍스트 바꾸기'),insert:()=>tr('텍스트 삽입'),typewriter:()=>tr('텍스트 주석'),
  textbox:()=>tr('텍스트 상자'),pencil:()=>tr('연필'),line:()=>tr('선'),oval:()=>tr('타원형'),rect:()=>tr('사각형'),polygon:()=>tr('다각형'),
  arrow:()=>tr('화살표'),callout:()=>tr('텍스트 설명선'),stamp:()=>tr('도장'),connected:()=>tr('연결된 선'),cloud:()=>tr('클라우드'),
};
export const typeName=(short:string)=>types[short]?.()??tr('주석');
/** `2026-11-07T08:45:00` → a short local date (InDesign shows the date only: "Nov 07, 2026"). */
export function commentDate(date:string):string {
  const m=/^(\d{4})-(\d{2})-(\d{2})/.exec(date);
  if(!m)return tr('(없음)');
  return new Date(Number(m[1]),Number(m[2])-1,Number(m[3])).toLocaleDateString(undefined,{year:'numeric',month:'short',day:'2-digit'});
}
function commentText(row:DocPdfComment):string {
  // InDesign shows a stamp's text as "<subject> : <text>" (the importer already wrote it so)
  return row.content||tr('(없음)');
}
/** The comments the document holds, read again after every edit. */
function useDocumentComments(ctx:PanelCtx):DocPdfComments|null {
  const [state,setState]=useState<DocPdfComments|null>(null);
  useEffect(()=>{
    let live=true;
    if(!ctx.doc){setState(null);return;}
    readDocumentComments().then(r=>{if(live)setState(r);},()=>{if(live)setState({available:false,comments:[],pageCount:0});});
    return ()=>{live=false;};
  },[ctx.doc,ctx.info]);
  return state;
}

function FilterPopup({rows,filters,set,close}:{rows:DocPdfComment[];filters:Filters;set:(f:Filters)=>void;close:()=>void}) {
  // InDesign 2025: collapsible groups Status, Reviewer (more than one reviewer), Type and PDF Source (more than one PDF), Clear All; the
  // choices are the ones the comments have, in the order the document first mentions them (rules.ts)
  const choices=filterChoices(rows);
  const status:Record<string,string>={open:tr('해결 안 됨'),resolved:tr('해결됨'),accepted:tr('수락됨'),unmapped:tr('매핑되지 않음')};
  const pdfName=(file:string)=>pdfLabel(rows.find(r=>r.file===file)!);
  const groups:{key:keyof Filters;title:string;values:[string,string][]}[]=[
    {key:'status',title:tr('상태'),values:choices.status.map(v=>[v,status[v]])},
    ...(showReviewerGroup(choices)?[{key:'reviewer' as const,title:tr('검토자'),values:choices.reviewer.map(v=>[v,v||tr('(없음)')] as [string,string])}]:[]),
    {key:'type',title:tr('유형'),values:choices.type.map(v=>[v,typeName(v)] as [string,string])},
    ...(showPdfGroup(choices)?[{key:'pdf' as const,title:tr('PDF 소스'),values:choices.pdf.map(v=>[v,pdfName(v)] as [string,string])}]:[]),
  ];
  const toggle=(key:keyof Filters,value:string)=>{
    const now=filters[key];
    set({...filters,[key]:now.includes(value)?now.filter(v=>v!==value):[...now,value]});
  };
  const any=groups.some(g=>filters[g.key].length);
  return <div className="pdfcomments-filter-popup" role="dialog" aria-label={tr('주석 필터')}>
    {groups.map(g=><details key={g.key} data-filter={g.key}><summary>{g.title}</summary>
      {g.values.map(([value,label])=><label key={value}><input type="checkbox" checked={filters[g.key].includes(value)} onChange={()=>toggle(g.key,value)}/>{label}</label>)}
    </details>)}
    <div className="pdfcomments-filter-actions"><button type="button" disabled={!any} onClick={()=>set(NO_FILTERS)}>{tr('모두 지우기')}</button><button type="button" onClick={close}>{tr('닫기')}</button></div>
  </div>;
}

/** The warnings of File ▸ Import PDF Comments… (InDesign's alerts, with its two buttons) and the question about a PDF outside the project. */
export function PdfCommentsAlert({alert,answer}:{alert:ImportAlert|null;answer:(accept:boolean)=>void}) {
  if(!alert)return null;
  const texts:Record<ImportAlert['kind'],[string,string]>={
    noComments:[tr('PDF에 주석이 없습니다. 계속하려면 다른 PDF를 선택하세요.'),tr('다른 파일 선택')],
    corrupt:[tr('PDF 파일이 손상되었습니다. 다른 PDF를 선택하세요.'),tr('다른 파일 선택')],
    notInDesign:[tr('Folio나 InDesign에서 만든 PDF가 아닙니다. 계속하려면 다른 PDF를 선택하세요.'),tr('다른 파일 선택')],
    otherDocument:[tr('다른 문서에서 만든 PDF입니다. 올바른 PDF를 선택하세요.'),tr('다른 파일 선택')],
    duplicate:[tr('같은 파일 이름의 PDF를 이미 가져왔습니다. 다시 가져오면 주석이 중복될 수 있습니다. 계속할까요?'),tr('계속')],
    copy:[tr('이 PDF는 프로젝트 밖에 있습니다. 프로젝트 안의 reviews 폴더로 복사해서 가져올까요?'),tr('복사해서 가져오기')],
  };
  const [message,primary]=texts[alert.kind];
  return createPortal(
    <div className="style-modal" role="alertdialog" aria-label={tr('PDF 주석 가져오기')} onPointerDown={e=>e.stopPropagation()} onKeyDown={e=>{e.stopPropagation();if(e.key==='Escape')answer(false);}}>
      <div className="card dialog style-options" data-alert={alert.kind}>
        <h2>{tr('PDF 주석 가져오기')}</h2>
        <p>{message}</p>
        <div className="dialog-actions"><button type="button" autoFocus onClick={()=>answer(true)}>{primary}</button><button type="button" onClick={()=>answer(false)}>{tr('취소')}</button></div>
      </div>
    </div>,document.body);
}

function Item({row,ctx,selected,select}:{row:DocPdfComment;ctx:PanelCtx;selected:boolean;select:()=>void}) {
  const [open,setOpen]=useState(false),[menu,setMenu]=useState(false);
  const short=shortType(row.kind),resolved=row.status==='ResolvedStatus';
  const can=ctx.editable&&!ctx.readOnly;
  const run=(label:string,ops:Parameters<PanelCtx['edit']>[1])=>{setMenu(false);if(can)void ctx.edit(label,ops);};
  // InDesign 2025: the selected comment shows one text button at the bottom right — Accept on a comment that edits text and is still tied
  // to the layout (Accepted once applied), Resolve (Unresolve once resolved) on the others, also on an orphan that edits text (rules.ts);
  // a resolved comment carries a small mark on its icon. What Accept does is measured for the strikethrough only: the others stay off.
  const action=footerAction(row);
  const accept=action==='accept';
  const toggleStatus=()=>run(tr('주석 업데이트'),[{op:'setPdfCommentStatus',ids:[row.id],status:resolved?'open':'resolved'}]);
  const act=()=>accept?run(tr('주석 적용'),[{op:'acceptPdfComment',id:row.id}]):toggleStatus();
  const replies=useMemo(()=>byDate(row.replies),[row.replies]);
  const showReplies=open||selected;
  return <li data-comment-id={row.id} data-type={short} data-status={resolved?'resolved':'open'} data-orphan={row.orphan} data-selected={selected} className={selected?'selected':undefined} onClick={select}>
    <div className="pdfcomments-heading">
      <span className="pdfcomments-icon" data-type={short} data-resolved={resolved} title={typeName(short)} aria-hidden="true"/>
      <span className="pdfcomments-who"><strong>{row.reviewer||tr('(없음)')}</strong><small>{commentDate(row.date)}</small></span>
      {selected&&row.orphan&&<span className="pdfcomments-orphan" role="img" data-applied={row.applied===true} aria-label={row.applied===true?tr('주석이 수락되어 연결된 개체가 수정되었거나 제거되었습니다.'):tr('연결된 개체가 수정되었거나 제거되었습니다.')}
        title={row.applied===true?tr('주석이 수락되어 연결된 개체가 수정되었거나 제거되었습니다.'):tr('연결된 개체가 수정되었거나 제거되었습니다.')}>
        <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M6.5 9.5 4.4 11.6a2 2 0 0 1-2.8-2.8L3.7 6.7M9.5 6.5l2.1-2.1a2 2 0 0 1 2.8 2.8l-2.1 2.1M6 10l4-4" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round"/></svg></span>}
      {selected&&<span className="pdfcomments-more-wrap" onClick={e=>e.stopPropagation()}>
        <button type="button" className="pdfcomments-more" aria-label={tr('옵션 더 보기')} aria-expanded={menu} onClick={()=>setMenu(!menu)}>···</button>
        {menu&&<ul className="pdfcomments-menu" role="menu">
          {menuHasStatus(row)&&<li role="menuitem"><button type="button" data-menu="status" disabled={!can} onClick={toggleStatus}>{resolved?tr('해결되지 않음으로 표시'):tr('해결됨으로 표시')}</button></li>}
          <li role="menuitem"><button type="button" onClick={()=>{setMenu(false);void navigator.clipboard?.writeText(row.content);}}>{tr('내용 복사')}</button></li>
          <li role="menuitem"><button type="button" disabled={!can} onClick={()=>run(tr('주석 삭제'),[{op:'removePdfComment',ids:[row.id]}])}>{tr('삭제')}</button></li>
        </ul>}
      </span>}
    </div>
    <p className="pdfcomments-content">{commentText(row)}</p>
    {(row.replies.length>0||selected)&&<div className="pdfcomments-replies-row">
      {row.replies.length>0&&<button type="button" className="pdfcomments-replies-toggle" aria-expanded={showReplies} onClick={e=>{e.stopPropagation();setOpen(!showReplies);}}>{tr('응답 {0}개',{0:row.replies.length})}</button>}
      {selected&&<span className="pdfcomments-action-wrap" onClick={e=>e.stopPropagation()}>
        {action==='accepted'?<span className="pdfcomments-accepted">{tr('수락됨')}</span>
          :<button type="button" className="pdfcomments-action" data-action={action} disabled={!can||(accept&&!acceptable(row))} title={accept?tr('주석을 레이아웃에 적용'):undefined} onClick={act}>{accept?tr('수락'):resolved?tr('해결 취소'):tr('해결')}</button>}
      </span>}
    </div>}
    {showReplies&&replies.length>0&&<ul className="pdfcomments-replies">{replies.map(reply=><li key={reply.id} data-reply-id={reply.id}>
      <strong>{reply.reviewer||tr('(없음)')}</strong> <small>{commentDate(reply.date)}</small>
      <p className="pdfcomments-content">{reply.content||tr('(없음)')}</p>
    </li>)}</ul>}
  </li>;
}

export function PdfCommentsPanel({ctx}:{ctx:PanelCtx}) {
  const state=ctx.pdfComments;
  const doc=useDocumentComments(ctx);
  const [filterOpen,setFilterOpen]=useState(false);
  const all=doc?.comments??[];
  const shown=useMemo(()=>all.filter(c=>passes(c,state?.filters??NO_FILTERS,state?.search??'')),[all,state?.filters,state?.search]);
  if(!state?.available)return <p className="p-empty">{tr('PDF 주석을 보려면 프로젝트 문서를 여세요.')}</p>;
  const pages=new Map<number,DocPdfComment[]>(),unmapped:DocPdfComment[]=[];
  for(const row of shown){
    if(row.orphan||row.page==null){unmapped.push(row);continue;}
    const list=pages.get(row.page)??[];list.push(row);pages.set(row.page,list);
  }
  // InDesign: the pages in order, the comments tied to no page last; inside each group the oldest comment first (rules.ts byDate)
  const groups=[...[...pages].sort((a,b)=>a[0]-b[0]).map(([page,rows])=>({key:String(page+1),title:tr('{0}쪽',{0:page+1}),rows:byDate(rows)})),...(unmapped.length?[{key:'unmapped',title:tr('연결되지 않은 주석'),rows:byDate(unmapped)}]:[])];
  const select=(row:DocPdfComment)=>{
    state.setSelected(row.id);
    const d=row.destination;
    // a comment on text selects that text (InDesign: app.selection becomes the commented word); the others go to their page
    if(d?.kind==='text'&&!row.orphan&&state.reveal)state.reveal({story:d.target,start:d.start,end:d.end});
    else if(row.page!=null)(ctx.fitPage??ctx.goToPage)(row.page);
  };
  return <div className="pdfcomments-panel">
    <div className="pdfcomments-search"><input type="search" aria-label={tr('주석 검색…')} placeholder={tr('주석 검색…')} value={state.search} onChange={e=>state.setSearch(e.target.value)}/></div>
    {state.toast&&<p className="pdfcomments-toast" role="status"><span>{state.toast}</span><button type="button" aria-label={tr('닫기')} onClick={()=>state.setToast('')}>×</button></p>}
    {!all.length&&<div className="pdfcomments-empty">
      <p>{tr('문서로 PDF 주석을 가져옵니다. 파일 메뉴에서도 가져올 수 있습니다.')}</p>
      <p>{tr('주석이 올바르게 연결되려면 주석을 가져오기 전에 문서를 편집하지 마세요.')}</p>
      <button type="button" disabled={!state.start||!ctx.editable||ctx.readOnly} onClick={()=>state.start?.()}>{tr('PDF 주석 가져오기…')}</button>
    </div>}
    {all.length>0&&!shown.length&&<p className="pdfcomments-none" role="status">{tr('조건에 맞는 주석이 없습니다.')}</p>}
    {groups.map(g=><section key={g.key} className="pdfcomments-page" data-page-group={g.key}>
      <h4 className="pdfcomments-page-head"><span>{g.title}</span><span className="pdfcomments-count">{g.rows.length}</span></h4>
      <ol>{g.rows.map(row=><Item key={row.id} row={row} ctx={ctx} selected={state.selected===row.id} select={()=>select(row)}/>)}</ol>
    </section>)}
    <div className="pdfcomments-footer">
      <span className="pdfcomments-total">{all.length>0&&(shown.length===1?tr('1개 주석'):tr('{0}개 주석',{0:shown.length}))}</span>
      <button type="button" className="pdfcomments-eye" data-hidden={state.hidden} disabled={!all.length} aria-pressed={state.hidden}
        aria-label={state.hidden?tr('레이아웃에서 주석 표시'):tr('레이아웃에서 주석 숨기기')} title={state.hidden?tr('레이아웃에서 주석 표시'):tr('레이아웃에서 주석 숨기기')} onClick={()=>state.setHidden(!state.hidden)}>
        <svg viewBox="0 0 20 20" width="18" height="18" aria-hidden="true"><path d="M1.5 10S5 4.5 10 4.5 18.5 10 18.5 10 15 15.5 10 15.5 1.5 10 1.5 10z" fill="none" stroke="currentColor" strokeWidth="1.5"/>
          {state.hidden?<path d="M3 3l14 14" stroke="currentColor" strokeWidth="1.5"/>:<circle cx="10" cy="10" r="2.6" fill="currentColor"/>}</svg>
      </button>
      <button type="button" className="pdfcomments-filter" disabled={!all.length} aria-expanded={filterOpen} aria-label={tr('주석 필터')} title={tr('주석 필터')} onClick={()=>setFilterOpen(!filterOpen)}>{tr('필터')}</button>
    </div>
    {filterOpen&&<FilterPopup rows={all} filters={state.filters} set={state.setFilters} close={()=>setFilterOpen(false)}/>}
  </div>;
}
