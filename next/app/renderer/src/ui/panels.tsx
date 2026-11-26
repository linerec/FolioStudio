import { ramp } from '../features/color/gradient';
import { TintFields } from '../features/color/tint';
import { mixedGroupName } from '../features/color/mixedinkgroup';
import { t as tr } from '../i18n';
import { actionLabel } from '../i18n';
import { TransformFields } from './control';
import { EffectsPanel as Effects, blendTargets } from './effects';
import { StrokePanel as Stroke, strokeTargets } from './stroke';
// Panel contents, laid out like InDesign's panels. Pages, Layers, Links, Swatches, styles and
// the readouts (Stroke, Color, Info, Transform, Effects) show the document; controls that
// change it are in place but wait for the editing engine, disabled.
import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { declarePanelCommands, usePanelCommands } from './panelbus';
import { Dropdown } from './menu';
import type { Commands } from './commands';
import { newLayer } from './layeredit';
import { clearLayerDrop, markLayerDrop } from './dropmark';
import { DeleteSwatchDialog, SwatchDialog, editable as swatchEditable } from './swatchedit';
import { type DocumentSummary, type EditCommand, type LinkRow, type LinkStatus, type LinksReport, type Settings } from '../engine';
import { type DocumentInfo, type ItemInfo, type PageItems, apply, decompose, formatLength, pageOf, parseLength, trim } from '../inspect';
import type { Target } from '../print';
import { Icon } from './icons';
import { linkXmpRows, useFileMetadata } from './linkxmp';
import { queue } from './docview';
import { type SelEntry, findItem } from './overlay';
import { cursor, measure, useLive } from './store';
import { AlignButtons, AlignReference, type AlignmentControls } from './alignment';
import { CharacterFields, ParagraphFields, StyleSelect, type TextFormatting } from './textformat';
import { TextStylesPanel } from './textstyles';
import { ObjectStylesPanel } from './objectstyles';
import { TextWrapControls } from './textwrap';
import { NumberField } from './controls/numberfield';
import { lengthSpec } from './controls/lengthfield';
import { usePreviewEdit } from './controls/usePreviewEdit';
import { CHANNELS, edited, nativeMode, rampColor, valuesIn, type ColorMode, type Shown } from '../features/color/convert';
import { rulerOffset, zeroOf } from './zeropoint';

export type PanelCtx = {
  dataMerge?: import('../features/datamerge/state').DataMergeState;
  /** Data Merge ▸ a database as the data source: the status of the document's source and what the panel's buttons do (features/datamerge/db/useDatabaseSource.tsx). */
  databaseSource?: import('../features/datamerge/db/useDatabaseSource').DatabaseActions;
  pdfComments?: import('../features/pdfcomments/state').PdfCommentsState;
  /** Window ▸ Output ▸ Flattener Preview: the panel's state and what its controls do (features/flattener/state.ts). */
  flattener?: import('../features/flattener/state').FlattenerPreviewState;
  /** Window ▸ Math Expressions: what the panel's buttons do (features/math/useMath.tsx). */
  math?: import('../features/math/useMath').MathActions;
  styleOverrideHighlighter?: {enabled:boolean;toggle:()=>void};
  /** The live command table (the style panels' right-click menus draw from it). */
  commands?: () => import('./commands').Commands;
  objectStyleActions?: (actions:import('./objectstylemenu').ObjectStyleAction[])=>void;
  /** Cell Styles and Table Styles panel menus: what the open panel's menu items do (ui/tablestylemenu.ts). */
  tableStyleActions?: (kind:'cell'|'table',actions:import('./tablestylemenu').TableStyleAction[])=>void;
  /** Paragraph Styles and Character Styles panel menus: what the open panel's menu items do (ui/textstylemenu.ts). */
  textStyleActions?: (kind:'paragraph'|'character',actions:import('./textstylemenu').TextStyleAction[])=>void;
  colorMenu?: import('../features/color/menu').ColorMenu;
  /** Color panel menu: the slider mode and Add to Swatches (features/color/panelmenu.ts). */
  colorPanel?: import('../features/color/panelmenu').ColorPanelMenu;
  /** The Color Picker (double-click a Fill or Stroke chip): for the Fill (false) or the Stroke (true). */
  openColorPicker?: (stroke: boolean) => void;
  pageEditing?: import('./pages').PageEditing;
  layerEditing?: import('./layeredit').LayerEditing;
  /** Info panel menu: Show Options, Show Content Offset (ui/infomenu.ts). */
  infoOptions?: import('./infomenu').InfoOptions;
  /** Show Options / Hide Options of the Stroke, Color, Gradient, Align, Text Wrap and Effects panels (ui/paneloptions.ts). */
  panelOptions?: import('./paneloptions').PanelOptions;
  /** The Links panel's selection and menu (ui/linkmenu.ts). */
  linkMenu?: import('./linkmenu').LinkMenu;
  /** The control bar's selection controls (the Transform panel edits with them). */
  control?: import('./control').ControlProps;
  /** Window ▸ History: the undo steps, and going back or forward to one. */
  history?: { undo: string[]; redo: string[]; deletable?: boolean[] };
  historyGo?: (done: number) => Promise<void>;
  /** History panel menu (build-c): a `historyDelete` / `historyClear` request, shown as an edit's reply. */
  historyCall?: (message: Record<string, unknown>) => Promise<import('../engine').EditResult | null>;
  pageRev?: Map<number, number>;
  /** Pages panel: open a master spread like a document (null: back to the document); the one open now. */
  openMaster?: (id: string | null) => void;
  openedMaster?: string | null;
  doc: DocumentSummary | null;
  info: DocumentInfo | null;
  readOnly: boolean;
  store: { peek: (p: number) => PageItems | null | undefined; subscribe: (fn: () => void) => () => void };
  /** The table cells selected (Cell Styles and Table Styles panels), when the document can be edited. */
  tableSelection?: import('./textedit').TableSelection | null;
  selection: SelEntry[];
  setSelection: (s: SelEntry[]) => void;
  current: number;
  selectedPage: number | null;
  setSelectedPage: (p: number | null) => void;
  goToPage: (index: number) => void;
  /** Go to the page and fit it in the window (the PDF Comments panel). */
  fitPage?: (index: number) => void;
  go: (t: Target) => void;
  units: { horizontal: string; vertical: string };
  rulerOrigin: string;
  links: LinksReport | null;
  linksBusy: boolean;
  settings: Settings;
  onAddRoot: () => void;
  onRemoveRoot: (r: string) => void;
  onRelink: () => void;
  spreads: boolean;
  /** The document can be changed (layer and object visibility and lock). */
  editable: boolean;
  edit: (label: string, commands: EditCommand[]) => Promise<import('../engine').EditResult | null>;
  textFormatting:TextFormatting;
  /** The Tools panel's Formatting Affects Container / Text (J): on, a swatch or tint goes to the text of the selected text frames. */
  formatText?: boolean;
  setFormatText?: (on: boolean) => void;
  /** The current tool (Window ▸ Utilities ▸ Tool Hints). */
  tool?: string;
  /** Opens one of InDesign's panel right-click menus (menutree CONTEXT_MENUS) at the pointer: the page icons and the empty area of the Pages panel, a layer row, the Links panel's column header and information pane. */
  openContext?: (kind: 'pagesPages' | 'pagesPanel' | 'pagesSection' | 'layer' | 'linksHeader' | 'linkInfo', at: { x: number; y: number }) => void;
  /** Opens Object ▸ Effects on a page (and a level). */
  openEffects?: (page: import('./effectsdialog').FxPage, level?: import('./effectsdialog').FxLevel) => void;
  /** Shows a place in a story (selects it and scrolls to it): Index panel ▸ Go to Selected Marker. */
  revealText?: (to: { story: string; start: number; end: number }) => void;
  /** Glyphs panel ▸ Load Glyph in Find / Change: puts the glyph in Find/Change ▸ Glyph and opens it. */
  loadGlyph?: (which: 'find' | 'change', glyph: { family: string; style: string; glyphId: number }) => void;
};

function Empty({ text = tr("문서가 열려 있지 않습니다.") }: { text?: string }) {
  return <p className="p-empty">{text}</p>;
}

/** Panel footer; unavailable actions are disabled, including buttons without a handler. */
function Foot({ children, buttons }: { children?: React.ReactNode; buttons?: { icon: string; title: string; onClick?: () => void; disabled?: boolean }[] }) {
  return (
    <div className="p-foot">
      <span className="p-foot-text">{children}</span>
      {buttons?.map((b) => (
        <button key={b.title} className="p-foot-btn" title={b.title} onClick={b.onClick} disabled={b.disabled || !b.onClick}>
          <Icon name={b.icon} size={15} />
        </button>
      ))}
    </div>
  );
}

/** The selected item (first of the selection), resolved. */
export function useSelected(ctx: PanelCtx): { item: ItemInfo; entry: SelEntry } | null {
  const [, tick] = useState(0);
  useEffect(() => ctx.store.subscribe(() => tick((n) => n + 1)), [ctx.store]);
  const e = ctx.selection[0];
  if (!e) return null;
  const f = findItem(ctx.store.peek(e.page), e.id);
  return f ? { item: f.item, entry: e } : null;
}

// ---------------------------------------------------------------------------------------
// Pages

export { PagesPanel } from './pages';

// ---------------------------------------------------------------------------------------
// Layers

export function LayersPanel({ ctx }: { ctx: PanelCtx }) {
  const sel = useSelected(ctx);
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const visibility = ctx.layerEditing?.visibility ?? {layers: ctx.info?.layers, edit: ctx.edit};
  if (!ctx.doc) return <Empty />;
  const layers = visibility.layers;
  const le = ctx.layerEditing;
  if (!layers) return <Empty text={tr("레이어 목록은 새 엔진에서 보입니다.")} />;
  const selLayer = sel?.item.layer ?? null;
  const active = layers.find((l) => l.id === ctx.info?.activeLayer) ?? layers[layers.length - 1];
  // objects that can change layer: the selected spread-level items (group members follow their group)
  const movable = () => ctx.selection.map((s) => ctx.store.peek(s.page)?.items.find((it) => it.id === s.id)).filter((it): it is ItemInfo => !!it && !!it.layer).map((it) => it.id);
  const drop = (e: React.DragEvent, l: (typeof layers)[number]) => {
    if (!ctx.editable) return;
    const moving = e.dataTransfer.getData('folio/layer'), items = e.dataTransfer.getData('folio/items');
    if (moving && moving !== l.id) {
      // the panel lists the top layer first: the upper half of a row means above it
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      ctx.edit(tr('레이어 순서'), [e.clientY < r.top + r.height / 2 ? { op: 'moveLayer', id: moving, above: l.id } : { op: 'moveLayer', id: moving, below: l.id }]);
    } else if (items) {
      const ids = JSON.parse(items) as string[];
      if (ids.length) ctx.edit(tr('개체를 다른 레이어로'), [{ op: 'toLayer', ids, layer: l.id }]);
    }
  };
  const remove = () => { if (active) le?.remove(active.id); };
  const selected = new Set(ctx.selection.map((s) => s.id));
  // items of the current spread, top first, as InDesign lists a layer's objects
  const spread = ctx.doc.pages[ctx.current]?.spread;
  const onSpread = ctx.doc.pages.filter((p) => p.spread === spread).flatMap((p) => (ctx.store.peek(p.index)?.items ?? []).map((it) => ({ page: p.index, it })));
  onSpread.sort((a, b) => b.it.z - a.it.z);
  const row = (page: number, it: ItemInfo, depth: number, color: [number, number, number]): React.ReactNode => (
    <div key={it.id}>
      <div className={selected.has(it.id) ? 'lp-row lp-item on' : 'lp-row lp-item'} onClick={() => ctx.setSelection([{ page, id: it.id, content: false }])} title={it.id}>
        <span
          className={`${it.visible ? 'lp-eye' : 'lp-eye off'}${ctx.editable ? ' click' : ''}`}
          title={tr("표시 전환")}
          onClick={(e) => { e.stopPropagation(); if (ctx.editable) ctx.edit(it.visible ? tr("숨기기") : tr("표시"), [{ op: 'setItem', id: it.id, visible: !it.visible }]); }}
        ><Icon name="eye" size={13} /></span>
        <span
          className={`${it.locked ? 'lp-lock' : 'lp-lock off'}${ctx.editable ? ' click' : ''}`}
          title={tr("잠금 전환")}
          onClick={(e) => { e.stopPropagation(); if (ctx.editable) ctx.edit(it.locked ? tr("잠금 해제") : tr("잠금"), [{ op: 'setItem', id: it.id, locked: !it.locked }]); }}
        >{it.locked ? <Icon name="lock" size={12} /> : null}</span>
        <span className="lp-color" />
        <span className="lp-twisty" style={{ paddingLeft: depth * 10 }}>{it.children.length ? (open[it.id] ? '▾' : '▸') : ''}</span>
        <span className="lp-name" onClick={(e) => { if (it.children.length) { e.stopPropagation(); setOpen((o) => ({ ...o, [it.id]: !o[it.id] })); } }}>{itemLabel(it)}</span>
        <span className="lp-pen" />
        <span className="lp-sel"><i className={selected.has(it.id) ? 'on' : ''} style={{ borderColor: `rgb(${color.join(',')})`, background: selected.has(it.id) ? `rgb(${color.join(',')})` : 'transparent' }} /></span>
      </div>
      {open[it.id] && it.children.map((c) => row(page, c, depth + 1, color))}
    </div>
  );
  return (
    <div className={le?.smallRows ? 'layers-panel small-rows' : 'layers-panel'}>
      <div className="lp-list">
        {layers.map((l) => {
          const items = onSpread.filter((x) => x.it.layer === l.id);
          return (
            <div key={l.id}>
              <div className={`lp-row${l.id === selLayer ? ' on' : ''}${le?.picked.has(l.id) ? ' picked' : ''}`} title={(l.printable ? tr('{0} · 개체 {1}개', {0:l.name,1:l.items}) : tr('{0} · 개체 {1}개 · 인쇄 안 함', {0:l.name,1:l.items}))} data-layer={l.id}
                onContextMenu={(e) => { e.preventDefault(); if (!le?.picked.has(l.id)) le?.pick(l.id, false); ctx.openContext?.('layer', { x: e.clientX, y: e.clientY }); }}
                draggable={ctx.editable} onDragStart={(e) => e.dataTransfer.setData('folio/layer', l.id)}
                onDragOver={(e) => { if (ctx.editable) { e.preventDefault(); markLayerDrop(e); } }} onDragLeave={clearLayerDrop} onDrop={(e) => { e.preventDefault(); clearLayerDrop(e); drop(e, l); }}>
                <span
                  className={`${l.visible ? 'lp-eye' : 'lp-eye off'}${ctx.editable ? ' click' : ''}`}
                  title={tr('레이어 표시 전환 (Alt+클릭: 다른 레이어 모두 숨기기·표시)')}
                  onClick={(e) => {
                    if (!ctx.editable) return;
                    if (e.altKey) {
                      // InDesign: Alt+click shows only this layer; again, every layer
                      const others = layers.filter((o) => o.id !== l.id);
                      const solo = others.some((o) => o.visible) || !l.visible;
                      const cmds = (solo ? [...(l.visible ? [] : [{ op: 'setLayer', id: l.id, visible: true }]), ...others.filter((o) => o.visible).map((o) => ({ op: 'setLayer', id: o.id, visible: false }))]
                        : others.map((o) => ({ op: 'setLayer', id: o.id, visible: true }))) as EditCommand[];
                      if (cmds.length) void visibility.edit(solo ? tr('다른 레이어 숨기기') : tr('모든 레이어 표시'), cmds);
                      return;
                    }
                    void visibility.edit(l.visible ? tr("레이어 숨기기") : tr("레이어 표시"), [{ op: 'setLayer', id: l.id, visible: !l.visible }]);
                  }}
                ><Icon name="eye" size={14} /></span>
                <span
                  className={`${l.locked ? 'lp-lock' : 'lp-lock off'}${ctx.editable ? ' click' : ''}`}
                  title={tr('레이어 잠금 전환 (Alt+클릭: 다른 레이어 모두 잠그기·풀기)')}
                  onClick={(e) => {
                    if (!ctx.editable) return;
                    if (e.altKey) {
                      // InDesign: Alt+click locks every other layer (this one unlocked); again, unlocks them
                      const others = layers.filter((o) => o.id !== l.id);
                      const lock = others.some((o) => !o.locked) || l.locked;
                      const cmds = (lock ? [...(l.locked ? [{ op: 'setLayer', id: l.id, locked: false }] : []), ...others.filter((o) => !o.locked).map((o) => ({ op: 'setLayer', id: o.id, locked: true }))]
                        : others.map((o) => ({ op: 'setLayer', id: o.id, locked: false }))) as EditCommand[];
                      if (cmds.length) ctx.edit(lock ? tr('다른 레이어 잠그기') : tr('모든 레이어 잠금 해제'), cmds);
                      return;
                    }
                    ctx.edit(l.locked ? tr("레이어 잠금 해제") : tr("레이어 잠금"), [{ op: 'setLayer', id: l.id, locked: !l.locked }]);
                  }}
                >{l.locked ? <Icon name="lock" size={13} /> : null}</span>
                <span className="lp-color" style={{ background: `rgb(${l.color.join(',')})` }} />
                <span className="lp-twisty" onClick={() => setOpen((o) => ({ ...o, [l.id]: !o[l.id] }))}>{open[l.id] ? '▾' : '▸'}</span>
                <span className={l.printable ? 'lp-name' : 'lp-name noprint'} title={tr('눌러서 활성 레이어로, Ctrl·Shift+클릭으로 여러 레이어 고르기(병합), 두 번 눌러 레이어 옵션')}
                  onClick={(e) => {
                    if (e.ctrlKey || e.metaKey || e.shiftKey) { le?.pick(l.id, true); return; }
                    le?.pick(l.id, false);
                    if (ctx.editable && l.id !== ctx.info?.activeLayer) ctx.edit(tr('활성 레이어 바꾸기'), [{ op: 'activeLayer', id: l.id }]);
                  }}
                  onDoubleClick={() => { if (ctx.editable) le?.openOptions(l.id); }}>{l.name}</span>
                <span className="lp-pen" title={tr("새 개체를 만드는 활성 레이어")}>{l.id === ctx.info?.activeLayer ? <Icon name="pen2" size={12} /> : null}</span>
                <span className="lp-sel" draggable={ctx.editable && l.id === selLayer} title={l.id === selLayer ? tr('다른 레이어로 끌면 선택한 개체를 옮깁니다') : undefined}
                  onDragStart={(e) => { e.stopPropagation(); e.dataTransfer.setData('folio/items', JSON.stringify(movable())); }}>{l.id === selLayer ? <i style={{ background: `rgb(${l.color.join(',')})` }} /> : null}</span>
              </div>
              {open[l.id] && items.map(({ page, it }) => row(page, it, 1, l.color))}
              {open[l.id] && !items.length && <div className="lp-none">{tr("이 펼침면에 개체 없음")}</div>}
            </div>
          );
        })}
      </div>
      <Foot buttons={[
        { icon: 'plus', title: tr('새 레이어 만들기'), onClick: ctx.editable ? () => newLayer(ctx.edit, layers) : undefined },
        { icon: 'trash', title: tr('활성 레이어 삭제'), onClick: ctx.editable && layers.length > 1 ? remove : undefined },
      ]}>{tr('페이지: {0}, {1}개 레이어', {0:ctx.doc.pages[ctx.current]?.name ?? '',1:layers.length})}</Foot>
    </div>
  );
}

const KIND_LABEL: Record<string, string> = { get textFrame() { return tr("텍스트 프레임"); }, get rectangle() { return tr("사각형"); }, get oval() { return tr("타원"); }, get polygon() { return tr("다각형"); }, get line() { return tr("선"); }, get group() { return tr("그룹"); }, get other() { return tr("개체"); } };

/** How InDesign names an object in the Layers panel: its name, a placed file, or its kind. */
function itemLabel(it: ItemInfo) {
  if (it.name) return it.name;
  if (it.content?.link) return `<${it.content.link}>`;
  return `<${KIND_LABEL[it.kind] ?? it.kind}>`;
}

// ---------------------------------------------------------------------------------------
// Links

const STATUS: Record<LinkStatus, { icon: string | null; label: string }> = {
  found: { icon: null, get label() { return tr("최신"); } },
  missing: { icon: 'missing', get label() { return tr("없음"); } },
  modified: { icon: 'modified', get label() { return tr("수정됨"); } },
  embedded: { icon: 'embedded', get label() { return tr("포함됨"); } },
  ambiguous: { icon: 'modified', get label() { return tr("여러 후보"); } },
};
/** The name a row shows: a content link has the source's name, `<rectangle>`-like when it has none (said in the screen's language). */
function linkName(l: LinkRow): string {
  return l.content?.generic ? `<${tr(KIND_LABEL[l.content.generic] ?? l.content.generic)}>` : l.name;
}
const HOW: Record<string, string> = { get recorded() { return tr("기록된 위치"); }, get document() { return tr("문서 폴더"); }, get links() { return tr("Links 폴더"); }, get folder() { return tr("검색 폴더(경로 일치)"); }, get name() { return tr("검색 폴더(이름만)"); } };

const THUMB = 22;
/** A link's thumbnail in the name column: its frame as the page shows it (the page's own preview, rendered for the frame only). */
function LinkThumb({ link, rev }: { link: LinkRow; rev: unknown }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const b = link.bounds;
  useEffect(() => {
    const el = ref.current;
    if (!el || link.page === null || !b || b.length !== 4 || b[2] <= b[0] || b[3] <= b[1]) return;
    let active = true;
    const scale = Math.min(8, (THUMB * (window.devicePixelRatio || 1)) / Math.max(b[2] - b[0], b[3] - b[1]));
    queue.renderRegion(link.page, scale, b, 20).then((bm) => {
      if (!active) return;
      el.width = bm.width; el.height = bm.height;
      el.getContext('2d')!.putImageData(new ImageData(bm.pixels, bm.width, bm.height), 0, 0);
    }, () => { /* the row keeps its name only */ });
    return () => { active = false; };
  }, [link.page, b?.join(','), rev]);
  const aspect = b && b.length === 4 && b[3] > b[1] ? (b[2] - b[0]) / (b[3] - b[1]) : 1;
  const [w, h] = aspect >= 1 ? [THUMB, Math.max(4, Math.round(THUMB / aspect))] : [Math.max(4, Math.round(THUMB * aspect)), THUMB];
  return <span className="lk-thumb" data-link-thumb style={{ width: THUMB, height: THUMB, display: 'inline-flex', alignItems: 'center', justifyContent: 'center', flex: 'none', marginRight: 4 }}>
    {link.page !== null && <canvas ref={ref} style={{ width: w, height: h, background: 'var(--panel-bg, #fff)', outline: '1px solid rgba(128,128,128,.4)' }} />}
  </span>;
}

export function LinksPanel({ ctx }: { ctx: PanelCtx }) {
  const [localSel, setLocalSel] = useState<LinkRow | null>(null);
  const [sort, setSort] = useState<'name' | 'status' | 'page'>('name');
  const [localInfo, setLocalInfo] = useState(true);
  // the panel menu acts on the panel's link (App keeps it: ui/linkmenu.ts)
  const selected = ctx.linkMenu ? ctx.linkMenu.selected : localSel;
  const setSelected = ctx.linkMenu ? ctx.linkMenu.select : setLocalSel;
  const infoOpen = ctx.linkMenu ? ctx.linkMenu.infoOpen : localInfo;
  const setInfoOpen = ctx.linkMenu ? ctx.linkMenu.setInfoOpen : setLocalInfo;
  const cols = ctx.linkMenu?.columns ?? { status: true, page: true };
  const thumbs = ctx.linkMenu?.thumbnails ?? true;
  const [rootsOpen, setRootsOpen] = useState(false);
  // the picked link's file: its XMP properties (title, description, author …) in the link information
  const meta = useFileMetadata(infoOpen && selected && !selected.content ? selected.path : null);
  const rows = useMemo(() => {
    // the content links (Content Placer ▸ Create Link) are listed with the placed files
    const list = [...(ctx.links?.links ?? []), ...(ctx.links?.contentLinks ?? [])];
    const order: LinkStatus[] = ['missing', 'modified', 'ambiguous', 'embedded', 'found'];
    if (sort === 'name') list.sort((a, b) => linkName(a).localeCompare(linkName(b), 'ko'));
    if (sort === 'status') list.sort((a, b) => order.indexOf(a.status) - order.indexOf(b.status));
    if (sort === 'page') list.sort((a, b) => (a.page ?? 1e9) - (b.page ?? 1e9));
    return list;
  }, [ctx.links, sort]);
  if (!ctx.doc) return <Empty />;
  const s = ctx.links?.summary;
  const contentRows = ctx.links?.contentLinks ?? [];
  const modifiedCount = (s?.modified ?? 0) + contentRows.filter((l) => l.status === 'modified').length;
  return (
    <div className="links-panel" style={{ ["--lk-cols" as string]: ["minmax(0, 1fr)", cols.status ? "26px" : "", cols.page ? "52px" : ""].filter(Boolean).join(" ") }}>
      <div className="lk-head" onContextMenu={(e) => { e.preventDefault(); ctx.linkMenu?.setTarget({ column: e.target instanceof Element ? e.target.closest('[data-col]')?.getAttribute('data-col') ?? undefined : undefined }); ctx.openContext?.('linksHeader', { x: e.clientX, y: e.clientY }); }}>
        <button data-col="name" onClick={() => setSort('name')} className={sort === 'name' ? 'on' : ''}>{tr("이름")}</button>
        {cols.status && <button data-col="status" onClick={() => setSort('status')} className={sort === 'status' ? 'on' : ''}>⚠</button>}
        {cols.page && <button data-col="page" onClick={() => setSort('page')} className={sort === 'page' ? 'on' : ''}>{tr("페이지")}</button>}
      </div>
      <div className="lk-list">
        {ctx.linksBusy && !ctx.links && <p className="p-empty">{tr("링크를 찾는 중…")}</p>}
        {rows.map((l) => (
          <button
            key={l.owner}
            className={selected?.owner === l.owner ? 'lk-row on' : 'lk-row'}
            onClick={() => setSelected(l)}
            onDoubleClick={() => { if (l.page !== null) ctx.go({ page: l.page, bounds: l.bounds }); }}
            title={l.path ?? l.recorded}
          >
            <span className="lk-name" style={thumbs && !l.content ? { display: 'flex', alignItems: 'center' } : undefined}>{thumbs && !l.content && <LinkThumb link={l} rev={ctx.links} />}{l.pdf ? `${tr('PDF {page}쪽', { page: l.pdf.page })} · ` : ''}{linkName(l)}</span>
            {cols.status && <span className="lk-st">{STATUS[l.status].icon && <Icon name={STATUS[l.status].icon!} size={13} title={STATUS[l.status].label} />}{l.content?.edited && <Icon name="pen2" size={13} title={tr('복사본을 고쳤습니다')} />}</span>}
            {cols.page && <span className="lk-page">{l.pageName ?? 'PB'}</span>}
          </button>
        ))}
      </div>
      <button className="lk-section" onClick={() => setInfoOpen(!infoOpen)}>{infoOpen ? '▾' : '▸'} {tr("링크 정보")}</button>
      {infoOpen && (
        <div className="lk-info" onContextMenu={(e) => {
          e.preventDefault();
          // the information field under the pointer (a label stands for the value beside it)
          const hit = e.target instanceof Element ? e.target.closest('dt, dd') : null;
          const value = hit?.tagName === 'DT' ? hit.nextElementSibling : hit;
          ctx.linkMenu?.setTarget({ field: value?.textContent ?? undefined });
          ctx.openContext?.('linkInfo', { x: e.clientX, y: e.clientY });
        }}>
          {selected ? (
            <dl className={ctx.linkMenu?.autoSize ? 'fit' : undefined}>
              <dt>{tr("이름:")}</dt><dd>{linkName(selected)}</dd>
              <dt>{tr("상태:")}</dt><dd>{STATUS[selected.status].label}{selected.how ? ` · ${HOW[selected.how] ?? selected.how}` : ''}{selected.content?.edited ? ` · ${tr('복사본을 고침')}` : ''}</dd>
              <dt>{tr("페이지:")}</dt><dd>{selected.pageName ?? tr("대지")}</dd>
              <dt>{tr("형식:")}</dt><dd>{selected.content ? (selected.content.external ? (selected.content.kind === 'story' ? tr('외부 링크 이야기') : tr('외부 링크 개체')) : selected.content.kind === 'story' ? tr('내부 링크 이야기') : tr('내부 링크 개체')) : selected.format ?? ''}</dd>
              {!selected.content && linkXmpRows(meta).map(([label, value]) => <Fragment key={label}><dt>{label}</dt><dd data-link-xmp>{value}</dd></Fragment>)}
              {selected.content && <><dt>{tr('원본:')}</dt><dd>{selected.content.status === 'external' ? tr('다른 문서(읽을 수 없음)') : selected.content.status === 'missing' ? (selected.content.external ? tr('원본 문서나 그 안의 개체가 없습니다') : tr('원본이 없습니다')) : selected.content.external ? tr('프로젝트의 다른 문서') : tr('같은 문서')}</dd></>}
              {selected.pdf && <><dt>{tr('PDF 쪽:')}</dt><dd>{selected.pdf.page}{selected.pdfPages ? ` / ${selected.pdfPages}` : ''}</dd>
                <dt>{tr('PDF 크기:')}</dt><dd>{Math.round(selected.pdf.width * 100) / 100} × {Math.round(selected.pdf.height * 100) / 100} pt</dd></>}
              {!selected.content && <><dt>{tr("색상 공간:")}</dt><dd>{selected.space ?? ''}</dd>
                <dt>{tr("유효 PPI:")}</dt><dd>{selected.ppi !== null ? Math.round(selected.ppi) : ''}</dd>
                <dt>{tr("경로:")}</dt><dd className="lk-path">{selected.path ?? selected.recorded}</dd></>}
            </dl>
          ) : (
            <p className="p-empty small">{tr("링크를 고르면 정보가 보입니다. 두 번 누르면 그 자리로 갑니다.")}</p>
          )}
        </div>
      )}
      <button className="lk-section" onClick={() => setRootsOpen((o) => !o)}>{rootsOpen ? '▾' : '▸'}{tr('링크 검색 폴더 ({0})', {0:ctx.settings.linkRoots.length})}</button>
      {rootsOpen && (
        <div className="lk-roots">
          {ctx.settings.linkRoots.map((r) => (
            <div key={r} className="lk-root" title={r}><span>{r}</span><button onClick={() => ctx.onRemoveRoot(r)} title={tr("빼기")}><Icon name="close" size={11} /></button></div>
          ))}
          <div className="lk-root-actions">
            <button onClick={ctx.onAddRoot}>{tr("폴더 추가…")}</button>
            <button onClick={ctx.onRelink} disabled={ctx.linksBusy}>{ctx.linksBusy ? tr("찾는 중…") : tr("다시 찾기")}</button>
          </div>
        </div>
      )}
      <Foot
        buttons={[
          { icon: 'relink', title: tr("다시 연결: 검색 폴더 추가"), onClick: ctx.onAddRoot },
          { icon: 'goTo', title: tr("링크로 이동"), onClick: () => { if (selected && selected.page !== null) ctx.go({ page: selected.page, bounds: selected.bounds }); }, disabled: !selected || selected.page === null },
          { icon: 'refresh', title: tr("링크 다시 찾기"), onClick: ctx.onRelink },
          { icon: 'pen2', title: tr("원본 편집") },
          // content links: Update Link (the menu's command) and Unlink
          { icon: 'rotateCw', title: tr('링크 업데이트'), onClick: ctx.linkMenu?.handlers['id.panel.links.updateLink']?.run ?? (() => {}), disabled: !ctx.linkMenu?.handlers['id.panel.links.updateLink']?.run },
          { icon: 'chainBroken', title: tr('링크 끊기'), onClick: ctx.linkMenu?.unlinkContent ?? (() => {}), disabled: !ctx.linkMenu?.unlinkContent },
        ]}
      >
        {s ? (s.missing ? (modifiedCount ? tr('링크 {0}개 · 없음 {1} · 수정됨 {2}', {0:s.links + contentRows.length,1:s.missing,2:modifiedCount}) : tr('링크 {0}개 · 없음 {1}', {0:s.links + contentRows.length,1:s.missing,2:modifiedCount})) : (modifiedCount ? tr('링크 {0}개 · 수정됨 {2}', {0:s.links + contentRows.length,1:s.missing,2:modifiedCount}) : tr('링크 {0}개', {0:s.links + contentRows.length,1:s.missing,2:modifiedCount}))) : ''}
      </Foot>
    </div>
  );
}

// ---------------------------------------------------------------------------------------
// Stroke, Color, Swatches

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="p-row">
      <span className="p-label">{label}</span>
      <span className="p-ctl">{children}</span>
    </div>
  );
}

/** A value box: shows the value, cannot change it yet (disabled). */
function Box({ value, width = 80 }: { value: string; width?: number }) {
  return <span className="p-box" aria-disabled="true" style={{ width }}>{value}<i>▾</i></span>;
}

export function StrokePanel({ ctx }: { ctx: PanelCtx }) {
  const [, tick] = useState(0);
  useEffect(() => ctx.store.subscribe(() => tick((n) => n + 1)), [ctx.store]);
  return <Stroke targets={strokeTargets(ctx.selection, (pg) => ctx.store.peek(pg))} info={ctx.info} editable={ctx.editable} edit={ctx.edit} options={ctx.panelOptions?.stroke !== false} />;
}

function swatchOf(ctx: PanelCtx, name: string | null | undefined) {
  return name ? ctx.info?.swatches.find((s) => s.name === name) ?? null : null;
}

export function ColorPanel({ ctx }: { ctx: PanelCtx }) {
  const sel = useSelected(ctx);
  const [stroke, setStroke] = useState(false);
  const paint = stroke ? sel?.item.stroke : sel?.item.fill;
  const sw = swatchOf(ctx, paint?.swatch);
  const tintValue = paint?.tint == null || paint.tint < 0 ? 100 : paint.tint;
  const tint = tintValue / 100;
  // the colour as the panel shows it: a swatch's values (a CMYK one at its tint), or an unnamed colour's own
  const shown: Shown = sw
    ? { space: sw.space, values: sw.space === 'CMYK' ? sw.values.map((v) => v * ((sw.tint ?? 100) / 100) * tint) : sw.values, rgb: paint?.rgb ?? sw.rgb }
    : { space: paint?.space ?? null, values: paint?.values ?? null, rgb: paint?.rgb ?? null };
  const native = nativeMode(shown.space);
  const mode = ctx.colorPanel?.mode ?? native;
  const own = paint?.kind === 'other' && paint.color ? paint.color : null;
  const publish = ctx.colorPanel?.publish;
  // the selection, not the item read back (that is missing while an edit's page items are fetched again)
  const key = `${ctx.selection.map((e) => e.id).join(',')}:${stroke}`;
  useEffect(() => publish?.(native, own, key), [publish, native, own, key]);
  const values = paint && paint.kind !== 'none' && paint.kind !== 'gradient' ? valuesIn(mode, shown) : null;
  const can = ctx.editable && !!sel && ctx.selection.every((e) => !e.content) && !!values;
  const setChannel = (i: number, v: number) => {
    if (!can || !values) return;
    const c = edited(mode, values, i, v);
    ctx.edit(tr('색상'), [{ op: 'setPaintColor', ids: ctx.selection.map((e) => e.id), stroke, space: c.space, value: c.value }]);
  };
  return (
    <div className="p-form color-panel" data-mode={mode}>
      <div className="cl-top">
        <MiniProxy fill={sel?.item.fill ?? null} stroke={sel?.item.stroke ?? null} strokeFocus={stroke} onFocus={setStroke} onOpen={ctx.openColorPicker} />
        <span className="cl-name">{paint ? (paint.kind === 'none' ? tr("[없음]") : paint.swatch ?? '') : ''}</span>
      </div>
      <TintFields key={`${sel?.item.id}:${stroke}:${paint?.swatch}`} value={tintValue}
        disabled={!ctx.editable||ctx.selection.length!==1||!sel||sel.entry.content||!sw||!['process','spot'].includes(sw.model)} edit={ctx.edit}
        build={value=>sel?[{op:'setPaint',ids:[sel.item.id],...(stroke?{strokeTint:value}:{fillTint:value})}]:[]}/>
      {ctx.panelOptions?.color !== false && <>
      {CHANNELS[mode].map((ch, i) => (
        <ChannelRow key={`${mode}${i}`} mode={mode} index={i} channel={ch} values={values} disabled={!can} commit={(n) => setChannel(i, n)} edit={ctx.edit}
          build={(from, n) => [{ op: 'setPaintColor', ids: ctx.selection.map((e) => e.id), stroke, ...edited(mode, from, i, n) }]} />
      ))}
      <div className="cl-ramp" role="slider" aria-label={tr('색상 램프')} aria-disabled={!can} title={tr('색상 램프에서 고르기')}
        onClick={(e) => {
          if (!can) return;
          const r = e.currentTarget.getBoundingClientRect(), c = rampColor(mode, (e.clientX - r.left) / r.width);
          ctx.edit(tr('색상'), [{ op: 'setPaintColor', ids: ctx.selection.map((x) => x.id), stroke, space: c.space, value: c.value }]);
        }} />
      </>}
    </div>
  );
}

const TRACK_END = {
  cmyk: ['#00aeef', '#ec008c', '#fff200', '#231f20'],
  rgb: ['#ff3b30', '#34c759', '#0a84ff'],
  hsb: [], lab: [],
};
function trackBackground(mode: ColorMode, i: number) {
  if (mode === 'hsb') return i === 0 ? 'linear-gradient(90deg, #f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)' : i === 1 ? 'linear-gradient(90deg, #fff, #f00)' : 'linear-gradient(90deg, #000, #fff)';
  if (mode === 'lab') return i === 0 ? 'linear-gradient(90deg, #000, #fff)' : i === 1 ? 'linear-gradient(90deg, #0a9, #ec008c)' : 'linear-gradient(90deg, #26c, #fff200)';
  return `linear-gradient(90deg, #fff, ${TRACK_END[mode][i]})`;
}

function ChannelRow({ mode, index, channel, values, disabled, commit, build, edit }: {
  mode: ColorMode; index: number; channel: { key: string; min: number; max: number; unit: string }; values: number[] | null; disabled: boolean;
  commit: (v: number) => void;
  /** The commands that give channel `index` the value, the other channels as `from` has them. */
  build: (from: number[], value: number) => EditCommand[];
  edit: (label: string, commands: EditCommand[]) => unknown;
}) {
  const value = values ? values[index] ?? null : null;
  // the colour as it was when a trial began: what a shown preview reads back is the converted colour (HSB of a grey has no hue)
  const start = useRef<number[] | null>(null);
  const [grip, setGrip] = useState<HTMLElement | null>(null);
  const handlers = usePreviewEdit(tr('색상'), (v) => build(start.current ?? values ?? [], v), edit);
  const span = channel.max - channel.min;
  return (
    <div className="cl-slider" data-channel={channel.key}>
      <span ref={setGrip} className="cl-k">{channel.key}</span>
      <span className="cl-track" style={{ background: trackBackground(mode, index) }}
        onClick={(e) => { if (disabled) return; const r = e.currentTarget.getBoundingClientRect(); commit(Math.round((channel.min + span * Math.min(1, Math.max(0, (e.clientX - r.left) / r.width))) * 10) / 10); }}>
        {value !== null && <i style={{ left: `${((value - channel.min) / span) * 100}%` }} />}
      </span>
      <NumberField value={value} label={tr('{0} 값', { 0: channel.key })} min={channel.min} max={channel.max} step={1} decimals={1} width="100%" disabled={disabled}
        scrubElement={grip} isolateKeys
        parse={(t) => { const n = Number(t.trim().replace(',', '.')); return t.trim() !== '' && Number.isFinite(n) ? Math.min(channel.max, Math.max(channel.min, n)) : null; }}
        onPreview={(v, b) => { start.current ??= values; handlers.onPreview(v, b); }}
        onCommit={(v) => { handlers.onCommit(v); start.current = null; }}
        onCancel={() => { start.current = null; handlers.onCancel(); }} />
      <span className="p-unit">{channel.unit}</span>
    </div>
  );
}

function MiniProxy({ fill, stroke, strokeFocus, onFocus, onOpen }: { fill: ItemInfo['fill'] | null; stroke: ItemInfo['stroke'] | null; strokeFocus: boolean; onFocus: (s: boolean) => void; onOpen?: (stroke: boolean) => void }) {
  const bg = (p: ItemInfo['fill'] | null) => (p && p.kind !== 'none' && p.rgb ? `rgb(${p.rgb.join(',')})` : '#fff');
  return (
    <span className="mini-proxy">
      <button className={`mp-fill${strokeFocus ? '' : ' front'}${!fill || fill.kind === 'none' ? ' none' : ''}`} style={{ background: bg(fill) }} onClick={() => onFocus(false)} onDoubleClick={() => onOpen?.(false)} title={tr("칠")} />
      <button className={`mp-stroke${strokeFocus ? ' front' : ''}${!stroke || stroke.kind === 'none' ? ' none' : ''}`} style={{ borderColor: bg(stroke) }} onClick={() => onFocus(true)} onDoubleClick={() => onOpen?.(true)} title={tr("획")} />
    </span>
  );
}

export function SwatchesPanel({ ctx }: { ctx: PanelCtx }) {
  const sel = useSelected(ctx);
  const [stroke, setStroke] = useState(false);
  const picked = ctx.colorMenu?.picked[0] ?? null;
  const [tint, setTint] = useState('100');
  // T = the Tools panel's Formatting Affects Text (J): the text of the selected text frames takes the swatch (off: the object's own
  // fill and stroke). A caret or a selected run of characters always takes it itself.
  const textMode = !!ctx.formatText;
  const [dialog, setDialog] = useState<null | { mode: 'new' | 'edit' | 'delete'; id: string | null }>(null);
  const targets = strokeTargets(ctx.selection, (pg) => ctx.store.peek(pg));
  const textSel = ctx.editable ? ctx.textFormatting.selection : null;
  // the tint the panel shows for what is selected: the characters' (FillTint/StrokeTint, -1 = none = 100), else the objects'; '' where they differ
  const appliedTint = (() => {
    const norm = (v: number | null | undefined) => (v == null ? null : v < 0 ? 100 : v);
    const key = stroke ? 'strokeTint' : 'fillTint';
    if (textSel) { const v = norm(textSel.format[key] as number | null | undefined); return v === null ? '' : String(v); }
    if (!targets.length) return undefined;
    const vs = targets.map((x) => norm((stroke ? x.item.stroke : x.item.fill).tint) ?? 100);
    return vs.every((v) => v === vs[0]) ? String(vs[0]) : '';
  })();
  useEffect(() => { if (appliedTint !== undefined) setTint(appliedTint); }, [appliedTint, stroke]);
  // The tint box shows a trial on the selected objects (their fill or stroke tint) the way the other panels' fields do; the characters' tint
  // (a text selection, T on) and the tint of the next tint swatch are only set when the value is chosen.
  const tintTried = useRef<number | null>(null);
  const tintObjectIds = targets.map((x) => x.id);
  const tintPreview = usePreviewEdit(tr('색조'), (v) => [{ op: 'setPaint', ids: tintObjectIds, ...(stroke ? { strokeTint: v } : { fillTint: v }) }], ctx.edit);
  if (!ctx.doc) return <Empty />;
  const list = ctx.info?.swatches;
  if (!list) return <Empty text={tr("색상 견본 목록은 새 엔진에서 보입니다.")} />;
  const current = stroke ? sel?.item.stroke : sel?.item.fill;
  const textPaint = textSel ? (textSel.format[stroke ? 'strokeColor' : 'fillColor'] as string | null | undefined) : undefined;
  const on = textSel ? (textPaint ? (textPaint === 'Swatch/None' ? '[None]' : list.find((x) => x.id === textPaint)?.name ?? null) : null)
    : current?.kind === 'none' ? '[None]' : current?.swatch ?? null;
  // The text frames (and text on a path) of the selection with T on: the engine's formatFrameText gives their stories the property (the whole story,
  // threaded frames included: static proof from InDesign 2025, artifacts/tbutton-c2-20261023/STATUS.ko.md); other objects stay as they are.
  const frameTextIds = () => targets.filter((x) => !!x.item.text?.story).map((x) => x.id);
  const toFrameText = (label: string, props: Record<string, string | number>) => {
    const ids = frameTextIds();
    if (ids.length) void ctx.edit(label, [{ op: 'formatFrameText', ids, props }]);
  };
  // Swatch or tint to characters (a caret, a run, or the text frames with T on), else to the objects.
  const paintTo = (textLabel: string, objectLabel: string, text: Record<string, string | number>, object: EditCommand | null) => {
    if (textSel) { ctx.textFormatting.apply(text); return true; }
    if (textMode && frameTextIds().length) { toFrameText(textLabel, text); return true; }
    if (object) { ctx.edit(objectLabel, [object]); return true; }
    return false;
  };
  const tintable = !!textSel || (targets.length > 0 && targets.every((x) => { const p = stroke ? x.item.stroke : x.item.fill; const sw = swatchOf(ctx, p.swatch); return !!sw && ['process', 'spot'].includes(sw.model); })) || (textMode && frameTextIds().length > 0);
  const commitTint = (raw: string) => {
    const n = Number(raw);
    if (!ctx.editable || raw.trim() === '' || !Number.isFinite(n) || n < 0 || n > 100 || !tintable) return;
    const ids = targets.map((x) => x.id);
    paintTo(tr('색조'), tr('색조'), { [stroke ? 'strokeTint' : 'fillTint']: n }, ids.length ? { op: 'setPaint', ids, ...(stroke ? { strokeTint: n } : { fillTint: n }) } : null);
  };
  const shown = list.filter(s=>!ctx.colorMenu || ctx.colorMenu.filter==='all' || (ctx.colorMenu.filter==='color'?['process','spot','paper','registration','none'].includes(s.model):s.model===ctx.colorMenu.filter));
  // colour groups (not the root): their swatches are listed together, under the group's name
  const groupOf = new Map<string, { id: string; name: string }>();
  for (const g of ctx.colorMenu?.groups ?? []) if (!g.root) for (const id of g.swatches) groupOf.set(id, g);
  let lastGroup: string | null = null;
  // mixed ink groups: a header row before the group's first swatch (click picks the group, double-click renames it)
  const mixedOf = new Map<string, string>();
  for (const [group, members] of Object.entries(ctx.info?.mixedInkGroups ?? {})) for (const id of members) mixedOf.set(id, group);
  let lastMixed: string | null = null;
  return (
    <div className="swatches-panel">
      <div className="sw-top" hidden={ctx.panelOptions?.swatches === false}>
        <MiniProxy fill={sel?.item.fill ?? null} stroke={sel?.item.stroke ?? null} strokeFocus={stroke} onFocus={setStroke} />
        <button className={textMode ? 'sw-t on' : 'sw-t'} aria-pressed={textMode} data-text-mode={textMode ? 'text' : 'container'} onClick={() => ctx.setFormatText?.(!textMode)}
          title={textMode ? tr('텍스트에 서식 적용: 켜짐 — 선택한 글 프레임의 글자에 적용합니다. 누르면 개체에 적용합니다.') : tr('텍스트에 서식 적용: 꺼짐 — 선택한 개체의 칠과 획에 적용합니다. 누르면 글 프레임의 글자에 적용합니다.')}>T</button>
        <span className="sw-tint">{tr('색조')}: <NumberField value={tint.trim() === '' ? null : Number(tint)} label={tr('색조')} min={0} max={100} step={1} width={46}
          isolateKeys anchor={100}
          onPreview={!textSel && !(textMode && frameTextIds().length) && tintable && tintObjectIds.length ? (v, b) => { tintTried.current = v; tintPreview.onPreview(v, b); } : undefined}
          onCancel={() => { tintTried.current = null; tintPreview.onCancel(); }}
          onCommit={(v) => {
            const tried = tintTried.current !== null;
            tintTried.current = null;
            setTint(String(v));
            if (tried) tintPreview.onCommit(v);
            else if (appliedTint !== undefined && String(v) !== appliedTint) commitTint(String(v));
          }} />%</span>
      </div>
      <label className="sw-filter">{tr('견본 표시')}<select aria-label={tr('견본 표시')} value={ctx.colorMenu?.filter ?? 'all'} onChange={e=>ctx.colorMenu?.setFilter(e.target.value)}><option value="all">{tr('모두')}</option><option value="color">{tr('색상')}</option><option value="tint">{tr('색조')}</option><option value="gradient">{tr('그레이디언트')}</option></select></label>
      <div className="sw-list" data-view={ctx.colorMenu?.view}>
        {shown.map((s) => {
          const grouped = groupOf.get(s.id);
          const header = grouped && grouped.id !== lastGroup ? grouped : null;
          lastGroup = grouped?.id ?? null;
          const mixedId = mixedOf.get(s.id) ?? null;
          const mixedHeader = mixedId && mixedId !== lastMixed ? mixedId : null;
          lastMixed = mixedId;
          const fixed = /^\[(None|Registration|Paper|Black|없음|맞춰찍기|용지|검정)\]$/.test(s.name) || ['None', 'Registration', 'Paper', 'Black'].includes(s.name);
          const label = DISPLAY[s.name] ?? s.name;
          const selected = on !== null && (s.name === on || (on === '[None]' && s.model === 'none'));
          const apply = (e: React.MouseEvent) => {
            // Ctrl+click adds or removes a swatch, Shift+click picks a run of them (no paint applied)
            if (e.ctrlKey || e.metaKey) { ctx.colorMenu?.toggle(s.id); return; }
            if (e.shiftKey) { ctx.colorMenu?.range(s.id, shown.map((x) => x.id)); return; }
            ctx.colorMenu?.select([s.id]);
            const ids = ctx.selection.filter((e) => !e.content).map((e) => e.id);
            if (!ctx.editable) return;
            paintTo(stroke ? tr('글자 획 적용') : tr('글자 칠 적용'), stroke ? tr("획 적용") : tr("칠 적용"), { [stroke ? 'strokeColor' : 'fillColor']: s.id },
              ids.length ? { op: 'setPaint', ids, ...(stroke ? { stroke: s.id } : { fill: s.id }) } : null);
          };
          return (
            <Fragment key={s.id}>
            {mixedHeader && <div className={`sw-group sw-mixed${ctx.colorMenu?.mixedGroup === mixedHeader ? ' on' : ''}`} data-mixed-group={mixedHeader} onClick={() => ctx.colorMenu?.selectMixedGroup(mixedHeader)} onDoubleClick={() => ctx.colorMenu?.openMixedGroup?.(mixedHeader)}><Icon name="folder" size={12} />{mixedGroupName(mixedHeader)}</div>}
            {header && <div className={`sw-group${ctx.colorMenu?.group === header.id ? ' on' : ''}`} data-group={header.id} onClick={() => ctx.colorMenu?.selectGroup(header.id)}><Icon name="folder" size={12} />{header.name}</div>}
            <div className={`${selected ? 'sw-row on' : 'sw-row'}${ctx.editable ? ' click' : ''}${ctx.colorMenu?.picked.includes(s.id) ? ' picked' : ''}`} data-swatch={s.id} onDoubleClick={() => { if (ctx.editable && s.model === 'gradient') { ctx.colorMenu?.openGradient(s.id); return; } if (ctx.editable && swatchEditable(s) && (s.model === 'process' || s.model === 'spot')) setDialog({ mode: 'edit', id: s.id }); }} title={`${label} · ${s.space} ${s.values.map((v) => trim(v, 1)).join(' ')}${ctx.editable ? `\n${stroke ? tr('누르면 선택한 개체의 획에 적용') : tr('누르면 선택한 개체의 칠에 적용')}` : ''}`} onClick={apply}>
              <span className={s.model === 'none' || s.name === 'None' ? 'sw-chip none' : 'sw-chip'} style={s.model === 'none' || s.name === 'None' ? undefined : { background: s.model === 'gradient' ? ramp(s.stops ?? [],list) : `rgb(${s.rgb.join(',')})` }} />
              <span className="sw-name">{label}{s.tint !== null && s.base ? ` ${trim(s.tint, 0)}%` : ''}</span>
              {fixed && <Icon name="noEdit" size={11} className="sw-fixed" />}
              {s.model !== 'none' && s.name !== 'None' && <span className={s.model === 'spot' ? 'sw-kind spot' : 'sw-kind'} title={s.model === 'spot' ? tr("별색") : s.model === 'gradient' ? tr("그레이디언트") : tr("원색")} />}
              {s.model !== 'none' && s.name !== 'None' && <span className="sw-space">{s.space === 'CMYK' ? 'CMYK' : s.space === 'RGB' ? 'RGB' : s.space === 'LAB' || s.space === 'Lab' ? 'Lab' : ''}</span>}
            </div>
            </Fragment>
          );
        })}
      </div>
      <Foot buttons={[
        { icon: 'folder', title: tr('새 색상 그룹'), onClick: ctx.colorMenu?.newGroup },
        { icon: 'plus', title: tr('새 색상 견본'), onClick: ctx.editable ? () => {
          const base = list.find((x) => x.id === picked);
          const t = Number(tint);
          // a colour picked and a tint under 100%: a tint swatch, as InDesign's New Swatch does
          if (base && (base.model === 'process' || base.model === 'spot') && t > 0 && t < 100) ctx.edit(tr('새 색조 견본'), [{ op: 'newTint', base: base.id, tint: t }]);
          else setDialog({ mode: 'new', id: picked });
        } : undefined },
        { icon: 'trash', title: tr('색상 견본 삭제'), onClick: ctx.colorMenu?.deleteMixedGroup ?? (ctx.editable && picked && list.some((x) => x.id === picked && swatchEditable(x)) ? () => setDialog({ mode: 'delete', id: picked }) : undefined) },
      ]}>{tr('{0}개', {0:list.length})}</Foot>
      {dialog && (dialog.mode === 'delete'
        ? (() => { const sw = list.find((x) => x.id === dialog.id); return sw ? <DeleteSwatchDialog swatch={sw} swatches={list} edit={ctx.edit} close={() => setDialog(null)} /> : null; })()
        : <SwatchDialog mode={dialog.mode} from={list.find((x) => x.id === dialog.id) ?? null} swatches={list} edit={ctx.edit} close={() => setDialog(null)} />)}
    </div>
  );
}

const DISPLAY: Record<string, string> = { get None() { return tr("[없음]"); }, get Registration() { return tr("[맞춰찍기]"); }, get Paper() { return tr("[용지]"); }, get Black() { return tr("[검정]"); }, get '[None]'() { return tr("[없음]"); }, get '[Registration]'() { return tr("[맞춰찍기]"); }, get '[Paper]'() { return tr("[용지]"); }, get '[Black]'() { return tr("[검정]"); } };

// ---------------------------------------------------------------------------------------
// Info, Transform, Effects

export function InfoPanel({ ctx }: { ctx: PanelCtx }) {
  const cur = useLive(cursor);
  const m = useLive(measure);
  const sel = useSelected(ctx);
  const L = (v: number | undefined, axis: 'h' | 'v' = 'h') => (v === undefined ? '' : formatLength(v, axis === 'h' ? ctx.units.horizontal : ctx.units.vertical));
  const off = (page: number) => rulerOffset(ctx.rulerOrigin, pageOf(ctx.info, page)?.spreadOffset, zeroOf(ctx.info));
  const it = sel?.item;
  const content = sel?.entry.content && it?.content ? it.content : null;
  const box = content ? content.bounds : it?.bounds;
  const d = m ? Math.hypot(m.x1 - m.x0, m.y1 - m.y0) : undefined;
  // the protractor (a second line from the first one's start): its length and the angle between the two lines, as InDesign's D1, D2 and angle
  const leg = m && m.x2 !== undefined && m.y2 !== undefined ? { d: Math.hypot(m.x2 - m.x0, m.y2 - m.y0), x: m.x2, y: m.y2 } : null;
  const ang = !m ? undefined : leg
    ? (() => { const a = Math.abs(Math.atan2(m.y1 - m.y0, m.x1 - m.x0) - Math.atan2(leg.y - m.y0, leg.x - m.x0)) * 180 / Math.PI; return a > 180 ? 360 - a : a; })()
    : -(Math.atan2(m.y1 - m.y0, m.x1 - m.x0) * 180) / Math.PI;
  // with nothing selected, W and H are the measured line's width and height (InDesign's Info panel while measuring)
  const mw = !box && m && !leg ? Math.abs(m.x1 - m.x0) : undefined, mh = !box && m && !leg ? Math.abs(m.y1 - m.y0) : undefined;
  return (
    <div className="info-panel">
      <div className="ip-grid">
        <span>X:</span><b>{cur ? L(cur.x + off(cur.page)[0]) : ''}</b>
        <span>W:</span><b>{box ? L(box[2] - box[0]) : mw !== undefined ? L(mw) : ''}</b>
        <span>Y:</span><b>{cur ? L(cur.y + off(cur.page)[1], 'v') : ''}</b>
        <span>H:</span><b>{box ? L(box[3] - box[1], 'v') : mh !== undefined ? L(mh, 'v') : ''}</b>
        <span>{leg ? 'D1:' : 'D:'}</span><b data-measure="d1">{d !== undefined ? L(d) : ''}</b>
        {leg && <><span>D2:</span><b data-measure="d2">{L(leg.d)}</b></>}
        <span>∠:</span><b data-measure="angle">{ang !== undefined ? `${trim(ang, 2)}°` : ''}</b>
      </div>
      {/* Show Content Offset: the graphic's top left from its frame's */}
      {ctx.infoOptions?.contentOffset && it?.content && (
        <div className="ip-grid ip-offset">
          <span>X+:</span><b>{L(it.content.bounds[0] - it.bounds[0])}</b>
          <span>Y+:</span><b>{L(it.content.bounds[1] - it.bounds[1], 'v')}</b>
        </div>
      )}
      {ctx.infoOptions?.options !== false && <div className="ip-more">
        {content && (
          <>
            <p>{tr('형식: {0}', {0:content.kind})}{content.link ? ` · ${content.link}` : ''}</p>
            {content.ppi && <p>{tr('유효 PPI: {0} × {1}', {0:Math.round(content.ppi[0]),1:Math.round(content.ppi[1])})}</p>}
          </>
        )}
        {it && !content && <p>{it.kind === 'textFrame' ? (it.text?.overset ? tr('텍스트 프레임 · 넘침') : tr('텍스트 프레임')) : it.kind}{it.name ? ` · ${it.name}` : ''}</p>}
        {it && it.fill.kind !== 'none' && <p>{tr('칠: {0}', {0:it.fill.swatch ?? tr('색상')})}</p>}
        {it && it.stroke.kind !== 'none' && <p>{tr('획: {0} {1} pt', {0:it.stroke.swatch ?? tr('색상'),1:trim(it.stroke.weight, 3)})}</p>}
      </div>}
    </div>
  );
}

export function TransformPanel({ ctx }: { ctx: PanelCtx }) {
  const sel = useSelected(ctx);
  if (ctx.control && ctx.doc) return <TransformFields {...ctx.control} />;
  const it = sel?.item;
  const content = sel?.entry.content && it?.content ? it.content : null;
  const m = content ? content.transform : it?.transform;
  const b = content ? content.inner : it?.inner;
  const L = (v: number | undefined, axis: 'h' | 'v' = 'h') => (v === undefined ? '' : formatLength(v, axis === 'h' ? ctx.units.horizontal : ctx.units.vertical));
  let x, y, w, h, dd;
  if (m && b && sel) {
    const off = rulerOffset(ctx.rulerOrigin, pageOf(ctx.info, sel.entry.page)?.spreadOffset, zeroOf(ctx.info));
    const [px, py] = apply(m, b[0], b[1]);
    dd = decompose(m);
    x = px + off[0]; y = py + off[1]; w = (b[2] - b[0]) * dd.sx; h = (b[3] - b[1]) * dd.sy;
  }
  return (
    <div className="p-form">
      <Row label="X:"><Box value={L(x)} width={90} /></Row>
      <Row label="Y:"><Box value={L(y, 'v')} width={90} /></Row>
      <Row label="W:"><Box value={L(w)} width={90} /></Row>
      <Row label="H:"><Box value={L(h, 'v')} width={90} /></Row>
      <Row label={tr("비율:")}><Box value={dd ? `${trim(dd.sx * 100, 2)}%` : ''} width={70} /><Box value={dd ? `${trim(dd.sy * 100, 2)}%` : ''} width={70} /></Row>
      <Row label={tr("회전:")}><Box value={dd ? `${trim(dd.rotation, 3)}°` : ''} width={70} /></Row>
      <Row label={tr("기울이기:")}><Box value={dd ? `${trim(dd.shear, 3)}°` : ''} width={70} /></Row>
    </div>
  );
}

export function EffectsPanel({ ctx }: { ctx: PanelCtx }) {
  useSelected(ctx); // drawn again when the selected page's items arrive
  return <Effects targets={blendTargets(ctx.selection, (pg) => ctx.store.peek(pg))} editable={ctx.editable} edit={ctx.edit} openEffects={ctx.openEffects} options={ctx.panelOptions?.effects !== false} />;
}

// ---------------------------------------------------------------------------------------
// styles (names from the document)

export function StylesPanel({ ctx, kind }: { ctx: PanelCtx; kind: 'paragraph' | 'character' | 'object' }) {
  return kind==='object'?<ObjectStylesPanel ctx={ctx}/>:<TextStylesPanel ctx={ctx} kind={kind}/>;
}

// ---------------------------------------------------------------------------------------
// panels waiting for text editing (C3) and object editing (B2): InDesign's layout, inactive

export function CharacterPanel({ctx}:{ctx:PanelCtx}) {
  return <div className="p-form text-format-panel" data-text-options="character"><CharacterFields control={ctx.textFormatting} options={ctx.panelOptions?.character}/><StyleSelect control={ctx.textFormatting} kind="character"/>
    <p className="p-note">{tr("수치 단위: pt (자간·커닝: 1/1000 em)")}</p></div>;
}
export function ParagraphPanel({ctx}:{ctx:PanelCtx}) {
  return <div className="p-form text-format-panel" data-text-options="paragraph"><ParagraphFields control={ctx.textFormatting} options={ctx.panelOptions?.paragraph}/><StyleSelect control={ctx.textFormatting} kind="paragraph"/>
    <p className="p-note">{tr("수치 단위: pt")}</p></div>;
}

/**
 * The distribute spacing: a length the buttons read when they are pressed, so a value typed without a unit follows the unit of the
 * direction (the document's horizontal or vertical unit) as it always did. The panel keeps what was typed as text; the box reads it in
 * the horizontal unit to show the number and to step it. A step, a wheel turn or a drag writes the length with its unit.
 */
function GapField({ control }: { control: AlignmentControls }) {
  const unit = control.units.horizontal;
  const typed = useRef<string | null>(null);
  const spec = lengthSpec(unit);
  return <NumberField value={parseLength(control.gap, unit)} label={tr("분배 간격")} title={tr("단위 없이 입력하면 문서의 가로·세로 단위를 따릅니다.")}
    min={-1e5} max={1e5} step={spec.step} width={100} format={spec.format}
    disabled={!control.useGap && control.bounds !== 'key'}
    parse={(text) => { typed.current = text; return spec.parse(text); }}
    onCommit={(v) => {
      const text = typed.current;
      typed.current = null;
      const same = text !== null && parseLength(text, unit) !== null && Math.abs((parseLength(text, unit) as number) - v) < 1e-9;
      control.setGap(same ? text! : spec.format(v));
    }} />;
}

export function AlignPanel({ control, options = true }: { control: AlignmentControls; options?: boolean }) {
  return (
    <div className="p-form align-panel">
      <div className="al-title">{tr("개체 정렬:")}</div>
      <AlignButtons control={control} />
      <div className="al-title">{tr("개체 분배:")}</div>
      <AlignButtons control={control} distribute />
      <Row label={tr("정렬 기준:")}><AlignReference control={control} /></Row>
      {control.bounds === 'key' && <Row label={tr("키 개체:")}>
        <select className="al-reference" aria-label={tr("키 개체")} value={control.key} disabled={!control.choices.length}
          onChange={(e) => control.setKey(e.target.value)}>
          {control.choices.map((c) => <option key={c.id} value={c.id}>{c.title}</option>)}
        </select>
      </Row>}
      {options && <>
      <div className="al-title">{tr("간격 분배:")}</div>
      <AlignButtons control={control} spacing />
      <label className="p-check"><input type="checkbox" checked={control.useGap || control.bounds === 'key'}
        disabled={control.bounds === 'key'} onChange={(e) => control.setUseGap(e.target.checked)} /> {tr("간격 사용")}</label>
      <GapField control={control} />
      </>}
    </div>
  );
}

export function TextWrapPanel({ctx}:{ctx:PanelCtx}) {
  const selected=useSelected(ctx);
  return <TextWrapControls ctx={ctx} item={selected?.item??null}/>;
}

export { GradientPanel } from '../features/color/gradient';

/** Window ▸ History: the document as opened, then every step done (the last one is where the
 * document is) and the steps undone (greyed, as InDesign shows them). Clicking a step undoes or
 * redoes to it. */
const HP = 'id.panel.history.';
declarePanelCommands([HP + 'delete', HP + 'clearHistory', HP + 'newDocument']);

export function HistoryPanel({ ctx }: { ctx: PanelCtx }) {
  const [busy, setBusy] = useState(false);
  // the panel menu and a step's right-click menu (build-c): Delete one step, Clear History, New Document from a step
  const [menu, setMenu] = useState<null | { x: number; y: number; k: number }>(null);
  const [note, setNote] = useState('');
  const h = ctx.history ?? { undo: [], redo: [] };
  const done = h.undo.length;
  const canDelete = (k: number) => k >= 1 && k <= done && (h.deletable?.[k - 1] ?? k === done);
  const remove = (k: number) => { if (canDelete(k) && ctx.historyCall) void ctx.historyCall({ cmd: 'historyDelete', index: k - 1 }); };
  const clear = () => { if (ctx.historyCall) void ctx.historyCall({ cmd: 'historyClear' }); };
  const newDocument = async (k: number) => {
    setNote('');
    const { header } = await window.folio.request({ cmd: 'historyNewDocument', done: k });
    setNote(header.ok ? tr('새 문서를 만들었습니다: {0}', { 0: String(header.relative ?? header.path ?? '') }) : tr('새 문서를 만들지 못했습니다: {0}', { 0: String(header.error) }));
  };
  const editable = !!ctx.editable && !!ctx.historyCall;
  usePanelCommands('history', {
    [HP + 'delete']: editable && canDelete(done) ? () => remove(done) : undefined,
    [HP + 'clearHistory']: editable && (done > 0 || h.redo.length > 0) ? clear : undefined,
    [HP + 'newDocument']: ctx.doc?.folio ? () => void newDocument(done) : undefined,
  });
  if (!ctx.doc) return <Empty />;
  const stepCommands = (k: number): Commands => ({
    'hist.delete': { label: tr('삭제'), run: editable && canDelete(k) ? () => remove(k) : undefined,
      why: k < 1 || k > done ? tr('실행 취소된 단계나 처음 상태는 지울 수 없습니다.') : tr('뒤 단계가 이 단계에서 바꾼 것을 다시 바꿔서 이 단계만 지울 수 없습니다.') },
    'hist.newDocument': { label: tr('새 문서'), run: ctx.doc?.folio ? () => void newDocument(k) : undefined },
  });
  const go = async (k: number) => {
    if (busy || k === done || !ctx.historyGo) return;
    setBusy(true);
    try { await ctx.historyGo(k); } finally { setBusy(false); }
  };
  const name = ctx.doc.path.split(/[\\/]/).pop() ?? '';
  const row = (k: number, label: string, key: string, undone = false) => (
    <li key={key} className={`hp-row${k === done ? ' on' : ''}${undone ? ' undone' : ''}`} aria-current={k === done ? 'step' : undefined} data-step={k}
      onClick={() => void go(k)} onContextMenu={(e) => { e.preventDefault(); e.stopPropagation(); setMenu({ x: e.clientX, y: e.clientY, k }); }}
      title={undone ? tr('눌러서 이 단계까지 다시 실행') : tr('눌러서 이 단계로 되돌리기')}>{label}</li>
  );
  return (
    <div className="history-panel">
      <ul className="hp-list" aria-busy={busy}>
        {row(0, name, 'open')}
        {h.undo.map((l, i) => row(i + 1, actionLabel(l), `u${i}`))}
        {h.redo.map((l, i) => row(done + i + 1, actionLabel(l), `r${i}`, true))}
      </ul>
      <div className="p-foot"><span className="p-foot-text">{tr('{0}단계', { 0: done + h.redo.length })}</span></div>
      {note && <p className="hp-note" role="status">{note}</p>}
      {menu && createPortal(<Dropdown items={[{ cmd: 'hist.delete' }, { sep: true }, { cmd: 'hist.newDocument' }]} commands={stepCommands(menu.k)} x={menu.x} y={menu.y} onClose={() => setMenu(null)} />, document.body)}
    </div>
  );
}
