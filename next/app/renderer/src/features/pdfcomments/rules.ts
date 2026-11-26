// What the PDF Comments panel of InDesign 2025 (the CommentsReview extension) decides from the comments' data. The rules were read from
// the panel's own script and checked against the application: next/oracle/rules/pdfcomments.ko.md (2026-11-11).
import {type DocPdfComment,shortType} from './query';

/** The panel's filter: nothing chosen in a group means no filtering by it (all of its values are shown). */
export type Filters={status:string[];reviewer:string[];type:string[];pdf:string[]};
export const NO_FILTERS:Filters={status:[],reviewer:[],type:[],pdf:[]};

const STAMP=/^(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2})(?::(\d{2}))?/;
const stamp=(date:string)=>{const m=STAMP.exec(date);return m?new Date(+m[1],+m[2]-1,+m[3],+m[4],+m[5],+(m[6]??0)).getTime():NaN;};
/**
 * Comments of one page (and the replies of one comment) are listed oldest first by their date. Measured: a PDF whose annotations are in
 * the order note, highlight, underline, strikethrough, text box, early highlight, with dates that disagree with that order, lists as
 * early highlight, highlight, strikethrough, text box, underline, note; two comments with the same date keep the document's order
 * (the sort is stable). A date that is missing compares equal to every other.
 */
export function byDate<T extends {date:string}>(rows:readonly T[]):T[] {
  return [...rows].sort((a,b)=>{const x=stamp(a.date),y=stamp(b.date);return x<y?-1:x>y?1:0;});
}

/** Comments that edit the text (strikethrough, replace, insert) carry `applied` (InDesign: the property `commentIsApplied` exists for them only). */
export const hasApply=(c:DocPdfComment)=>c.applied!=null;
/** The text button at the bottom right of the selected comment. Accept is for a comment that edits the text and is still tied to the layout;
 *  once accepted it says Accepted; a comment that edits the text but lost its object (orphan) offers Resolve like the other comments. */
export type FooterAction='accepted'|'accept'|'resolve'|'unresolve';
export function footerAction(c:DocPdfComment):FooterAction {
  if(hasApply(c)&&(c.applied===true||!c.orphan))return c.applied===true?'accepted':'accept';
  return c.status==='ResolvedStatus'?'unresolve':'resolve';
}
/** For those comments the ··· menu also holds Mark as Resolved / Unresolved (the button is Accept, so the status toggle moves there). */
export const menuHasStatus=(c:DocPdfComment)=>hasApply(c)&&(c.applied===true||!c.orphan);

/** The comment's status as the filter names it. */
export const statusKey=(c:DocPdfComment):'open'|'resolved'=>c.status==='OpenStatus'?'open':'resolved';
/** A PDF's name as the filter lists it: the file name up to its first dot. */
export const pdfLabel=(c:DocPdfComment)=>((c.fileName||c.file).replace(/^.*[\\/:]/,'')).split('.')[0];

/**
 * The choices of the filter, each in the order the document's comments first mention them (Status: Unresolved, Resolved, Accepted and
 * Unmapped only when a comment is in that state; Reviewer: the authors of comments and of replies; Type: the types present; PDF Source:
 * the imported files). The groups Reviewer and PDF Source are offered only when there is more than one choice.
 */
export type FilterChoices={status:string[];reviewer:string[];type:string[];pdf:string[]};
export function filterChoices(all:readonly DocPdfComment[]):FilterChoices {
  const out:FilterChoices={status:[],reviewer:[],type:[],pdf:[]};
  const add=(list:string[],value:string)=>{if(!list.includes(value))list.push(value);};
  for(const c of all){
    add(out.reviewer,c.reviewer);
    c.replies.forEach(r=>add(out.reviewer,r.reviewer));
    add(out.type,shortType(c.kind));
    add(out.status,statusKey(c));
    if(c.orphan){
      if(c.applied===true)add(out.status,'accepted');
      add(out.status,'unmapped');
    }
    add(out.pdf,c.file);
  }
  return out;
}
export const showReviewerGroup=(choices:FilterChoices)=>choices.reviewer.length>1;
export const showPdfGroup=(choices:FilterChoices)=>choices.pdf.length>1;

const same=(chosen:string[],value:string)=>!chosen.length||chosen.includes(value);
/** The filter and the search over the author and text of a comment and of its replies (InDesign: a reply by the chosen reviewer keeps its comment). */
export function passes(c:DocPdfComment,f:Filters,query:string):boolean {
  if(query){
    const q=query.toLowerCase();
    if(![c.reviewer,c.content,...c.replies.flatMap(r=>[r.reviewer,r.content])].some(t=>t.toLowerCase().includes(q)))return false;
  }
  const status=!f.status.length||f.status.includes(statusKey(c))||(f.status.includes('accepted')&&c.applied===true)||(f.status.includes('unmapped')&&c.orphan);
  const reviewer=!f.reviewer.length||f.reviewer.includes(c.reviewer)||c.replies.some(r=>f.reviewer.includes(r.reviewer));
  return status&&reviewer&&same(f.type,shortType(c.kind))&&same(f.pdf,c.file);
}
