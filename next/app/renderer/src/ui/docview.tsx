//! Composes the document window, canvas, overlays, rulers and status controls.

import { t as tr } from '../i18n';
import {RegionOverlay} from '../ai/RegionOverlay';
import { useState } from 'react';
import { type Bitmap, type DocumentSummary, type EditCommand, type EditResult, type InkView, type PageInfo, type PreflightReport } from '../engine';
import { type DocumentInfo, type ItemStore, trim } from '../inspect';
import type { Target } from '../print';
import { Icon } from './icons';
import { NumberField } from './controls/numberfield';
import { type LayerLook, type SelEntry } from './overlay';
import { useStyleOverrideMarks } from '../features/view/styleoverrides';
import { useDynamicSpellingMarks } from '../features/spelling/dynamic-underline';
import type { SpellOptions, UserDictionaries } from '../features/spelling/spelling';
import { screenRegion } from '../features/view/screen';
import { type Cursor, cursor } from './store';
import type { ScreenMode, ToolId } from './tools';
import { type TableActions, type TableSelection, type TextActions, type TextSelectionState } from './textedit';
import { DrawingPreview } from './drawing';
import { DrawingSizeDialog } from './drawing-dialogs';
import { type Placement } from './place';
import { type Threading } from './threading';
import { type GuideSelection } from './guides';
import { ConveyorBar } from './conveyorbar';
import { zeroOf } from './zeropoint';
import { type SmartPrefs } from './smartguides';
import { ThreadLines } from './threadlines';
import { PageCanvas } from './docview-canvas';
import { useDocumentInput } from './docview-input';
import { Rulers, useRulerCorner } from './docview-rulers';
import { usePageSelection } from './docview-selection';
import { GAP, PAD_X, PAD_Y, type Viewport } from './docview-viewport';

export { queue } from './docview-canvas';
export { ZOOM_STOPS, groupRows, stepZoom, useViewport, type Viewport, type Zoom } from './docview-viewport';

/** The tabs of a window's strip when the document has several windows (Window > Arrange > New Window): the windows docked as tabs. */
export type WindowTabs = { items: { id: number; name: string; dirty: boolean }[]; select: (id: number) => void; close: (id: number) => void };

export type DocWindowProps = {
  strokeFocus: boolean;
  /** T on with text frames selected: the Gradient tool's drag goes to the characters of their stories (toolboxpaint.ts). */
  toolboxText?: import('./toolboxpaint').ToolboxText | null;
  /** Which window of the document this is, when it has several (shown after the name: `file.folio:2`). */
  winNo?: number;
  /** The strip's tabs when there are several windows; null: no strip (the second half of a split window). */
  tabs?: WindowTabs | null;
  placement: Placement;
  threading: Threading;
  doc: DocumentSummary;
  readOnly: boolean;
  rows: PageInfo[][];
  vp: Viewport;
  tool: ToolId;
  ink: InkView | null;
  onPainted: (b: Bitmap) => void;
  /** Ink under the cursor (separations panel). */
  onInk?: (page: number, x: number, y: number) => void;
  onInkLeave?: () => void;
  target: Target | null;
  store: ItemStore;
  info: DocumentInfo | null;
  layers: LayerLook;
  selection: SelEntry[];
  setSelection: (s: SelEntry[]) => void;
  selectedPage: number | null;
  setSelectedPage: (p: number | null) => void;
  /** Page tool: a page dragged by a distance in points (`movePage`: with Objects Move with Page the page's objects go along). */
  onPageMove?: (page: number, dx: number, dy: number) => void;
  current: number;
  setCurrent: (p: number) => void;
  screenMode: ScreenMode;
  guides: boolean;
  edges: boolean;
  /** Type ▸ Show Hidden Characters. */
  hiddenChars?: boolean;
  /** View ▸ Extras ▸ Show Notes. */
  showNotes?: boolean;
  /** The comments of the PDF Comments panel are drawn on the layout (its eye button; off in Preview). */
  pdfComments?: boolean;
  /** The comment picked in the PDF Comments panel (its blue frame on the layout). */
  pdfCommentSelected?: string | null;
  styleOverrideHighlighter?: boolean;
  /** Edit ▸ Spelling ▸ Dynamic Spelling: draw a squiggle under the words that are not in the dictionary (screen only). */
  dynamicSpelling?: { enabled: boolean; options: SpellOptions; userDictionaries: UserDictionaries; color: unknown };
  contentGrabber?: boolean;
  tripleClickSelectsLine?: boolean;
  /** Preferences ▸ Type ▸ Adjust Spacing Automatically When Cutting and Pasting Words (on unless false). */
  smartCutAndPaste?: boolean;
  /** Preferences ▸ Type ▸ Enable in Layout View (Drag and Drop Text Editing; off unless true). */
  dragDropText?: boolean;
  showContentGrabber?: boolean;
  /** The Pencil Tool Preferences (user settings; a double click on the Pencil tool opens them). */
  pencilPrefs?: import('./pencilprefs').PencilPrefs;
  marks?: import('../features/view/marks').ViewMarks | null;
  /** The Alt drag of the anchored object control opens the Anchored Object Options dialog on the object it anchored. */
  onAnchorOptions?: (id: string) => void;
  /** View ▸ Extras ▸ Show Link Badge: the link states to mark (absent: off), and what clicking a badge does. */
  linkBadges?: import('./overlay').BadgeLinks | null;
  onLinkBadge?: (owner: string) => void;
  highlightObjectUnderSelectionTool?: boolean;
  /** View ▸ Show Hidden Spreads (this computer's view setting, not the document's): hidden spreads are laid out, drawn dimmed. */
  showHiddenSpreads?: boolean;
  /** View ▸ Extras ▸ Show Text Threads. */
  threads?: boolean;
  rulers: boolean;
  /** The Control panel's reference point: a turn dragged from outside a handle goes about it. */
  refPoint?: [number, number];
  units: { horizontal: string; vertical: string };
  rulerOrigin: string;
  /** View ▸ Grids & Guides ▸ Smart Guides and its four options (Preferences ▸ Guides & Pasteboard): dragged and resized objects are pulled
   *  onto other objects' edges and middles, the page's middle, equal gaps and equal sizes, and lines are drawn (ui/smartguides.ts). */
  smart?: SmartPrefs;
  preflight: PreflightReport | null;
  preflightBusy: boolean;
  onPreflight: () => void;
  onNote: (text: string) => void;
  onClose: () => void;
  onDoubleClickText: () => void;
  onExitText: () => void;
  textActions: React.MutableRefObject<TextActions | null>;
  textReveal?:{story:string;start:number;end:number;allowOverset?:boolean}|null;
  onTextState: (state:TextSelectionState|null)=>void;
  onNoteAt: (story:string,at:number)=>void;
  /** Selected table cells (type tool in a table). */
  onTableState?: (state: TableSelection | null) => void;
  tableActions?: React.MutableRefObject<TableActions | null>;
  onTableKey?: (event: KeyboardEvent) => boolean;
  onPathKey?: (event: KeyboardEvent) => boolean;
  pathFinish?: React.MutableRefObject<(() => void) | null>;
  onTextHistory: (redo: boolean) => Promise<EditResult | null>;
  /** Right side of the status line (notes, read-only state, missing fonts). */
  statusExtra?: React.ReactNode;
  /** Right-click on the pasteboard: what is under the cursor, after selecting it. */
  onContextMenu?: (at: { x: number; y: number }, kind: 'layoutNone' | 'layoutObject' | 'layoutGraphic' | 'text' | 'guide' | 'rulerH' | 'rulerV' | 'zeroPoint') => void;
  /** Ruler corner ▸ Lock Zero Point (an application setting): the corner then neither moves nor resets the zero point. */
  zeroPointLocked?: boolean;
  onGuideSelection?: (selection: GuideSelection) => void;
  /** The document can be changed (a .folio this window holds). */
  editable: boolean;
  /** Changed since the last save: the tab shows `*`, as InDesign's does. */
  dirty: boolean;
  /** Per page: bumped when an edit changed it, so it is drawn again. */
  aiMarks?: Target[];
  pageRev: Map<number, number>;
  onEdit: (label: string, commands: EditCommand[], extra?: Record<string, unknown>) => Promise<EditResult | null>;
  /** View ▸ Extras ▸ Show Conveyor: the bar of the Content Collector / Placer tools. */
  showConveyor?: boolean;
};

const documentInputText = {
  unavailableTool: () => tr('ui.a6399943a3'),
  mixedSpreads: () => tr('ui.fa17f69476'),
  duplicate: () => tr('ui.ec3a6b612c'),
  moveToSpread: () => tr('ui.58ad690294'),
  move: () => tr('ui.c686d05434'),
  moveContent: () => tr('ui.bc6deb4a64'),
  resize: () => tr('ui.3ae13dba87'),
};

const rulerCornerTitle = (unit: string) => tr('ui.60be59a141', {0: unit});

export function DocWindow(p: DocWindowProps) {
  const { vp, rows, doc } = p;
  const overrideMarks=useStyleOverrideMarks(!!p.styleOverrideHighlighter&&p.editable&&p.screenMode==='normal',doc.path,p.pageRev);
  const scale = vp.scale;
  const spellMarks = useDynamicSpellingMarks({ enabled: !!p.dynamicSpelling?.enabled && p.screenMode === 'normal', document: doc.path, revision: p.pageRev,
    options: p.dynamicSpelling?.options ?? { checkMisspelledWords: true, checkRepeatedWords: true, checkCapitalizedWords: true, checkCapitalizedSentences: true },
    userDictionaries: p.dynamicSpelling?.userDictionaries ?? {}, color: p.dynamicSpelling?.color });
  const {
    text, hover, marquee, zoomBox, drawing, drawingSize, setDrawingSize, createDrawing, measure,
    placeLoadedCursor, ghost, smartDraw, sides, guides, paths, live, tpBrackets, tpCursor, pivot, smartCss,
    pageAt, onPointerDown, onPointerMove, onPointerUp, onPointerCancel, onPointerLeave, onDoubleClick, onContextMenu,
  } = useDocumentInput(p, documentInputText);
  const selByPage = usePageSelection(p.selection);
  const { onRulerCorner, resetZeroPoint } = useRulerCorner(p, pageAt);

  const normal = p.screenMode === 'normal';
  const zoomPct = `${trim(scale * 100, scale < 0.1 ? 1 : 0)}%`;
  return (
    <section className={p.tabs === null ? 'docwin no-tabs' : 'docwin'} data-pane={p.winNo ?? 1}>
      {drawingSize && <DrawingSizeDialog polygon={drawingSize.kind === 'polygon' || drawingSize.kind === 'polygonFrame'} close={() => setDrawingSize(null)} create={async (width, height) => {
        const { page, kind, x, y, centered } = drawingSize;
        const left = centered ? x - width / 2 : x, top = centered ? y - height / 2 : y;
        await createDrawing(page, kind, [left, top, left + width, top + height]);
      }} />}
      {p.tabs !== null && (
        <div className="doc-tabs">
          {(p.tabs ? p.tabs.items : [null]).map((t) => (t === null || t.id === p.winNo ? (
            <div className="doc-tab on" key={t?.id ?? 'own'} title={doc.path}>
              <span>{fileName(doc.path)}{p.winNo ? `:${p.winNo}` : ''}{doc.masterName ? ` — ${doc.masterName}` : ''}{p.dirty ? '*' : ''} @ {zoomPct}{p.readOnly ? tr('ui.052739ec7b') : ''}</span>
              <button className="doc-tab-x" onClick={p.onClose} title={tr('ui.92b9c29050')}><Icon name="close" size={12} /></button>
            </div>
          ) : (
            <div className="doc-tab" key={t.id} data-win-tab={t.id} title={doc.path} onClick={() => p.tabs?.select(t.id)}>
              <span>{t.name}{t.dirty ? '*' : ''}</span>
              <button className="doc-tab-x" onClick={(e) => { e.stopPropagation(); p.tabs?.close(t.id); }} title={tr('ui.92b9c29050')}><Icon name="close" size={12} /></button>
            </div>
          )))}
        </div>
      )}
      <div className={p.rulers ? 'doc-area with-rulers' : 'doc-area'}>
        {!!p.threading.loaded && <div role="status" className="place-loaded" style={{ position: 'absolute', top: 24, left: 36, zIndex: 8, padding: '5px 10px', background: '#173f64', color: 'white', pointerEvents: 'none' }}>
          {tr('글을 실었습니다 · 텍스트 프레임을 누르면 잇기 · 빈 곳을 누르거나 끌면 새 프레임 · Esc 취소')}
        </div>}
        {!!p.placement.files.length && <div role="status" className="place-loaded" style={{ position: 'absolute', top: 24, left: 36, zIndex: 8, padding: '5px 10px', background: '#173f64', color: 'white', pointerEvents: 'none' }}>
          {tr('가져와 놓기 · {0}개 · {1} · 클릭 또는 끌기 · Esc 취소', { 0: p.placement.files.length, 1: fileName(p.placement.files[0]) })}
        </div>}
        {!!p.placement.files.length && <label className="place-loaded place-options" style={{ position: 'absolute', top: 54, left: 36, zIndex: 8, padding: '3px 10px', background: '#173f64', color: 'white', display: 'flex', alignItems: 'center', gap: 6 }}>
          <input type="checkbox" checked={p.placement.showOptions.show} onChange={(e) => p.placement.showOptions.set(e.target.checked)} />{tr('다음에 놓을 때 가져오기 옵션 표시')}
        </label>}
        {p.rulers && <Rulers cornerTitle={rulerCornerTitle} vp={vp} units={p.units} origin={p.rulerOrigin} zero={zeroOf(p.info)} current={p.current} onDown={normal ? guides.fromRuler : undefined} onDouble={normal ? guides.fromRulerDouble : undefined} onCorner={normal ? onRulerCorner : undefined} onCornerDouble={normal ? resetZeroPoint : undefined} onMenu={p.onContextMenu && ((e, kind) => { e.preventDefault(); p.onContextMenu!({ x: e.clientX, y: e.clientY }, kind); })} />}
        {guides.ghost}
        {text.input}
        <div
          className={`view tool-${p.tool}${p.tool === 'typePath' && tpCursor !== 'free' ? ` tp-${tpCursor}` : ''}${paths.cursor !== 'none' ? ` pen-cur-${paths.cursor}` : ''}${normal ? '' : ' preview'}`}
          ref={vp.setView}
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerCancel}
          onPointerLeave={onPointerLeave}
          onDoubleClick={onDoubleClick}
          onContextMenu={onContextMenu}
          style={{
            position: 'relative', padding: `${PAD_Y}px ${PAD_X}px`, gap: GAP, ...(p.placement.files.length || p.threading.loaded || placeLoadedCursor ? { cursor: 'crosshair' } : {}),
            // Preferences > Guides & Pasteboard: the Preview pasteboard has its own colour unless it matches the theme
            ...(!normal && p.info?.view && !p.info.view.pasteboard.matchPreviewBackground ? { background: `rgb(${p.info.view.pasteboard.previewColor.join(',')})` } : {}),
          }}
        >
          {normal && p.threads && <ThreadLines view={vp.viewEl} selection={p.selection} store={p.store} pages={doc.pages.length} scale={scale} layers={p.layers} revision={p.pageRev} />}
          {rows.map((row, i) => {
            // View ▸ Rotate Spread: the row is drawn turned, and takes the room of its turned shape; the pointer tools
            // do not reach into a turned spread yet (they measure the pages unturned)
            const turn = doc.pageSettings?.rotation?.[row[0].spread] ?? 0;
            const rw = row.reduce((a, pg) => a + Math.round(pg.width * scale), 0), rh = Math.max(...row.map((pg) => Math.round(pg.height * scale)));
            const shown = !!p.showHiddenSpreads && !!doc.pageSettings?.hidden?.[row[0].spread];
            return (
            <div className="spread" key={i} data-row-of={row[0].index} data-turn={turn || undefined} data-hidden-spread={shown || undefined}
              style={turn || shown ? { ...(turn ? { transform: `rotate(${turn}deg)`, pointerEvents: 'none' as const, ...(turn === 90 || turn === 270 ? { margin: `${(rw - rh) / 2}px ${(rh - rw) / 2}px` } : {}) } : {}), ...(shown ? { opacity: 0.5 } : {}) } : undefined}>
              {row.map((pg, j) => (
                <PageCanvas
                  key={pg.index}
                  page={pg}
                  region={screenRegion(p.screenMode, pg, doc, sides.get(pg.index) ?? (row.length > 1 ? (j === 0 ? 'left' : 'right') : undefined), j === 0, j === row.length - 1)}
                  scale={scale}
                  view={vp.viewEl}
                  ink={p.ink}
                  onPainted={p.onPainted}
                  highlight={p.target?.page === pg.index ? p.target : null}
                  store={p.store}
                  layers={p.layers}
                  side={sides.get(pg.index) ?? (row.length > 1 ? (j === 0 ? 'left' : 'right') : undefined)}
                  outerLeft={j === 0}
                  outerRight={j === row.length - 1}
                  bleed={doc.bleed ?? null}
                  grids={normal ? p.info?.view ?? null : null}
                  guides={normal && p.guides}
                  edges={normal && p.edges}
                  hiddenChars={normal && !!p.hiddenChars}
                  showNotes={normal && p.showNotes !== false}
                  pdfComments={normal && !!p.pdfComments}
                  pdfCommentSelected={p.pdfCommentSelected ?? null}
                  contentGrabber={p.contentGrabber !== false}
                  selection={text.active ? NONE : selByPage.get(pg.index) ?? NONE}
                  hover={p.highlightObjectUnderSelectionTool !== false && hover?.page === pg.index ? hover.id : null}
                  showContentGrabber={p.showContentGrabber !== false}
                  marks={p.marks ?? null}
                  liveEdit={live.liveFor(pg.index)}
                  brackets={tpBrackets.view(pg.index)}
                  linkBadges={normal ? p.linkBadges ?? null : null}
                  marquee={marquee?.page === pg.index ? marquee.rect : null}
                  measure={measure?.page === pg.index ? [measure.x0, measure.y0, measure.x1, measure.y1, ...(measure.x2 !== undefined && measure.y2 !== undefined ? [measure.x2, measure.y2] : [])] : null}
                  pageSelected={p.selectedPage === pg.index}
                  ghost={ghost && ghost.pages.includes(pg.index) ? ghost.g : null}
                  smartDraw={smartDraw && smartDraw.page === pg.index ? smartDraw.draw : null}
                  smartColor={smartCss}
                  pivot={pivot?.page === pg.index ? pivot : null}
                  guide={guides.selection.length ? guides.selection.map((g) => g.id).join(',') : null}
                  rev={p.pageRev.get(pg.index) ?? 0}
                  textOverlay={<>{overrideMarks(pg.index,pg.width,pg.height)}{spellMarks(pg.index,pg.width,pg.height,scale)}{text.overlay(pg.index, pg.width, pg.height)}<RegionOverlay key={p.tool==='aiRegion'?'drawing':'view'} page={pg.index} width={pg.width} height={pg.height} active={p.tool==='aiRegion'} targets={p.aiMarks??[]}/></>}
                  drawingOverlay={drawing?.page === pg.index ? <DrawingPreview drawing={drawing} width={pg.width} height={pg.height} turn={turn} /> : null}
                  pathOverlay={paths.overlay(pg.index, pg.width, pg.height)}
                />
              ))}
            </div>
            );
          })}
        </div>
        <ConveyorBar tool={p.tool} shown={!!p.showConveyor} />
        {zoomBox && <div className="zoom-box" style={{ left: Math.min(zoomBox[0], zoomBox[2]), top: Math.min(zoomBox[1], zoomBox[3]), width: Math.abs(zoomBox[2] - zoomBox[0]), height: Math.abs(zoomBox[3] - zoomBox[1]) }} />}
      </div>
      <DocStatus {...p} zoom={scale * 100} />
    </section>
  );
}

const NONE: SelEntry[] = [];

/** The zoom list of the status bar (percent), the largest first as the old menu listed them. */
const ZOOM_LEVELS = [4000, 1600, 800, 400, 200, 150, 125, 100, 75, 50, 25, 12.5, 5];

function DocStatus(p: DocWindowProps & { zoom: number }) {
  const { doc, vp } = p;
  const [pageText, setPageText] = useState<string | null>(null);
  const goTo = (text: string) => {
    const byName = doc.pages.find((q) => q.name === text.trim());
    const n = Number(text);
    const t = byName ?? (Number.isFinite(n) ? doc.pages[Math.min(doc.pages.length, Math.max(1, n)) - 1] : undefined);
    if (t) vp.scrollToPage(t.index);
  };
  // a master spread opened as a document numbers its pages from 1,000,000: step through the list, not the numbers
  const pos = Math.max(0, doc.pages.findIndex((q) => q.index === p.current));
  const cur = doc.pages[pos];
  const step = (d: number) => {
    const to = doc.pages[Math.max(0, Math.min(doc.pages.length - 1, pos + d))];
    if (to) vp.scrollToPage(to.index);
  };
  const pf = p.preflight;
  const errors = pf?.summary.errors ?? 0;
  return (
    <footer className="doc-status">
      <div className="zoom-box-field">
        <NumberField value={p.zoom} label={tr('확대 비율')} min={3} max={4000} step={5} width={78} spin={false}
          format={(v) => `${trim(v, v < 10 ? 1 : 0)}%`} suffix="%" options={ZOOM_LEVELS}
          parse={(t) => { const v = parseFloat(t); return v > 0 ? Math.min(4000, Math.max(3, v)) : null; }}
          onCommit={(v) => vp.zoomTo(v / 100)} />
      </div>
      <div className="page-nav">
        <button onClick={() => vp.scrollToPage(doc.pages[0]?.index ?? 0)} title={tr('ui.33b69abf44')}><Icon name="first" size={14} /></button>
        <button onClick={() => step(-1)} title={tr('ui.b5f6e8aed4')}><Icon name="chevronLeft" size={14} /></button>
        <input
          aria-label={tr('ui.5123763a37')}
          value={pageText ?? cur?.name ?? ''}
          onFocus={(e) => { setPageText(cur?.name ?? ''); e.currentTarget.select(); }}
          onChange={(e) => setPageText(e.target.value)}
          onBlur={() => setPageText(null)}
          onKeyDown={(e) => { if (e.key === 'Enter') { goTo(e.currentTarget.value); e.currentTarget.blur(); } if (e.key === 'Escape') e.currentTarget.blur(); }}
        />
        <button onClick={() => step(1)} title={tr('ui.b2aa104e6e')}><Icon name="chevronRight" size={14} /></button>
        <button onClick={() => vp.scrollToPage(doc.pages[doc.pages.length - 1].index)} title={tr('ui.37d4490a14')}><Icon name="last" size={14} /></button>
      </div>
      <button className={`pf-state${p.preflightBusy ? ' busy' : errors ? ' error' : pf ? ' ok' : ''}`} onClick={p.onPreflight} title={tr('ui.3cb799e570')}>
        <i />
        {p.preflightBusy ? tr('ui.72ea7814e5') : !pf ? tr('ui.4814fa3d59') : errors ? tr('ui.aacb8741e2', {0: errors}) : tr('ui.b1701fa204')}
      </button>
      <span className="doc-status-fill" />
      {p.statusExtra}
    </footer>
  );
}

export const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p;

export type { Cursor };
