import { typedAutoCorrect } from '../features/spelling/typing';
import { t as tr } from '../i18n';
// One serialized input stream per document. DOM/IME offsets never reach the engine:
// its indexes are Unicode scalars; arrows and deletion advance whole graphemes.
import { useContext, useEffect, useRef, useState } from 'react';
import { commandKey, textKey, verticalMove } from './keys';
import { movedCell, type CellMove } from './tablekeys';
import { cellSizeCommands, cellSizeLabel, type CellSizeStep } from './tablekeysize';
import { PreviewEditContext, type PreviewPort } from './controls/usePreviewEdit';
import { type EditCommand, type EditResult, type TableHit, type TableInfo, type TextGeometry, type TextHit, type TextFragment, type TextFormat, type TextProperties, type TextRect, tableCells, tableHit, tableInfo, textCursor, textHit, textSelection, textStory, textFormat, textCopy, textNavigate } from '../engine';

export type TextSelectionState = { story:string;start:number;end:number;format:TextFormat;fonts:Record<string,string[]>; features:string[]; nested?:boolean;
  noteContext?: import('../engine').NoteContext;
  /** Text variable instances (and page number marks) in the selection: Convert Variable to Text needs one. */
  variables:number };
/** Selected table cells: the cell clicked (`anchor`) and the one dragged to (`end`), grown over
 * merged cells to `from`..`to` ([column, row]); the table as the Table menu sees it. */
export type TableSelection = Pick<TableHit, 'story' | 'table' | 'frame' | 'page'> & {
  anchor: [number, number]; end: [number, number]; from: [number, number]; to: [number, number]; info: TableInfo;
  /** The cell (its `Self`) whose text has the text cursor: the cell is not shown selected. */
  cursor?: string };
export type TableActions = { select: (anchor: [number, number], end: [number, number]) => void; exit: () => void;
  move: (move: CellMove) => void;
  resize: (step: CellSizeStep) => void;
  toggleAutogrow: () => void;
  /** The text cursor in a cell's text ([column, row]: Table ▸ Edit Header / Edit Footer). */
  edit: (cell: [number, number]) => void };
export type TextActions = { history: (redo: boolean) => void; exit: () => Promise<void>;
  footnote: () => void;
  /** Type ▸ Insert Endnote: the reference goes after the selection (a range keeps its text). */
  endnote: () => void;
  goToFootnote: () => void;
  goToEndnote: () => void;
  format: (props:TextProperties)=>Promise<void>;
  /**
   * The engine's preview session for the character and paragraph panels' number fields (ui/controls/README.ko.md): a value tried is shown
   * provisionally, and made one undo step or given back exactly. Unlike `format` it leaves the keyboard focus in the field.
   */
  preview: PreviewPort | null;
  style:(kind:'paragraph'|'character',style:string,clear:boolean)=>void;
  clearOverrides:()=>void;
  insertGlyph:(text:string,props:TextProperties)=>void;
  /** Glyphs panel: one glyph of a face by glyph id (engine command `insertGlyph`; a glyph without a Unicode value too). */
  insertGlyphById:(family:string,style:string,glyph:number)=>void;
  /** Type ▸ Bulleted & Numbered Lists: bullets (numbers false) or numbers on / off for the selection's paragraphs. */
  list:(numbers:boolean)=>void;
  /** Type ▸ Bulleted & Numbered Lists ▸ Restart/Continue Numbering. */
  restartNumbering:()=>void;
  clipboard:(how:'copy'|'cut'|'paste'|'pastePlain')=>void; selectAll:()=>void; clear:()=>void;
  /** Type ▸ Insert Special Character / White Space / Break Character, in place of the selection. */
  special:(name:string)=>void;
  /** Type ▸ Text Variables ▸ Insert Variable: an instance of the text variable (its `Self`), in place of the selection. */
  variable:(id:string)=>void;
  /** Type ▸ Text Variables ▸ Convert Variable to Text: the instances in the selection become the text they show. */
  convertVariables:()=>void;
  /** Type ▸ Change Case of the selection. */
  changeCase:(mode:'UPPERCASE'|'LOWERCASE'|'TITLECASE'|'SENTENCECASE')=>void };
type Session = TextHit & { anchor: number; text: string; affinity: string; nested?:boolean; noteContext?: import('../engine').NoteContext };
type CursorState = Pick<Session, 'story' | 'at' | 'anchor' | 'affinity'>;
const cursorState = (s: Session): CursorState => ({ story: s.story, at: s.at, anchor: s.anchor, affinity: s.affinity });
type Options = {
  reveal?: {story:string;start:number;end:number;allowOverset?:boolean}|null;
  editable: boolean; tool: string; document: string; revision: unknown;
  tripleClickSelectsLine?: boolean;
  /** Preferences ▸ Type ▸ Adjust Spacing Automatically When Cutting and Pasting Words (on unless false): a cut or paste runs the engine's adjustSpacing too. */
  smartSpacing?: boolean;
  /** Preferences ▸ Type ▸ Drag and Drop Text Editing ▸ Enable in Layout View (off unless true): selected text dragged moves (Ctrl or Alt: a copy). */
  dragDrop?: boolean;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
  history: (redo: boolean) => Promise<EditResult | null>;
  actions: React.MutableRefObject<TextActions | null>;
  onState: (state:TextSelectionState|null)=>void;
  onTable?: (state: TableSelection | null) => void;
  /** Filled while cells are selected: select others of the same table, leave the cells. */
  tableActions?: React.MutableRefObject<TableActions | null>;
  onTableKey?: (event: KeyboardEvent) => boolean;
  selectFrame: (frame: string, page: number) => void;
  enterTool: () => void; exitTool: () => void; note: (message: string) => void;
};
const count = (s: string) => Array.from(s).length;
const segmenter = new Intl.Segmenter('ko', { granularity: 'grapheme' });
const words = new Intl.Segmenter('ko', { granularity:'word' });
function wordRanges(text:string) {
  let at=0; return [...words.segment(text)].map((s)=>{const start=at;at+=count(s.segment);return {start,end:at,word:s.isWordLike};});
}
// A fragment stays inside its source document. Only our own clipboard token restores
// formatting; external/system text is always inserted as plain text.
let clipboard: {token:string;document:string;fragment:TextFragment}|null=null;
// Edit ▸ Paste without Formatting: the next paste drops the copied formatting, so the text takes
// the formatting where it lands (as typing does).
let plainPaste=false;
function boundary(text: string, at: number, direction: -1 | 1) {
  const stops = [0]; let n = 0;
  for (const part of segmenter.segment(text)) { n += count(part.segment); stops.push(n); }
  return direction < 0 ? stops.filter((s) => s < at).at(-1) ?? 0 : stops.find((s) => s > at) ?? n;
}
function wordBoundary(text: string, at: number, direction: -1 | 1) {
  const stops=wordRanges(text).filter(w=>w.word).map(w=>w.start);
  return direction<0 ? stops.filter(i=>i<at).at(-1)??0 : stops.find(i=>i>at)??count(text);
}

export function useTextEditor(options: Options) {
  const opts = useRef(options); opts.current = options;
  const session = useRef<Session | null>(null);
  const [state, setState] = useState<Session | null>(null);
  const [geometry, setGeometry] = useState<TextGeometry>({ caret: null, rects: [] });
  const [preedit, setPreedit] = useState('');
  const [compositionCaret, setCompositionCaret] = useState<TextGeometry['caret']>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const composing = useRef(false);
  const suppressCommit = useRef<string | null>(null);
  const chain = useRef(Promise.resolve());
  const queueEpoch = useRef(0);
  const formatBatch = useRef<{session:Session; document:string; from:Element|null; pending:Promise<EditResult|null>[]; done:Promise<void>}|null>(null);
  const alive = useRef(true);
  const knownRev = useRef<number | null>(null);
  // how many characters of the word before the caret were typed (the engine counts; autocorrect leaves a word nobody typed alone)
  const typedRun = useRef(0);
  // Only cursor positions belong here. The engine owns byte-splice undo; retaining a
  // whole story per keypress would turn a long article's history into quadratic storage.
  const selectionHistory = useRef<{ before: CursorState; after: CursorState }[]>([]);
  const undone = useRef<typeof selectionHistory.current>([]);
  const dragging = useRef(false);
  /** Drag and drop text editing (tools-c2): the range picked up, where the pointer went down and where it is now; `dropCaret` is drawn where it would land. */
  const textDrag = useRef<{ story: string; lo: number; hi: number; from: [number, number, number, string | undefined]; to?: [number, number, number, string | undefined] } | null>(null);
  const [dropCaret, setDropCaret] = useState<TextRect | null>(null);
  const desiredX=useRef<number|undefined>(undefined);
  const formats=useRef<{format:TextFormat;fonts:Record<string,string[]>;features:string[]}>({format:{},fonts:{},features:[]});
  const copied=useRef<{key:string;fragment:TextFragment}|null>(null);
  const cells = useRef<TableSelection | null>(null);
  const [cellRects, setCellRects] = useState<TextRect[]>([]);
  const [cellState, setCellState] = useState<TableSelection | null>(null);
  const publishCells = (sel: TableSelection | null, rects: TextRect[]) => {
    cells.current = sel; setCellRects(rects); setCellState(sel); opts.current.onTable?.(sel);
  };
  /** Selects cells of a table (kept inside the table, which may have changed). */
  const selectCells = async (sel: Omit<TableSelection, 'from' | 'to' | 'info'>) => {
    let info: TableInfo;
    try { info = (await tableInfo(sel.table)).table; } catch { publishCells(null, []); return; }
    if (!info.rows.length || !info.columns.length) { publishCells(null, []); return; }
    const clamp = (c: [number, number]): [number, number] => [Math.min(c[0], info.columns.length - 1), Math.min(c[1], info.rows.length - 1)];
    const anchor = clamp(sel.anchor), end = clamp(sel.end);
    const r = await tableCells(sel.story, sel.table, anchor, end);
    if (!alive.current) return;
    publishCells({ ...sel, anchor, end, from: r.from, to: r.to, info }, sel.cursor ? [] : r.rects);
  };
  /** The text cursor in a cell's text (`select`: all of it, as Tab does). */
  const enterCell = async (sel: Omit<TableSelection, 'from' | 'to' | 'info'> & { cursor: string }, select: boolean) => {
    const info = await textStory(sel.cursor);
    await textCursor(sel.cursor);
    const at = count(info.text);
    session.current = { story: sel.cursor, frame: sel.frame, page: sel.page, at, anchor: select ? 0 : at, text: info.text, affinity: 'downstream' };
    desiredX.current = undefined;
    await selectCells(sel);
    await refresh(); focus();
  };
  /** Tab / Shift+Tab in a cell's text: the next or previous cell (after the last one a new row,
   * as InDesign adds one), its text selected. */
  const tabCell = (back: boolean) => enqueue(async () => {
    const c = cells.current, s = session.current;
    if (!c?.cursor || !s) return;
    const order = [...c.info.cells].sort((a, b) => a.cell[1] - b.cell[1] || a.cell[0] - b.cell[0]);
    const k = order.findIndex((x) => x.id === c.cursor);
    let next: TableInfo['cells'][number] | undefined = order[k + (back ? -1 : 1)];
    if (!next && back) return;
    if (!next) {
      const last = c.info.rows.length - 1;
      const r = await opts.current.edit(tr('행 삽입'), [{ op: 'insertRows', table: c.table, at: last, count: 1, before: false }]);
      if (!r) return;
      knownRev.current = r.rev;
      const info = (await tableInfo(c.table)).table;
      next = info.cells.find((x) => x.cell[1] === last + 1 && x.cell[0] === 0);
      if (!next) return;
    }
    // a graphic cell has no text: it is chosen (its content), as a click chooses it (tools-c2)
    if (next.graphic) { session.current = null; setGeometry({ caret: null, rects: [] }); publish(); await selectCells({ ...c, anchor: next.cell, end: next.cell, cursor: next.id }); focus(); return; }
    await enterCell({ ...c, anchor: next.cell, end: next.cell, cursor: next.id }, true);
  });
  /** Graphic cells (tools-c2): with the Type tool a graphic cell is chosen as its content (the cell's cursor, nothing shown selected) or as the cell
   * (shown selected). Esc switches between the two; the arrows go to the cell beside it and Tab / Shift+Tab to the next / previous cell (a text cell:
   * its text, all selected, as Tab does; a graphic cell: chosen). Returns whether the key was one of these. */
  const graphicCellKey = (e: React.KeyboardEvent) => {
    const c = cells.current;
    if (!c || e.ctrlKey || e.metaKey || e.altKey) return false;
    const one = c.from[0] === c.to[0] && c.from[1] === c.to[1];
    const at = (col: number, row: number) => c.info.cells.find((x) => col >= x.cell[0] && col < x.cell[0] + (x.span?.[0] ?? 1) && row >= x.cell[1] && row < x.cell[1] + (x.span?.[1] ?? 1));
    const here = c.cursor ? c.info.cells.find((x) => x.id === c.cursor) : one ? at(c.from[0], c.from[1]) : undefined;
    if (!here?.graphic) return false;
    const go = (next: TableInfo['cells'][number] | undefined) => {
      if (!next) return;
      const sel = { story: c.story, table: c.table, frame: c.frame, page: c.page, anchor: next.cell, end: next.cell, cursor: next.id };
      enqueue(async () => { if (next.graphic) { await selectCells(sel); focus(); } else await enterCell(sel, true); });
    };
    if (e.key === 'Escape') { enqueue(async () => { await selectCells({ story: c.story, table: c.table, frame: c.frame, page: c.page, anchor: here.cell, end: here.cell, cursor: c.cursor ? undefined : here.id }); focus(); }); return true; }
    if (e.key === 'Tab') {
      const order = [...c.info.cells].sort((a, b) => a.cell[1] - b.cell[1] || a.cell[0] - b.cell[0]);
      go(order[order.findIndex((x) => x.id === here.id) + (e.shiftKey ? -1 : 1)]);
      return true;
    }
    if (e.shiftKey) return false;
    const d = e.key === 'ArrowLeft' ? [-1, 0] : e.key === 'ArrowRight' ? [here.span?.[0] ?? 1, 0] : e.key === 'ArrowUp' ? [0, -1] : e.key === 'ArrowDown' ? [0, here.span?.[1] ?? 1] : null;
    if (!d) return false;
    go(at(here.cell[0] + d[0], here.cell[1] + d[1]));
    return true;
  };
  const publish = () => {
    const s=session.current; setState(s ? { ...s } : null);
    opts.current.onState(s ? {story:s.story,nested:s.nested,noteContext:s.noteContext,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor),variables:Array.from(s.text).slice(Math.min(s.at,s.anchor),Math.max(s.at,s.anchor)).filter(c=>c==='\u0018').length,...formats.current}:null);
  };
  const enqueue = (work: () => Promise<void>) => {
    // A cursor/content operation is a barrier: subsequent formatting must wait for it.
    formatBatch.current = null;
    queueEpoch.current++;
    const document = opts.current.document;
    chain.current = chain.current.then(() => { if (alive.current && opts.current.document === document) return work(); }).catch((e) => opts.current.note(String(e.message ?? e)));
    return chain.current;
  };
  const focus = () => textarea.current?.focus({ preventScroll: true });
  const restoreFocus = (from: Element | null) => {
    const now = document.activeElement;
    if (!now || now === document.body || now === from || now === textarea.current) focus();
  };
  const refresh = async () => {
    const s = session.current;
    if (!s) return;
    let info: Awaited<ReturnType<typeof textStory>>;
    try { info = await textStory(s.story, s.at); } catch (e) {
      // the cell with the cursor is gone (merged, deleted): its place in the table is selected
      const c = cells.current;
      if (c?.cursor !== s.story) throw e;
      session.current = null; setGeometry({ caret: null, rects: [] }); publish();
      await selectCells({ ...c, cursor: undefined });
      return;
    }
    if (knownRev.current !== null && knownRev.current !== info.rev) {
      selectionHistory.current = []; undone.current = [];
    }
    knownRev.current = info.rev;
    s.nested = info.cell;
    s.noteContext = info.noteContext;
    s.text = info.text; s.at = Math.min(s.at, info.length); s.anchor = Math.min(s.anchor, info.length);
    const g = await textSelection(s.story, s.anchor, s.at, s.affinity);
    formats.current=await textFormat(s.story,Math.min(s.at,s.anchor),Math.max(s.at,s.anchor));
    copied.current=null;
    if(s.at!==s.anchor) {
      try { const r=await textCopy(s.story,Math.min(s.at,s.anchor),Math.max(s.at,s.anchor)); copied.current={key:`${s.story}:${s.anchor}:${s.at}:${info.rev}`,fragment:r.fragment}; }
      catch { /* Inline objects deliberately cannot enter the plain-text clipboard. */ }
    }
    if (!alive.current || session.current !== s) return;
    if (g.caret) { s.frame=g.caret.frame; s.page=g.caret.page; }
    setGeometry(g); publish();
    if (info.overset && !g.caret) opts.current.note(tr("넘친 글을 편집하고 있습니다."));
  };
  const history = (redo: boolean) => {
    const focusFrom = document.activeElement;
    enqueue(async () => {
      // Undo/Redo of the document does not need a caret: the session can have ended (a note placed with the Note tool leaves
      // the text) while the menu still offers the command until React has rendered the new state; the command was dropped then.
      const s = session.current;
      if (composing.current) return;
      const r = await opts.current.history(redo);
      if (!r) return;
      knownRev.current = r.rev;
      if (!s) return;
      const from = redo ? undone.current : selectionHistory.current;
      const to = redo ? selectionHistory.current : undone.current;
      const h = from.pop();
      if (h && (redo ? h.before.story : h.after.story) === s.story) {
        const saved = redo ? h.after : h.before;
        s.story = saved.story;
        s.at = saved.at; s.anchor = saved.anchor; s.affinity = saved.affinity;
      }
      if (h) to.push(h);
      await refresh(); restoreFocus(focusFrom);
    });
  };
  const exit = () => { enqueue(async () => {
    const c = cells.current, s = session.current;
    if (!c && !s) return;
    if (c) publishCells(null, []);
    composing.current = false; setPreedit(''); session.current = null;
    setGeometry({ caret: null, rects: [] }); publish();
    textarea.current?.blur();
    const at = (c ?? s)!;
    opts.current.selectFrame(at.frame, at.page); opts.current.exitTool();
  }); return chain.current; };
  // The preview port of the number fields: the window's session, with what the text editor keeps next to the engine's revision
  // (the cursor of each undo step) looked after when a trial ends. A trial changes no text, so the cursor stays where it is.
  const previewSession=useContext(PreviewEditContext);
  const noteRev=(r:unknown)=>{const rev=(r as {rev?:number}|null)?.rev;if(typeof rev==='number')knownRev.current=rev;};
  const settlePreview=(commit:boolean)=>new Promise<unknown>(resolve=>enqueue(async()=>{
    const s=session.current,before=s?cursorState(s):null,rev=knownRev.current;
    let r:EditResult|null=null;
    try{r=(commit?await previewSession!.commit():await previewSession!.cancel()) as EditResult|null;}finally{resolve(r);}
    if(!r)return;
    knownRev.current=r.rev;await refresh();
    if(commit&&s&&before&&r.rev!==rev){selectionHistory.current.push({before,after:cursorState(s)});undone.current=[];}
  }));
  const previewPort:PreviewPort|null=previewSession?{
    get open(){return previewSession.open;},
    apply:(label,commands,onReply)=>{if(!composing.current)previewSession.apply(label,commands,r=>{noteRev(r);onReply?.(r);});},
    commit:()=>settlePreview(true),
    cancel:()=>settlePreview(false),
    discard:()=>previewSession.discard(),
  } as PreviewPort:null;
  const applyFormat=(make:(s:Session)=>EditCommand,label:string,pipeline=false)=>{
    // Where the keyboard focus was when the button or list was used. The text takes the focus back when the edit is shown, but not from a box the
    // user has moved on to meanwhile (a slow story: typing a size into the next field would be cut off, and the half-typed entry committed).
    const from=document.activeElement;
    const active=formatBatch.current;
    if(pipeline&&active&&alive.current&&active.document===opts.current.document&&active.session===session.current&&!composing.current){
      active.pending.push(opts.current.edit(label,[make(active.session)]));
      active.from=from;
      return active.done;
    }
    const epoch=queueEpoch.current+1;
    return enqueue(async()=>{
      const s=session.current;if(!s || composing.current) return;
      const before=cursorState(s), rev=knownRev.current;
      if(pipeline){
        // Formatting keeps the selection fixed. Send consecutive field commits in order
        // without waiting for layout and panel refresh between them.
        const batch={session:s,document:opts.current.document,from,pending:[opts.current.edit(label,[make(s)])],done:chain.current};
        if(queueEpoch.current===epoch)formatBatch.current=batch;
        try{
          for(const request of batch.pending){
            const r=await request;if(!r)continue;
            if(r.rev!==knownRev.current){selectionHistory.current.push({before,after:cursorState(s)});undone.current=[];}
            knownRev.current=r.rev;
          }
        }finally{if(formatBatch.current===batch)formatBatch.current=null;}
        await refresh();restoreFocus(batch.from);
        return;
      }
      const r=await opts.current.edit(label,[make(s)]);if(!r)return;
      knownRev.current=r.rev;await refresh();
      restoreFocus(from);
      if(r.rev!==rev){selectionHistory.current.push({before,after:cursorState(s)});undone.current=[];}
    });
  };
  if (options.tableActions) options.tableActions.current = cellState ? {
    toggleAutogrow: () => enqueue(async () => {
      const c = cells.current; if (!c) return;
      const { table: info } = await tableInfo(c.table);
      const rows = Array.from({ length: c.to[1] - c.from[1] + 1 }, (_, i) => c.from[1] + i);
      const autoGrow = !rows.every(i => info.rows[i].autoGrow);
      const r = await opts.current.edit(tr('행 자동 증가 전환'), [{ op: 'setRows', table: c.table, rows, autoGrow }]);
      if (r) { knownRev.current = r.rev; await selectCells(c); focus(); }
    }),
    resize: step => enqueue(async () => {
      const c = cells.current; if (!c || c.cursor) return;
      const commands = await cellSizeCommands(c, step);
      if (!commands.length) return;
      const r = await opts.current.edit(cellSizeLabel(step.axis), commands);
      if (r) { knownRev.current = r.rev; await selectCells(c); focus(); }
    }),
    move: move => enqueue(async () => {
      const c = cells.current; if (!c || (c.cursor && !move.edge)) return;
      const end = movedCell(c, move);
      if (c.cursor) {
        const target = c.info.cells.find(x => x.cell.every((n, i) => n <= end[i] && end[i] < n + x.span[i]));
        if (target) await enterCell({ ...c, anchor: target.cell, end: target.cell, cursor: target.id }, true);
        return;
      }
      await selectCells({ ...c, anchor: move.extend ? c.anchor : end, end }); focus();
    }),
    select: (anchor, end) => enqueue(async () => { const c = cells.current; if (c) await selectCells({ ...c, anchor, end }); }),
    edit: (cell) => enqueue(async () => {
      const c = cells.current, x = c?.info.cells.find((k) => k.cell[0] === cell[0] && k.cell[1] === cell[1]);
      if (c && x) await enterCell({ ...c, anchor: x.cell, end: x.cell, cursor: x.id }, false);
    }),
    exit,
  } : null;
  options.actions.current = state ? { history, exit,
    footnote:()=>insertFootnote(),
    endnote:()=>insertEndnote(),
    goToFootnote:()=>goToFootnote(),
    goToEndnote:()=>goToFootnote(true),
    clipboard:how=>{focus();plainPaste=how==='pastePlain';void window.folio.clipboardCommand(how==='pastePlain'?'paste':how).catch(e=>{plainPaste=false;opts.current.note(String(e));});},
    selectAll:()=>enqueue(async()=>{const s=session.current;if(s){s.anchor=0;s.at=count(s.text);await textCursor(s.story);await refresh();focus();}}),
    clear:()=>replace(''),
    insertGlyph:(text,props)=>replace(text,undefined,undefined,'character',props),
    insertGlyphById:(family,style,glyph)=>insertGlyphById(family,style,glyph),
    special:name=>insertSpecial(name),
    variable:id=>insertVariable(id),
    convertVariables:()=>convertVariables(),
    changeCase:mode=>applyFormat(s=>({op:'changeCase',story:s.story,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor),mode}),tr('대/소문자 변경')),
    preview:previewPort,
    format:props=>applyFormat(s=>({op:'textFormat',story:s.story,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor),props}),tr("글 서식"),true),
    restartNumbering:()=>applyFormat(s=>({op:'toggleNumberingRestart',story:s.story,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor)}),tr('번호 다시 매기기/계속 매기기')),
    list:numbers=>applyFormat(s=>({op:'toggleList',story:s.story,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor),numbers}),numbers?tr('번호 적용/제거'):tr('글머리 기호 적용/제거')),
    style:(kind,style,clear)=>applyFormat(s=>({op:kind==='paragraph'?'paragraphStyle':'characterStyle',story:s.story,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor),style,clear}),tr("글 스타일")),
    clearOverrides:()=>applyFormat(s=>({op:'clearTextOverrides',story:s.story,start:Math.min(s.at,s.anchor),end:Math.max(s.at,s.anchor)}),tr('모든 재정의 지우기'))
  } : null;
  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; opts.current.actions.current = null; };
  }, []);
  useEffect(() => {
    session.current = null; publish(); setGeometry({ caret: null, rects: [] }); setPreedit('');
    publishCells(null, []);
    selectionHistory.current = []; undone.current = [];
    knownRev.current = null;
  }, [options.document]);
  useEffect(() => {
    if (session.current) enqueue(refresh);
    if (cells.current) enqueue(async () => { const c = cells.current; if (c) await selectCells(c); });
  }, [options.revision]);
  useEffect(() => {
    const wanted=options.reveal;if(!wanted)return;
    enqueue(async()=>{
      if(composing.current||!opts.current.editable)return;
      const info=await textStory(wanted.story);
      const g=await textSelection(wanted.story,wanted.start,wanted.end);
      let c: {frame:string;page:number}|undefined=g.caret??g.rects[0];
      if(!c&&wanted.allowOverset){
        const first=await textSelection(wanted.story,0,0);
        c=first.caret??first.rects[0];
        if(!c){
          const {header}=await window.folio.request({cmd:'textLayout',story:wanted.story});
          if(header.ok)c=(header.frames as {frame:string;page:number}[]).at(-1);
        }
      }
      if(!c){opts.current.note(tr('찾은 글이 넘침 영역에 있습니다.'));return;}
      session.current={story:wanted.story,frame:c.frame,page:c.page,at:wanted.end,anchor:wanted.start,text:info.text,affinity:'downstream'};
      opts.current.enterTool();opts.current.selectFrame(c.frame,c.page);
      await refresh();
    });
  },[options.reveal]);
  // The Gradient tool keeps the text selection (InDesign 2025, real input: choosing it with a range or a caret selected keeps both, and keys still type);
  // any other tool ends the text editing.
  useEffect(() => { if ((session.current || cells.current) && (!options.editable || (options.tool !== 'type' && !(options.tool === 'gradient' && !cells.current)))) exit(); }, [options.editable, options.tool]);
  useEffect(() => { if (state || cellState) focus(); }, [!!state, !!cellState]);
  useEffect(() => {
    // Keep the native IME candidate window next to the page caret even though the
    // textarea's own glyphs are transparent. Zoom and scrolling use actual CSS pixels.
    const c=preedit ? compositionCaret ?? geometry.caret : geometry.caret;
    const pg=c && document.querySelector<HTMLElement>(`.page[data-page="${c.page}"]`);
    const view=pg?.closest<HTMLElement>('.view');
    const width=pg?.querySelector<SVGSVGElement>('.text-edit-overlay')?.viewBox.baseVal.width;
    if (c && pg && view && width) {
      const b=pg.getBoundingClientRect(), v=view.getBoundingClientRect(), k=b.width/width;
      const x=b.left+c.rect[0]*k, top=b.top+c.rect[1]*k, bottom=b.top+c.rect[3]*k;
      view.scrollLeft += x<v.left+16 ? x-v.left-16 : x>v.right-16 ? x-v.right+16 : 0;
      view.scrollTop += top<v.top+16 ? top-v.top-16 : bottom>v.bottom-16 ? bottom-v.bottom+16 : 0;
    }
    const position = () => {
      const c=preedit ? compositionCaret ?? geometry.caret : geometry.caret, el=textarea.current;
      const pg=c && document.querySelector<HTMLElement>(`.page[data-page="${c.page}"]`);
      if (!c || !el || !pg) return;
      const b=pg.getBoundingClientRect();
      const svg=pg.querySelector<SVGSVGElement>('.text-edit-overlay');
      const width=svg?.viewBox.baseVal.width;
      if (!width) return;
      const k=b.width/width;
      el.style.left=`${b.left+c.rect[0]*k}px`; el.style.top=`${b.top+c.rect[1]*k}px`;
      el.style.height=`${Math.max(1,(c.rect[3]-c.rect[1])*k)}px`;
    };
    position(); window.addEventListener('scroll',position,true); window.addEventListener('resize',position);
    return () => { window.removeEventListener('scroll',position,true); window.removeEventListener('resize',position); };
  }, [geometry, compositionCaret, !!preedit]);

  const point = (page: number, x: number, y: number, frame?: string, extend = false, sameStory = false, clicks = 1) => enqueue(async () => {
    if (!opts.current.editable || composing.current) return;
    // In a table the type tool works as InDesign's: a click puts the cursor in the cell's text,
    // a drag or Shift+click into another cell selects the cells between.
    const cell = (await tableHit(page, x, y)).hit;
    const c = cells.current;
    if (cell) {
      const same = c && c.table === cell.table ? c : null;
      if ((sameStory || extend) && same && same.cursor !== cell.id) {
        session.current = null; setGeometry({ caret: null, rects: [] }); publish();
        await selectCells({ ...same, cursor: undefined, end: cell.cell });
        return;
      }
      if (sameStory && !same) return;
      if (!(sameStory || extend) || !same) {
        await selectCells({ story: cell.story, table: cell.table, frame: cell.frame, page: cell.page, anchor: cell.cell, end: cell.cell, cursor: cell.id });
      }
    } else {
      if (sameStory && c) return;
      if (c) publishCells(null, []);
    }
    const { hit } = await textHit(page, x, y, frame, cell?.id);
    // a graphic cell has no text: it is chosen (its content), and the keys come here (graphicCellKey)
    if (!hit) { if (cell && !sameStory) { session.current = null; setGeometry({ caret: null, rects: [] }); publish(); focus(); } return; }
    const old = session.current;
    if (sameStory && old?.story !== hit.story) return;
    const info = await textStory(hit.story);
    await textCursor(hit.story);
    // Snap a hit inside a multi-scalar grapheme to its leading boundary.
    const at = boundary(info.text, hit.at + 1, -1);
    session.current = { ...hit, at, anchor: extend && old?.story === hit.story ? old.anchor : at, text: info.text, affinity: 'downstream' };
    typedRun.current = 0; // a click puts the caret somewhere new: the word before it was not typed (arrow keys keep the count, measured)
    desiredX.current=undefined;
    const s=session.current;
    if(clicks===2) {
      const word=wordRanges(info.text).find(w=>w.start<=at&&at<w.end);
      if(word){s.anchor=word.start;s.at=word.end;}
    } else if(clicks===3 && opts.current.tripleClickSelectsLine !== false) {
      const g=await textSelection(s.story,at,at);
      if(g.caret){s.anchor=g.caret.lineStart;s.at=g.caret.lineSelectionEnd;s.affinity='upstream';}
    } else if(clicks>=3) {
      const chars=Array.from(info.text);
      s.anchor=chars.slice(0,at).lastIndexOf('\r')+1;
      const end=chars.indexOf('\r',at);s.at=end<0?chars.length:end+1;
    }
    opts.current.enterTool(); opts.current.selectFrame(hit.frame, hit.page);
    await refresh(); focus();
  });
  // `typed`: the text comes from key presses (not a paste or a committed composition): only typed text is corrected and gets curly quotes
  // `smart`: a cut or a paste: with the preference on, the engine tidies the spaces around it in the same step (adjustSpacing)
  const replace = (text: string, deletion?: -1 | 1, fragment?:TextFragment, unit: 'character'|'word'|'line'='character', props?:TextProperties, typed = true, smart = false) => enqueue(async () => {
    const s = session.current;
    if (!s || !opts.current.editable) return;
    let lo = Math.min(s.at, s.anchor), hi = Math.max(s.at, s.anchor);
    if (lo === hi && deletion) {
      const caret=unit==='line' ? (await textSelection(s.story,s.anchor,s.at,s.affinity)).caret : null;
      const at=unit==='line' ? (deletion<0 ? caret?.lineStart??lo : caret?.lineEnd??hi)
        : unit==='word' ? wordBoundary(s.text,s.at,deletion) : boundary(s.text,s.at,deletion);
      if (deletion < 0) lo = at; else hi = at;
    }
    const commands: EditCommand[] = [];
    // Typing rules (Edit ▸ Spelling ▸ Autocorrect, typographer's quotes): the engine says whether typing this text changes the word it finishes or a
    // quote (typeText does it, in one step; a corrected word and the character that finished it are two steps, as in InDesign)
    let caretAt = lo + count(text);
    const typing = typed && lo === hi && !deletion && !fragment && !props && !!text;
    const rule = typing && typedAutoCorrect.current ? await typedAutoCorrect.current(s.story, lo, text, typedRun.current).catch(() => null) : null;
    if (typing) typedRun.current = rule ? rule.run : 0;
    else if (lo === hi && deletion && deletion < 0) typedRun.current = Math.max(0, typedRun.current - 1);
    else typedRun.current = 0;
    const corrected = rule?.fix ?? null;
    let followUp: NonNullable<typeof corrected>['split'] | null = null;
    if (corrected?.split?.typedFirst) {
      // Enter: the paragraph break goes in first and the correction after it (InDesign: the first undo takes the correction back, the break stays)
      commands.push({ op: 'insertText', story: s.story, at: lo, text });
      caretAt = lo + count(text);
      followUp = corrected.split;
    } else if (corrected?.split) {
      // the correction is its own undo step; the typed character goes in as the next one (through the common path below)
      const sp = corrected.split, first = cursorState(s), wordEnd = sp.start + count(sp.word);
      const fixed = await opts.current.edit(tr('자동 고침'), [{ op: 'deleteText', story: s.story, start: sp.start, end: sp.end }, { op: 'insertText', story: s.story, at: sp.start, text: sp.word }]);
      if (!fixed) return;
      knownRev.current = fixed.rev;
      s.at = s.anchor = wordEnd; s.affinity = 'downstream';
      selectionHistory.current.push({ before: first, after: cursorState(s) });
      commands.push({ op: 'insertText', story: s.story, at: wordEnd, text: sp.typed });
      caretAt = corrected.caretAt;
    } else if (corrected) {
      commands.push(corrected.command);
      caretAt = corrected.caretAt;
    } else {
      if (lo !== hi) commands.push({ op: 'deleteText', story: s.story, start: lo, end: hi });
      if (text) commands.push(fragment ? {op:'pasteText',story:s.story,at:lo,fragment} : { op: 'insertText', story: s.story, at: lo, text });
      if (smart && opts.current.smartSpacing !== false && (text || lo !== hi)) {
        commands.push({ op: 'adjustSpacing', story: s.story, start: lo, end: lo + count(text) });
        // the caret follows the space the engine puts before a pasted word, or the space it takes away before the cut (text/smart_spacing.rs)
        const chars = Array.from(s.text), word = (c?: string) => !!c && /[\p{L}\p{N}_]/u.test(c);
        const before = chars[lo - 1], after = chars[hi];
        if (text && word(before) && word(Array.from(text)[0])) caretAt += 1;
        else if (!text && before === ' ' && after !== ' ' && (after === undefined || after === String.fromCharCode(0x2029) || /[.,;:!?)\]}…。、\r\n]/.test(after))) caretAt -= 1;
      }
    }
    if (text && props) commands.push({op:'textFormat',story:s.story,start:lo,end:lo+count(text),props});
    if (!commands.length) return;
    const before = cursorState(s);
    const r = await opts.current.edit(text ? tr("글 입력") : tr("글 삭제"), commands);
    if (!r) return;
    knownRev.current = r.rev;
    desiredX.current=undefined;
    s.at = s.anchor = caretAt; s.affinity = 'downstream';
    if (followUp) {
      const sp = followUp, mid = cursorState(s);
      const fixed = await opts.current.edit(tr('자동 고침'), [{ op: 'deleteText', story: s.story, start: sp.start, end: sp.end }, { op: 'insertText', story: s.story, at: sp.start, text: sp.word }]);
      if (fixed) { knownRev.current = fixed.rev; s.at = s.anchor = corrected!.caretAt; selectionHistory.current.push({ before, after: mid }, { before: mid, after: cursorState(s) }); undone.current = []; }
      await refresh();
      return;
    }
    await refresh();
    selectionHistory.current.push({ before, after: cursorState(s) }); undone.current = [];
  });
  // One special character (a marker, a space, a break…) in place of the selection; the caret
  // goes after it, as after typing.
  const insertSpecial = (special: string) => enqueue(async () => {
    const s = session.current;
    if (!s || !opts.current.editable) return;
    const lo = Math.min(s.at, s.anchor), hi = Math.max(s.at, s.anchor);
    const commands: EditCommand[] = [];
    if (lo !== hi) commands.push({ op: 'deleteText', story: s.story, start: lo, end: hi });
    commands.push({ op: 'insertSpecial', story: s.story, at: lo, special });
    const before = cursorState(s);
    const r = await opts.current.edit(tr('특수 문자 삽입'), commands);
    if (!r) return;
    knownRev.current = r.rev;
    desiredX.current = undefined;
    s.at = s.anchor = lo + 1; s.affinity = 'downstream';
    await refresh(); focus();
    selectionHistory.current.push({ before, after: cursorState(s) }); undone.current = [];
  });
  // Glyphs panel: one glyph by id in place of the selection (the engine stores a glyph without a Unicode value as U+FFFD + CustomGlyph); the caret goes
  // after it, as after typing.
  const insertGlyphById = (family: string, style: string, glyph: number) => enqueue(async () => {
    const s = session.current;
    if (!s || !opts.current.editable) return;
    const lo = Math.min(s.at, s.anchor), hi = Math.max(s.at, s.anchor);
    const commands: EditCommand[] = [];
    if (lo !== hi) commands.push({ op: 'deleteText', story: s.story, start: lo, end: hi });
    commands.push({ op: 'insertGlyph', story: s.story, at: lo, family, style, glyph });
    const before = cursorState(s);
    const r = await opts.current.edit(tr('글리프 삽입'), commands);
    if (!r) return;
    knownRev.current = r.rev;
    desiredX.current = undefined;
    s.at = s.anchor = lo + 1; s.affinity = 'downstream';
    await refresh(); focus();
    selectionHistory.current.push({ before, after: cursorState(s) }); undone.current = [];
  });
  // Type ▸ Text Variables ▸ Convert Variable to Text: the instances in the selection become the text they show; the cursor stays where the
  // selection began.
  const convertVariables = () => enqueue(async () => {
    const s = session.current;
    if (!s || !opts.current.editable) return;
    const lo = Math.min(s.at, s.anchor), hi = Math.max(s.at, s.anchor);
    const before = cursorState(s);
    const r = await opts.current.edit(tr('변수를 텍스트로 변환'), [{ op: 'convertVariableToText', story: s.story, start: lo, end: hi }]);
    if (!r) return;
    knownRev.current = r.rev;
    desiredX.current = undefined;
    s.at = s.anchor = lo; s.affinity = 'downstream';
    await refresh(); focus();
    selectionHistory.current.push({ before, after: cursorState(s) }); undone.current = [];
  });
  // A text variable (its value for the page it is on shows in the text), in place of the selection.
  const insertVariable = (variable: string) => enqueue(async () => {
    const s = session.current;
    if (!s || !opts.current.editable) return;
    const lo = Math.min(s.at, s.anchor), hi = Math.max(s.at, s.anchor);
    const commands: EditCommand[] = [];
    if (lo !== hi) commands.push({ op: 'deleteText', story: s.story, start: lo, end: hi });
    commands.push({ op: 'insertVariable', story: s.story, at: lo, variable });
    const before = cursorState(s);
    const r = await opts.current.edit(tr('텍스트 변수 삽입'), commands);
    if (!r) return;
    knownRev.current = r.rev;
    desiredX.current = undefined;
    s.at = s.anchor = lo + 1; s.affinity = 'downstream';
    await refresh(); focus();
    selectionHistory.current.push({ before, after: cursorState(s) }); undone.current = [];
  });
  const insertFootnote = () => enqueue(async () => {
    const s = session.current;
    if (!s || s.nested || s.at !== s.anchor || composing.current) return;
    const before = cursorState(s);
    const r = await opts.current.edit(tr('각주 삽입'), [{ op:'insertFootnote', story:s.story, at:s.at }]);
    const id = r?.created?.[0];
    if (!r || !id) return;
    knownRev.current = r.rev;
    const info = await textStory(id);
    s.story = id; s.at = s.anchor = info.length; s.affinity = 'downstream';
    desiredX.current = undefined;
    await textCursor(id); await refresh(); focus();
    selectionHistory.current.push({ before, after:cursorState(s) }); undone.current = [];
  });
  const insertEndnote = () => enqueue(async () => {
    const s = session.current;
    if (!s || s.nested || composing.current) return;
    const before = cursorState(s), end = Math.max(s.at, s.anchor);
    const r = await opts.current.edit(tr('미주 삽입'), [{ op:'insertEndnote', story:s.story, at:end }]);
    if (!r) return;
    knownRev.current = r.rev;
    // the endnote body is not editable here yet: the cursor stays in the text, after the reference
    s.at = s.anchor = end + 1; s.affinity = 'downstream';
    desiredX.current = undefined;
    await textCursor(s.story); await refresh(); focus();
    selectionHistory.current.push({ before, after:cursorState(s) }); undone.current = [];
  });
  const goToFootnote = (endnote = false) => enqueue(async () => {
    const s = session.current;
    if (!s || s.at !== s.anchor || composing.current) return;
    const context = (await textStory(s.story, s.at)).noteContext;
    const id = endnote ? context?.endnoteTarget?.story : context?.footnoteTarget;
    if (!id) return;
    s.story = id; s.at = s.anchor = endnote ? context!.endnoteTarget!.at : 0; s.affinity = 'downstream';
    desiredX.current = undefined;
    await textCursor(id); await refresh(); focus();
  });
  const keyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (composing.current || e.nativeEvent.isComposing || e.keyCode === 229) { e.stopPropagation(); return; }
    if (!session.current && cells.current && graphicCellKey(e)) { e.stopPropagation(); e.preventDefault(); return; }
    if (cells.current && opts.current.onTableKey?.(e.nativeEvent)) { e.stopPropagation(); e.preventDefault(); return; }
    if (!session.current && cells.current) {
      // selected cells: Escape leaves them; other keys go to the commands (Table menu shortcuts)
      if (e.key === 'Escape') { e.stopPropagation(); e.preventDefault(); exit(); }
      return;
    }
    const inCell = cells.current?.cursor && cells.current.cursor === session.current?.story;
    if (inCell && e.key === 'Tab' && !e.ctrlKey && !e.altKey && !e.metaKey) {
      e.stopPropagation(); e.preventDefault(); tabCell(e.shiftKey); return;
    }
    if (inCell && e.key === 'Escape') {
      // Escape in a cell's text selects the cell (a second one leaves the table)
      e.stopPropagation(); e.preventDefault();
      enqueue(async () => {
        const c = cells.current; if (!c) return;
        session.current = null; setGeometry({ caret: null, rects: [] }); publish();
        await selectCells({ ...c, cursor: undefined });
      });
      return;
    }
    const intent=textKey(e);
    // Ctrl/⌘ chords the story does not use (save, close, export, zoom…) go on to the commands
    // (and the function keys: Find/Change's Shift+F1..F3, the context menu's Shift+F10; tools-c1)
    if (!intent) { if (!commandKey(e) && !e.altKey && !/^F([1-9]|1[0-2])$/.test(e.code)) e.stopPropagation(); return; }
    e.stopPropagation();
    e.preventDefault();
    if (intent.action==='history') { history(intent.redo); return; }
    if (intent.action==='selectAll') {
      e.preventDefault(); enqueue(async () => { const s=session.current; if (s) {s.anchor=0;s.at=count(s.text);await textCursor(s.story);await refresh();} }); return;
    }
    if (intent.action==='selectLine') {
      enqueue(async () => {
        const s=session.current; if (!s) return;
        const g=await textSelection(s.story,s.at,s.at);
        if (g.caret) { s.anchor=g.caret.lineStart; s.at=g.caret.lineSelectionEnd; s.affinity='upstream'; }
        desiredX.current=undefined;
        await textCursor(s.story); await refresh();
      });
      return;
    }
    if (intent.action==='exit') { exit(); return; }
    if (intent.action==='delete') { replace('', intent.direction, undefined, intent.unit); return; }
    if (intent.action==='insert') { replace(intent.text); return; }
    if (intent.action==='move') {
      enqueue(async () => {
        const s = session.current; if (!s) return;
        const g = await textSelection(s.story, s.anchor, s.at, s.affinity);
        const { unit, direction:dir, extend:shiftKey } = g.vertical ? verticalMove(intent.move) : intent.move;
        if (unit==='line' || unit==='story') {
          s.at = unit==='story' ? (dir<0 ? 0 : count(s.text)) : (dir<0 ? g.caret?.lineStart ?? 0 : g.caret?.lineEnd ?? count(s.text));
          s.affinity = dir>0 ? 'upstream' : 'downstream';
        } else if (unit==='paragraph') {
          // the start of the paragraph before the caret (the one before that when already at a start), or of the next paragraph
          const chars=Array.from(s.text), starts=[0];
          chars.forEach((c,i)=>{ if (c==='\r') starts.push(i+1); });
          const here=starts.filter(p=>p<=s.at).at(-1) ?? 0;
          s.at = dir<0 ? (s.at>here ? here : starts.filter(p=>p<s.at).at(-1) ?? 0) : starts.find(p=>p>s.at) ?? chars.length;
          s.affinity='downstream';
        } else if (unit==='vertical') {
          const n=await textNavigate(s.story,s.at,dir<0?'up':'down',s.affinity,desiredX.current);
          s.at=n.at;s.affinity=n.affinity;desiredX.current=n.x;
        } else {
          const next=unit==='word' ? wordBoundary(s.text,s.at,dir) : boundary(s.text,s.at,dir);
          s.at = !shiftKey && s.at !== s.anchor ? (dir < 0 ? Math.min(s.at,s.anchor) : Math.max(s.at,s.anchor)) : next;
          s.affinity = 'downstream';
        }
        if(unit!=='vertical') desiredX.current=undefined;
        if (!shiftKey) s.anchor = s.at;
        await textCursor(s.story);
        await refresh();
      });
    }
  };
  const copy=(e:React.ClipboardEvent<HTMLTextAreaElement>,cut:boolean)=>{
    e.preventDefault();const s=session.current,cp=copied.current;
    if(!s || composing.current || s.at===s.anchor) return;
    if(!cp || cp.key!==`${s.story}:${s.anchor}:${s.at}:${knownRev.current}`){opts.current.note(tr("표·고정 개체를 제외한 일반 글을 선택하세요."));return;}
    const token=crypto.randomUUID();clipboard={token,document:opts.current.document,fragment:cp.fragment};
    e.clipboardData.setData('text/plain',cp.fragment.text.replace(/\r/g,'\n'));
    e.clipboardData.setData('application/x-folio-text',token);
    if(cut)replace('',undefined,undefined,'character',undefined,false,true);
  };
  const input = (
    <textarea ref={textarea} aria-label={tr("기사 글 입력")} data-text-editor="true"
      data-story={state?.story} data-text-at={state?.at} data-text-anchor={state?.anchor}
      data-table={cellState?.table} data-cells={cellState ? `${cellState.from.join(':')}-${cellState.to.join(':')}` : undefined}
      autoCapitalize="off" autoCorrect="off" spellCheck={false}
      style={{ position: 'fixed', left: -2, top: 0, width: 1, height: 1, opacity: 0, padding: 0, border: 0, resize: 'none', pointerEvents: 'none' }}
      onKeyDown={keyDown} onKeyUp={(e) => e.stopPropagation()}
      onCompositionStart={() => {
        composing.current = true; suppressCommit.current = null; setPreedit(''); setCompositionCaret(null);
        enqueue(async () => {
          const s=session.current; if (!s) return;
          const at=Math.min(s.anchor,s.at);
          const g=await textSelection(s.story,at,at);
          if (composing.current) setCompositionCaret(g.caret);
        });
      }}
      onCompositionUpdate={(e) => setPreedit(e.data)}
      onCompositionEnd={(e) => {
        composing.current = false; setPreedit(''); setCompositionCaret(null);
        suppressCommit.current = e.data; e.currentTarget.value = '';
        if (e.data) replace(e.data, undefined, undefined, 'character', undefined, false); // a committed composition is no key press: InDesign does not autocorrect it
        const committed=e.data;
        window.setTimeout(() => { if (suppressCommit.current===committed) suppressCommit.current=null; },0);
      }}
      onInput={(e) => {
        const native = e.nativeEvent as InputEvent;
        if (composing.current || native.isComposing) { setPreedit(native.data ?? e.currentTarget.value); return; }
        const value = e.currentTarget.value || native.data || '';
        e.currentTarget.value = '';
        if (suppressCommit.current !== null) {
          const duplicate = value === suppressCommit.current;
          suppressCommit.current = null; if (duplicate) return;
        }
        if (value) replace(value.replace(/\r\n|\n/g, '\r'));
      }}
      onCopy={e=>copy(e,false)} onCut={e=>copy(e,true)}
      onPaste={(e) => {
        e.preventDefault();if(composing.current)return;
        const c=clipboard,token=e.clipboardData.getData('application/x-folio-text');
        const internal=c && c.token===token && c.document===opts.current.document;
        const plain=plainPaste;plainPaste=false;
        if(internal&&!plain)replace(c.fragment.text,undefined,c.fragment,'character',undefined,true,true);
        else replace(internal?c.fragment.text:e.clipboardData.getData('text/plain').replace(/\r\n|\n/g,'\r'), undefined, undefined, 'character', undefined, false, true);
      }}
    />
  );
  const overlay = (page: number, width: number, height: number) => (state || cellState) && (
    <svg className="text-edit-overlay" viewBox={`0 0 ${width} ${height}`} style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', pointerEvents: 'none', overflow: 'visible' }}>
      <style>{'@keyframes folioCaret { 0%,49% {opacity:1} 50%,100% {opacity:0} }'}</style>
      {geometry.rects.filter((r) => r.page === page).map((r,i) => <polygon key={i} points={r.quad.map((p) => p.join(',')).join(' ')} fill="rgba(40,120,235,.3)" />)}
      {cellRects.filter((r) => r.page === page).map((r, i) => <polygon key={`cell${i}`} data-table-cells="true" points={r.quad.map((p) => p.join(',')).join(' ')} fill="rgba(40,120,235,.3)" />)}
      {dropCaret?.page === page && <line data-text-drop="true" x1={dropCaret.quad[0][0]} y1={dropCaret.quad[0][1]} x2={dropCaret.quad[3][0]} y2={dropCaret.quad[3][1]} stroke="#1266d6" strokeWidth={2} strokeDasharray="2 2" vectorEffect="non-scaling-stroke" />}
      {state && (preedit ? compositionCaret ?? geometry.caret : geometry.caret)?.page === page && (() => {
        const c=(preedit ? compositionCaret ?? geometry.caret : geometry.caret)!, q=c.quad, b=c.rect;
        return <g>
          {state.anchor === state.at && !preedit && <line key={`${state.at}-${state.text}`} data-text-caret="true" x1={q[0][0]} y1={q[0][1]} x2={q[3][0]} y2={q[3][1]} stroke="#111" strokeWidth={1} vectorEffect="non-scaling-stroke" style={{ animation: 'folioCaret 1s step-end infinite' }} />}
          {preedit && <foreignObject x={b[0]} y={b[1]} width={Math.max(100, preedit.length * (b[3]-b[1]))} height={(b[3]-b[1])*1.5}>
            <span data-ime-preedit="true" style={{ fontFamily: '"Malgun Gothic", sans-serif', fontSize: Math.max(8,b[3]-b[1]), lineHeight: 1, background: 'white', color: '#111', borderBottom: '1px solid #1266d6' }}>{preedit}</span>
          </foreignObject>}
        </g>;
      })()}
    </svg>
  );
  // Drag and drop text editing (tools-c2): a press inside the selected text picks it up; let go elsewhere, one edit moves it there (Ctrl or Alt:
  // a copy; the engine's deleteText and pasteText, so one Undo). A press that does not move puts the cursor there, as a click does.
  const beginDrag = (page: number, x: number, y: number, frame: string, extend: boolean, clicks: number) => {
    const s = session.current;
    dragging.current = clicks === 1;
    if (!(opts.current.dragDrop && clicks === 1 && !extend && s && s.at !== s.anchor && opts.current.editable)) { textDrag.current = null; point(page, x, y, frame, extend, false, clicks); return; }
    const story = s.story, lo = Math.min(s.at, s.anchor), hi = Math.max(s.at, s.anchor);
    enqueue(async () => {
      const { hit } = await textHit(page, x, y, frame);
      if (hit && hit.story === story && hit.at >= lo && hit.at < hi) { textDrag.current = { story, lo, hi, from: [page, x, y, frame] }; return; }
      textDrag.current = null;
      point(page, x, y, frame, extend, false, clicks);
    });
  };
  const moveDrag = (page: number, x: number, y: number, frame?: string) => {
    const d = textDrag.current;
    if (!d) { point(page, x, y, frame, true, true); return; }
    d.to = [page, x, y, frame];
    enqueue(async () => {
      const { hit } = await textHit(page, x, y, frame);
      setDropCaret(hit ? (await textSelection(hit.story, hit.at, hit.at)).caret : null);
    });
  };
  const endDrag = (how?: { copy: boolean }) => {
    dragging.current = false;
    const d = textDrag.current;
    textDrag.current = null;
    if (!d) return;
    setDropCaret(null);
    if (!how) return;
    if (!d.to) { const [pg, x, y, f] = d.from; point(pg, x, y, f); return; }
    const [pg, x, y, f] = d.to;
    enqueue(async () => {
      const { hit } = await textHit(pg, x, y, f);
      if (!hit || (hit.story === d.story && hit.at >= d.lo && hit.at <= d.hi)) return;
      const { fragment } = await textCopy(d.story, d.lo, d.hi);
      const len = d.hi - d.lo;
      // moving later in the same story: the place shifts by what was taken out before it
      const at = !how.copy && hit.story === d.story && hit.at > d.hi ? hit.at - len : hit.at;
      // with Adjust Spacing on, the spaces at both places are tidied as after a cut and a paste (the later place first, so the earlier indexes hold)
      const smart = opts.current.smartSpacing !== false;
      const paste: EditCommand[] = [{ op: 'pasteText', story: hit.story, at: hit.at, fragment }, ...(smart ? [{ op: 'adjustSpacing' as const, story: hit.story, start: hit.at, end: hit.at + len }] : [])];
      const cut: EditCommand[] = [{ op: 'deleteText', story: d.story, start: d.lo, end: d.hi }, ...(smart ? [{ op: 'adjustSpacing' as const, story: d.story, start: d.lo, end: d.lo }] : [])];
      const commands: EditCommand[] = how.copy ? paste : hit.story === d.story && hit.at > d.hi ? [...paste, ...cut] : [...cut, ...paste.map((c) => c.op === 'pasteText' ? { ...c, at } : c)];
      const before = session.current ? cursorState(session.current) : null;
      const r = await opts.current.edit(how.copy ? tr('글 끌어 놓기(복사)') : tr('글 끌어 놓기'), commands);
      if (!r) return;
      knownRev.current = r.rev;
      const info = await textStory(hit.story);
      // the dropped text is selected: where it landed (spaces put in or taken out around it move it a character or so)
      const chars = Array.from(info.text), piece = Array.from(fragment.text);
      let start = at, best = Infinity;
      for (let i = Math.max(0, at - 3); i <= Math.min(chars.length - piece.length, at + 3); i++) {
        if (chars.slice(i, i + piece.length).join('') === fragment.text && Math.abs(i - at) < best) { best = Math.abs(i - at); start = i; }
      }
      session.current = { ...hit, at: start + len, anchor: start, text: info.text, affinity: 'downstream' };
      opts.current.selectFrame(hit.frame, hit.page);
      await refresh(); focus();
      if (before) { selectionHistory.current.push({ before, after: cursorState(session.current) }); undone.current = []; }
    });
  };
  return { active: !!state || !!cellState, point, input, overlay, dragging,
    begin: beginDrag,
    move: moveDrag,
    end: endDrag, exit };
}
