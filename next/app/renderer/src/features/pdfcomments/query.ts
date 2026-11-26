// What the document holds (File ▸ Import PDF Comments…): designmap `PDFComment`s as the engine reads them (folio_model::pdfcomments).
export type DocPdfReply = {id:string; reviewer:string; content:string; date:string};
export type DocPdfDestination = {id:string; kind:'page'|'pageItem'|'text'; target:string; point:[number,number]; rect:[number,number,number,number]; start:number; end:number};
export type DocPdfComment = {
  id:string; reviewer:string; content:string; date:string;
  /** IDML CommentType: `StickyNoteType`, `HighlightTextType`, `StrikethroughTextType` … */
  kind:string;
  file:string; status:'OpenStatus'|'ResolvedStatus'|string; orphan:boolean; applied?:boolean|null;
  destination?:DocPdfDestination|null; replies:DocPdfReply[];
  /** Added by the query: the page (0-based) the comment is on (null when it is tied to none), the PDF's file name and its project path. */
  page?:number|null; fileName?:string; path?:string|null;
};
export type DocPdfComments = {available:boolean; comments:DocPdfComment[]; pageCount:number};
export async function readDocumentComments():Promise<DocPdfComments> {
  const {header}=await window.folio.request({cmd:'pdfCommentsDoc'});
  if(!header.ok)throw Error(String(header.error));
  return {available:!!header.available,comments:(header.comments??[]) as DocPdfComment[],pageCount:Number(header.pageCount??0)};
}

/** InDesign's `CommentType` as the short name the panel keys its icons and texts by (the PDF reader's `type`). */
const SHORT:Record<string,string>={
  StickyNoteType:'sticky',HighlightTextType:'highlight',UnderlineTextType:'underline',SquigglyTextType:'squiggly',StrikethroughTextType:'strike',
  ReplaceTextType:'replace',InsertTextType:'insert',TextTypewriterType:'typewriter',TextBoxType:'textbox',FreeformDrawingType:'pencil',
  LineSegmentType:'line',OvalType:'oval',RectangleType:'rect',PolygonType:'polygon',ArrowType:'arrow',TextCalloutType:'callout',StampType:'stamp',
  ConnectedLinesType:'connected',CloudType:'cloud',
};
export const shortType=(kind:string)=>SHORT[kind]??'unknown';
/** Comments of these types are accepted by editing the text (measured: a strikethrough takes its text out). */
export const acceptable=(c:DocPdfComment)=>c.kind==='StrikethroughTextType'&&!c.orphan&&c.applied!==true&&c.destination?.kind==='text';

/** What InDesign draws on a page for the comments tied to the layout (engine query `pdfCommentMarks`; page points, origin at the page's top left). */
export type PdfCommentMark = {id:string; kind:string; boxes?:[[number,number],[number,number],[number,number],[number,number]][]; badge?:[number,number]};
export async function readPdfCommentMarks(page:number,selected?:string|null):Promise<{marks:PdfCommentMark[];selection:PdfCommentSelection|null}> {
  const {header}=await window.folio.request({cmd:'pdfCommentMarks',page,...(selected?{selected}:{})});
  if(!header.ok)throw Error(String(header.error));
  return {marks:(header.marks??[]) as PdfCommentMark[],selection:(header.selection??null) as PdfCommentSelection|null};
}
/** What InDesign shows for the comment picked in the panel (page points): a blue frame around the badge of a comment that has one (`ring`), or a blue
 *  frame mark in the middle of the page for the comments it does not draw (`center`). */
export type PdfCommentSelection = {id:string; kind:string; ring?:[number,number]; center?:[number,number]};
