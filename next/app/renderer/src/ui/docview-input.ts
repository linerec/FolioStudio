//! Routes pointer and tool input and exposes its previews to the document window.

import { t as tr } from '../i18n';
import { textHit } from '../engine';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { type EditCommand, type EditResult } from '../engine';
import { type Box, pageOf, physicalMargins, unionBox } from '../inspect';
import { commandKey } from './keys';
import { type Ghost, findItem, portAt } from './overlay';
import { cursor, measure as measureLive, selectAllGuidesRequest, useLive } from './store';
import type { ToolId } from './tools';
import { useTextEditor } from './textedit';
import { DRAW_TOOLS, drawRect, type Drawing } from './drawing';
import { NO_GRID, copyGridOffsets, griddable, gridCells, gridKey, polygonKey, type Grid } from './drawgrid';
import { ownRect } from './turned';
import { drawingCommand, drawingSettings, useDrawingSettings } from './drawing-settings';
import { isPlaceTarget } from './place';
import { columnFrame } from './threading';
import { useGuides } from './guides';
import { usePathTools } from './pathtools';
import { useTransformTools } from './transformtools';
import { useGradientTools } from './gradienttools';
import { useEyedropper } from './eyedropper';
import { loadedCursor, useConveyorTools } from './conveyor';
import { useCoordinateTools } from '../features/tools/coordinates';
import { useTableLines } from '../features/tools/tablelines';
import { inDesignZoom, snapPointer } from './dragunit';
import { useLiveCorners } from '../features/view/useLiveCorners';
import { useAnchorControl } from '../features/anchored/useAnchorControl';
import { useTextPathBrackets } from '../features/tools/useTextPathBrackets';
import { SNAP_ZONE_PX, type Targets, snapPoint, snapTargets } from './snap';
import { type SmartDraw, type SmartPrefs, SMART_ALL, hasDraw, smartObjects } from './smartguides';
import { anchorPoints, smartPoint } from './pointsnap';
import { smartLineColor, useUiColorTable } from './smartcolor';
import type { DocWindowProps } from './docview';
import { usePageCoordinates } from './docview-coordinates';
import { stepZoom, useViewportInput } from './docview-viewport';

import { capture, entryFor, selectionInput, type SelectionDrag, type SelectionText } from './docview-selection';

export function useDocumentInput(p: DocWindowProps, labels: SelectionText & { unavailableTool: () => string }) {
  const { vp, rows, doc } = p;
  const scale = vp.scale;
  const text = useTextEditor({
    editable: p.editable, tool: p.tool, document: doc.path, revision: p.pageRev,
    tripleClickSelectsLine: p.tripleClickSelectsLine,
    smartSpacing: p.smartCutAndPaste,
    dragDrop: p.dragDropText,
    edit: p.onEdit, history: p.onTextHistory, actions: p.textActions, onState:p.onTextState, onTable: p.onTableState, tableActions: p.tableActions,
    onTableKey: p.onTableKey,
    reveal:p.textReveal,
    selectFrame: (id, page) => p.setSelection([{ id, page, content: false }]),
    enterTool: p.onDoubleClickText,
    exitTool: p.onExitText, note: p.onNote,
  });
  const [hover, setHover] = useState<{ page: number; id: string } | null>(null);
  const [marquee, setMarquee] = useState<{ page: number; rect: [number, number, number, number] } | null>(null);
  const [zoomBox, setZoomBox] = useState<[number, number, number, number] | null>(null);
  const [drawing, setDrawing] = useState<Drawing | null>(null);
  const drawingState = useDrawingSettings();
  const polygonAspect = drawingState?.aspect ?? 1;
  const [drawingSize, setDrawingSize] = useState<{ page: number; kind: Drawing['kind']; x: number; y: number; centered: boolean } | null>(null);
  const createDrawing = async (index: number, kind: Drawing['kind'], rect: Box) => {
    const page = p.info?.pages.find(pg => pg.index === index)?.id;
    if (!page) throw Error(tr('페이지를 찾을 수 없습니다.'));
    const commands: EditCommand[] = [await drawingCommand(page, kind, rect)];
    // Vertical Type tool: the new frame's story is vertical, in the same edit (one undo)
    if (kind === 'textFrame' && p.tool === 'typeVertical') commands.push({ op: 'setStoryOrientation', story: '', orientation: 'Vertical' });
    const result = await p.onEdit(tr('개체 만들기'), commands);
    const id = result?.created?.[0];
    if (!id) throw Error(tr('개체를 만들지 못했습니다.'));
    p.setSelection([{ page: index, id, content: false }]); p.setSelectedPage(null);
    if (kind === 'textFrame') {
      const pg = doc.pages.find(q => q.index === index);
      const own = pg ? ownRect(doc.pageSettings?.rotation?.[pg.spread], pg.width, pg.height, rect) : rect;
      text.point(index, own[0], own[1], id);
    }
  };
  /** A grid of drawings (the arrow keys while drawing): one edit, every cell selected. */
  const createGrid = async (index: number, kind: Drawing['kind'], cells: Box[]) => {
    const page = p.info?.pages.find(pg => pg.index === index)?.id;
    if (!page) throw Error(tr('페이지를 찾을 수 없습니다.'));
    const commands = await Promise.all(cells.map(cell => drawingCommand(page, kind, cell)));
    const result = await p.onEdit(tr('격자로 개체 만들기'), commands);
    const ids = result?.created ?? [];
    if (!ids.length) throw Error(tr('개체를 만들지 못했습니다.'));
    p.setSelection(ids.map(id => ({ page: index, id, content: false }))); p.setSelectedPage(null);
  };
  const measure = useLive(measureLive);
  const placeLoadedCursor = !!useLive(loadedCursor);
  const drag = useRef<
    | { kind: 'hand'; x: number; y: number; sl: number; st: number }
    | SelectionDrag
    | { kind: 'zoom'; x: number; y: number; moved: boolean; alt: boolean }
    /** `leg`: the protractor's second line (Alt from an end of the line already measured): `x`,`y` is then the corner, `other` the first line's far end. */
    | { kind: 'measure'; page: number; x: number; y: number; other?: [number, number] }
    /** Page tool: a page being dragged: where the pointer went down (client) and the screen pixels per point of that page. */
    | { kind: 'pagemove'; page: number; x: number; y: number; k: number }
    /** Drawing with a shape tool: `grid` the cells the arrow keys asked for (ui/drawgrid.ts), `space` where the pointer was while Space is held (the drawing moves with it), `end` the last pointer point. */
    | { kind: 'create'; page: number; shape: NonNullable<typeof DRAW_TOOLS[ToolId]>; x: number; y: number; rx: number; ry: number; rect: Box | null; snap?: Targets | null; grid: Grid; space?: [number, number] | null; end?: [number, number]; keys?: { shift: boolean; alt: boolean }; polygon?: { sides: number; inset: number } }
    | { kind: 'place'; page: number; x: number; y: number; rect: Box | null; id?: string }
    | { kind: 'thread'; page: number; x: number; y: number; rect: Box | null }
    | null
  >(null);
  const drawingStateRef = useRef(drawingState);
  drawingStateRef.current = drawingState;
  /** The selection's box where the drag picked it up (page points), for the grid of copies. */
  const moveBox = (d: Extract<NonNullable<typeof drag.current>, { kind: 'move' }>) => {
    if (d.snap?.box) return d.snap.box;
    const items = p.store.peek(d.pages[0]);
    const found = items && d.ids.map((id) => findItem(items, id));
    return found && found.every((f) => f) ? unionBox(found.map((f) => f!.item.bounds)) : null;
  };
  /** The offsets of the copies the Alt-drag grid makes (none: one copy, or a plain move). */
  const moveCopies = (d: Extract<NonNullable<typeof drag.current>, { kind: 'move' }>) => {
    const box = d.alt && d.grid ? moveBox(d) : null;
    return box && d.grid ? copyGridOffsets(box, d.by, d.grid) : [];
  };
  const showMove = (d: Extract<NonNullable<typeof drag.current>, { kind: 'move' }>) => {
    const row = vp.viewEl?.querySelector<HTMLElement>(`[data-page="${d.pages[0]}"]`)?.closest('.spread');
    const pages = row ? [...row.querySelectorAll<HTMLElement>('.page')].map((el) => Number(el.dataset.page)) : d.pages;
    const copies = moveCopies(d);
    setGhost({ pages, g: { kind: 'move', ids: d.ids, dx: d.by[0], dy: d.by[1], ...(copies.length ? { copies } : {}) } });
  };
  const showMoveRef = useRef(showMove);
  showMoveRef.current = showMove;
  /** The cells of the drawing in progress (one, or the grid the arrow keys asked for), drawn as the preview. */
  const showCreate = (d: Extract<NonNullable<typeof drag.current>, { kind: 'create' }>) => {
    if (!d.rect) return;
    setDrawing({ page: d.page, kind: d.shape, rect: d.rect, cells: griddable(d.shape) && (d.grid.cols > 1 || d.grid.rows > 1) ? gridCells(d.rect, d.grid) : undefined });
  };
  useEffect(() => {
    // while a shape is drawn: the arrows make a grid of it (Ctrl: the space between), Space moves it, Space with the arrows changes a polygon (ui/drawgrid.ts)
    const keys = (e: KeyboardEvent) => {
      const d = drag.current;
      // Alt-dragging the selection: the arrows make a grid of copies over the dragged area (Ctrl: the space between), as when drawing (tools-c2)
      if (d?.kind === 'move' && e.altKey && d.moved && e.key.startsWith('Arrow')) {
        e.preventDefault(); e.stopImmediatePropagation();
        if (e.type !== 'keydown') return;
        const g = gridKey(d.grid ?? NO_GRID, e.key, e.ctrlKey || e.metaKey);
        if (g) { d.grid = g; d.alt = true; showMoveRef.current(d); }
        return;
      }
      if (d?.kind !== 'create') return;
      if (e.code === 'Space' || e.key === ' ') {
        e.preventDefault(); e.stopImmediatePropagation();
        if (e.type === 'keydown') { if (!d.space) d.space = d.end ?? [d.x, d.y]; } else d.space = null;
        return;
      }
      if (e.type !== 'keydown' || !e.key.startsWith('Arrow')) return;
      e.preventDefault(); e.stopImmediatePropagation();
      if (d.space && (d.shape === 'polygon' || d.shape === 'polygonFrame')) {
        // keys come faster than the engine answers: the drag keeps the settings it asked for and builds on them
        const now = d.polygon ?? drawingStateRef.current?.polygon;
        const next = now && polygonKey(now, e.key);
        if (next) { d.polygon = next; void drawingSettings(next).catch((err) => p.onNote(String(err))); }
        return;
      }
      if (!griddable(d.shape)) return;
      const g = gridKey(d.grid, e.key, e.ctrlKey || e.metaKey);
      if (g) { d.grid = g; showCreate(d); }
    };
    window.addEventListener('keydown', keys, true);
    window.addEventListener('keyup', keys, true);
    return () => { window.removeEventListener('keydown', keys, true); window.removeEventListener('keyup', keys, true); };
  }, []);
  useEffect(() => {
    const cancel = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && (drag.current?.kind === 'create' || drag.current?.kind === 'place' || drag.current?.kind === 'thread')) {
        drag.current = null; setDrawing(null); e.preventDefault(); e.stopImmediatePropagation();
      } else if (e.key === 'Escape' && (drag.current?.kind === 'resize' || drag.current?.kind === 'rotate')) {
        drag.current = null; setGhost(null); setSmartDraw(null); e.preventDefault(); e.stopImmediatePropagation();
      }
    };
    window.addEventListener('keydown', cancel, true);
    return () => window.removeEventListener('keydown', cancel, true);
  }, []);
  const [ghost, setGhost] = useState<{ pages: number[]; g: Ghost } | null>(null);
  /** The Smart Guides lines, arrows and brackets of the drag in progress (ui/smartguides.ts), on the page they belong to. */
  const [smartDraw, setSmartDraw] = useState<{ page: number; draw: SmartDraw } | null>(null);
  const hoverFrame = useRef(0);
  const sides = useMemo(() => new Map((p.info?.pages ?? []).map((q) => [q.index, q.side])), [p.info]);
  const widthOf = useMemo(() => new Map(doc.pages.map((q) => [q.index, q.width])), [doc]);
  /** Screen pixels per point of a drawn page. Its CSS width is rounded to whole pixels, so
   *  this, not the zoom, maps the pointer exactly (the zoom is off by up to half a pixel over
   *  the page: 0.6 pt at its far edge). */
  const pxPerPt = useCallback((el: HTMLElement, r: DOMRect) => {
    // a page of a spread turned by 90 or 270 is shown on its side: its drawn width is the box's height
    const turned = el.closest<HTMLElement>('.spread')?.dataset.turn;
    return (turned === '90' || turned === '270' ? r.height : r.width) / (widthOf.get(Number(el.dataset.page)) ?? r.width / scale);
  }, [widthOf, scale]);

  useViewportInput(p);
  const geometry = usePageCoordinates(p, pxPerPt);
  const { pageAt, ownOf, hitAt, ownAt, turnOf, pointOn, moveOn } = geometry;

  const guides = useGuides({
    editable: p.editable, shown: p.screenMode === 'normal' && p.guides, document: doc.path, info: p.info, store: p.store, layers: p.layers,
    scale, viewRef: vp.viewRef, pageAt, edit: p.onEdit, clearSelection: () => { p.setSelection([]); p.setSelectedPage(null); },
    snapGuide: (page, axis, value, skip) => {
      const pt = pullPoint(snapOn(page, skip), axis === 'x' ? [value, 0] : [0, value]);
      return axis === 'x' ? pt[0] : pt[1];
    },
  });
  useEffect(() => { if (p.selection.length) guides.deselect(); }, [p.selection, guides.deselect]);
  // Select All Guides (Ctrl+Alt+G): the guides of the spread with the current page
  useEffect(() => selectAllGuidesRequest.subscribe(() => {
    const row = rows.find((r) => r.some((pg) => pg.index === p.current)) ?? [];
    if (!guides.selectAll(row.map((pg) => pg.index))) p.onNote(tr('고를 수 있는 안내선이 없습니다.'));
  }), [rows, p.current, guides.selectAll, p.onNote]);
  useEffect(() => { p.onGuideSelection?.(guides.selected); }, [guides.selected, p.onGuideSelection]);
  // the Pen tool and the points of a path (Direct Selection)
  const paths = usePathTools({
    onKey: p.onPathKey,
    tool: p.tool, editable: p.editable, info: p.info, store: p.store, layers: p.layers, selection: p.selection, scale,
    edit: p.onEdit, setSelection: p.setSelection, note: p.onNote,
    snap: (page, pt) => pullPoint(snapOn(page), pt),
    grid: (page, pt) => (turnedPage(page) ? pt : [snapPointer(pt[0], inDesignZoom(scale)), snapPointer(pt[1], inDesignZoom(scale))]),
    hit: (cx, cy) => { const h = hitAt(cx, cy, false, 4, true); return h ? { page: h.page, item: h.leaf } : null; },
    pageAt: ownAt, pencil: p.pencilPrefs, turn: turnOf,
    // Smart Guides for one point (ui/pointsnap.ts): the anchors of the paths of the spread, then the object and page lines, in the snap zone
    smartPoint: (page, pt, skip) => {
      const items = smartOn ? p.store.peek(page) : null;
      if (!items) return null;
      const mates = spreadMates(page), zone = (p.info?.view?.snapZone ?? SNAP_ZONE_PX) / scale;
      return smartPoint(pt, anchorPoints(items, skip, mates), smartObjects(items, skip ? [skip.id] : [], false, mates),
        { size: items.size, margins: physicalMargins(items.margins, items.side ?? sides.get(page)) }, smart, zone);
    },
    showSmart: (page, draw) => setSmartDraw(draw && hasDraw(draw) ? { page, draw } : null),
  });
  useEffect(() => {
    if (!p.pathFinish) return;
    const finish = () => paths.finish(false);
    p.pathFinish.current = finish;
    return () => { if (p.pathFinish?.current === finish) p.pathFinish.current = null; };
  }, [p.pathFinish, paths.finish]);

  const transforms = useTransformTools({ tool: p.tool, editable: p.editable, selection: p.selection, store: p.store,
    info: p.info, scale, view: vp.viewRef, pxPerPt, pointOn, ghost: setGhost, edit: p.onEdit });
  const pivot = transforms.pivot;
  const gradients = useGradientTools({ tool: p.tool, editable: p.editable, selection: p.selection, store: p.store, strokeFocus: p.strokeFocus, text: p.toolboxText,
    view: vp.viewRef, pxPerPt, ghost: setGhost, edit: p.onEdit, note: p.onNote });
  const eyedropper = useEyedropper({ tool: p.tool, editable: p.editable, document: doc.path, info: p.info, edit: p.onEdit, note: p.onNote });
  const conveyorTools = useConveyorTools({ tool: p.tool, editable: p.editable, pageId: (index) => p.info?.pages.find((pg) => pg.index === index)?.id,
    edit: p.onEdit, setSelection: p.setSelection, note: p.onNote });
  const coordinates = useCoordinateTools({ tool: p.tool, editable: p.editable, info: p.info,
    view: vp.viewRef, pxPerPt, ghost: setGhost, edit: p.onEdit });
  const tableLinesTool = useTableLines({ tool: p.tool, editable: p.editable, revision: p.pageRev, view: vp.viewRef, scale, ghost: setGhost, edit: p.onEdit });
  // the live corner: the yellow square of a selected frame and its four diamonds (View ▸ Extras ▸ Live Corners)
  const live = useLiveCorners({ tool: p.tool, editable: p.editable, selection: p.selection, marks: p.marks ?? null, peek: (page) => p.store.peek(page),
    pxPerPt, edit: p.onEdit });
  // the anchored object control: its blue square dragged into text anchors the object there (View ▸ Extras ▸ Show Anchored Object Control)
  const anchorControl = useAnchorControl({ tool: p.tool, editable: p.editable, selection: p.selection, marks: p.marks ?? null, peek: (page) => p.store.peek(page),
    pxPerPt, turned: (page) => { const pg = doc.pages.find((q) => q.index === page); return !!pg && !!doc.pageSettings?.rotation?.[pg.spread]; },
    edit: p.onEdit, options: (id) => p.onAnchorOptions?.(id) });
  // the bracket handles of a selected text path: drawn for the Selection, Direct Selection and Type on a Path tools, dragged with the first two
  const [tpCursor, setTpCursor] = useState<'free' | 'path' | 'text'>('free');
  const tpBrackets = useTextPathBrackets({ tool: p.tool, editable: p.editable, selection: p.selection, peek: (page) => p.store.peek(page), pxPerPt,
    origin: (page) => pageOf(p.info, page)?.spreadOrigin ?? [0, 0],
    turned: (page) => { const pg = doc.pages.find((q) => q.index === page); return !!pg && !!doc.pageSettings?.rotation?.[pg.spread]; }, edit: p.onEdit });

/** Smart Guides: on in the normal screen mode only; the four options say what they do (ui/smartguides.ts). */
  const smart: SmartPrefs = p.smart ?? { ...SMART_ALL, enabled: false };
  const smartOn = smart.enabled && p.screenMode === 'normal';
  const smartCss = smartLineColor(smart.color, useUiColorTable());
  /** What a drag on this page snaps to: View ▸ Grids & Guides ▸ Snap to Guides (while guides show) and Snap to Document Grid. */
  const snapOn = (page: number, skipGuides: string[] = []): Targets | null => {
    const v = p.info?.view;
    if (!v || p.screenMode !== 'normal') return null;
    const guides = v.guidesSnap && p.guides, grid = v.documentGridSnap;
    const peeked = p.store.peek(page), pg = p.info?.pages.find((x) => x.index === page);
    if (!(guides || grid) || !peeked || !pg) return null;
    // a guide being dragged is not a target for itself
    const items = skipGuides.length ? { ...peeked, guides: peeked.guides.filter((g) => !skipGuides.includes(g.id)) } : peeked;
    return snapTargets({
      items, size: items.size, margins: physicalMargins(items.margins, items.side ?? pg.side), guides,
      hiddenGuides: (layer) => { const l = layer ? p.layers.get(layer) : undefined; return !!l && (!l.visible || !l.guides); },
      grid: grid ? v.grid : null, pageLeft: pg.spreadOffset[0], pageTop: pg.spreadOffset[1],
    });
  };

  /** A point of a page pulled onto the snap targets within the snap zone (guides, page edges, margins, columns, gridlines). */
  const pullPoint = (t: Targets | null | undefined, pt: [number, number]): [number, number] => {
    if (!t) return pt;
    const [sx, sy] = snapPoint(pt, t, (p.info?.view?.snapZone ?? SNAP_ZONE_PX) / scale);
    return [pt[0] + sx, pt[1] + sy];
  };

  /** Whether the page sits on a turned spread (Pages panel: Rotate Spread View): the drag is read as the page is shown and the grid there was not measured. */
  const turnedPage = (page: number): boolean => !!vp.viewRef.current?.querySelector<HTMLElement>(`[data-page="${page}"]`)?.closest<HTMLElement>('.spread')?.dataset.turn;

  /** The other pages of the spread `page` is on, each with its loaded items and its place relative to `page`: their objects are Smart Guide targets too. */
  const spreadMates = (page: number) => {
    const me = pageOf(p.info, page);
    if (!me) return [];
    return (p.info?.pages ?? []).filter((pg) => pg.spreadIndex === me.spreadIndex && pg.index !== page)
      .map((pg) => ({ items: p.store.peek(pg.index), dx: pg.spreadOffset[0] - me.spreadOffset[0], dy: pg.spreadOffset[1] - me.spreadOffset[1] }));
  };

  const selection = selectionInput({ labels, p, scale, setDrag: (next) => { drag.current = next; }, geometry, guides, live, anchorControl, tpBrackets, paths, sides,
    smart, smartOn, turnedPage, spreadMates, pxPerPt, snapOn, showMove, moveCopies, setGhost, setSmartDraw, setMarquee });

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0 || !(e.target as HTMLElement).closest('.view')) return;
    const view = vp.viewRef.current!;
    const t = p.tool;
    if (t === 'hand') {
      drag.current = { kind: 'hand', x: e.clientX, y: e.clientY, sl: view.scrollLeft, st: view.scrollTop };
      capture(view, e.pointerId);
      view.classList.add('grabbing');
      return;
    }
    if (t === 'zoom') {
      drag.current = { kind: 'zoom', x: e.clientX, y: e.clientY, moved: false, alt: e.altKey };
      capture(view, e.pointerId);
      return;
    }
    const here = pageAt(e.clientX, e.clientY);
    // Edit ▸ Place and Link: the cursor loaded from the selection places it at this click, whichever tool is current
    if (loadedCursor.get() && here) { e.preventDefault(); void conveyorTools.placeLoaded(here); return; }
    if (t === 'note' && here && p.editable) {
      e.preventDefault();
      const h=hitAt(e.clientX,e.clientY);
      if(h?.leaf.kind==='textFrame') void textHit(here.index,here.x,here.y,h.leaf.id).then(({hit})=>{if(hit)p.onNoteAt(hit.story,hit.at);});
      return;
    }
    // loaded text (from a frame's port): the next click or drag threads it
    if (p.threading.loaded && here && p.editable) {
      e.preventDefault();
      drag.current = { kind: 'thread', page: here.index, x: here.x, y: here.y, rect: null };
      capture(view, e.pointerId);
      return;
    }
    if (p.placement.files.length && here && p.editable) {
      e.preventDefault();
      const hit = hitAt(e.clientX, e.clientY), item = hit?.leaf;
      drag.current = { kind: 'place', page: here.index, x: here.x, y: here.y, rect: null,
        id: item && isPlaceTarget(p.placement.files[0], item.kind) ? item.id : undefined };
      capture(view, e.pointerId);
      return;
    }
    if ((t === 'pen' || t === 'pencil' || t === 'erase' || t === 'smooth' || t === 'direct' || t === 'penAdd' || t === 'penDelete' || t === 'convert' || t === 'scissors') && paths.onDown(e, ownAt(e.clientX, e.clientY))) { capture(view, e.pointerId); return; }
    if (t === 'typePath' && here && p.editable) {
      e.preventDefault();
      // InDesign 2025 (typepath-c8): only the path itself is hit (about 1 screen pixel plus half the stroke; the inside of a shape or frame is not).
      const h = hitAt(e.clientX, e.clientY, true, 1);
      if (h?.leaf.text) {
        text.point(here.index, here.x, here.y, h.leaf.id);
      } else if (h?.leaf.paths?.length) {
        // An open path takes the text over its whole length. A closed shape starts it where the press lands on the outline, the end bracket
        // one point behind (InDesign 2025, typepath-c8): the engine reads the press from spread coordinates and how far from the outline it counts.
        const id = h.leaf.id, closed = h.leaf.paths.some(sub => !sub.open);
        const origin = pageOf(p.info, here.index)?.spreadOrigin ?? [0, 0];
        const reach = 1 / scale + (h.leaf.stroke.kind !== 'none' ? h.leaf.stroke.weight / 2 : 0) + 0.5;
        const command: EditCommand = closed
          ? { op: 'createTextPathAt', id, point: [here.x + origin[0], here.y + origin[1]], reach, text: '' }
          : { op: 'createTextPath', ids: [id], text: '' };
        void p.onEdit(tr('패스에 입력'), [command]).then(result => {
          if (result) text.point(here.index, here.x, here.y, id);
        });
      }
      return;
    }
    if (t === 'type' && tableLinesTool.down(e, here)) { capture(view, e.pointerId); return; }
    if ((t === 'type' || t === 'typeVertical') && here && p.editable) {
      e.preventDefault();
      const h = hitAt(e.clientX, e.clientY);
      if (h?.leaf.text) {
        text.begin(here.index, here.x, here.y, h.leaf.id, e.shiftKey, e.detail || 1);
        capture(view, e.pointerId);
        return;
      }
    }
    const shape = DRAW_TOOLS[t];
    if (shape && here) {
      if (!p.editable) { p.onNote(tr("읽기 전용 문서에는 개체를 만들 수 없습니다.")); return; }
      const layer = p.info?.layers.find(l => l.id === p.info?.activeLayer);
      if (layer && (layer.locked || !layer.visible)) { p.onNote(tr("활성 레이어가 잠겼거나 숨겨져 있습니다.")); return; }
      e.preventDefault();
      // Snap to Guides / Document Grid: the corner the drag starts from lands on a guide, page edge or gridline near it
      const snap = snapOn(here.index), pulled = pullPoint(snap, [here.x, here.y]), [sx, sy] = drawGrid(here.index, [here.x, here.y], pulled);
      // a grid's cells start apart by the page's column gutter (Layout ▸ Margins and Columns; tools-c1 decision)
      const gutter = p.store.peek(here.index)?.columns.gutter ?? NO_GRID.gap[0];
      drag.current = { kind: 'create', page: here.index, shape, x: sx, y: sy, rx: pulled[0], ry: pulled[1], rect: null, snap, grid: { ...NO_GRID, gap: [gutter, gutter] } };
      capture(view, e.pointerId);
      return;
    }
    if (t === 'measure' && here) {
      // Alt on an end of the measured line: a second line from that end, the angle between the two (InDesign's protractor)
      const m = measureLive.get(), near = (x: number, y: number) => Math.hypot(x - here.x, y - here.y) * scale <= 6;
      if (e.altKey && m && m.page === here.index && (near(m.x0, m.y0) || near(m.x1, m.y1))) {
        const [corner, other]: [number, number][] = near(m.x0, m.y0) ? [[m.x0, m.y0], [m.x1, m.y1]] : [[m.x1, m.y1], [m.x0, m.y0]];
        drag.current = { kind: 'measure', page: here.index, x: corner[0], y: corner[1], other };
        measureLive.set({ page: here.index, x0: corner[0], y0: corner[1], x1: other[0], y1: other[1], x2: corner[0], y2: corner[1] });
        capture(view, e.pointerId);
        return;
      }
      drag.current = { kind: 'measure', page: here.index, x: here.x, y: here.y };
      measureLive.set({ page: here.index, x0: here.x, y0: here.y, x1: here.x, y1: here.y });
      capture(view, e.pointerId);
      return;
    }
    if (t === 'page') {
      if (here) {
        p.setSelectedPage(here.index); p.setSelection([]);
        const el = view.querySelector<HTMLElement>(`[data-page="${here.index}"]`);
        if (el && p.onPageMove && p.editable) {
          drag.current = { kind: 'pagemove', page: here.index, x: e.clientX, y: e.clientY, k: pxPerPt(el, el.getBoundingClientRect()) };
          capture(view, e.pointerId);
        }
      }
      return;
    }
    if (transforms.down(e, ownAt(e.clientX, e.clientY))) { capture(view, e.pointerId); return; }
    if (gradients.down(e, here)) { capture(view, e.pointerId); return; }
    if (coordinates.down(e, here)) { capture(view, e.pointerId); return; }
    if (t === 'eyedropper' && eyedropper.down(e, hitAt(e.clientX, e.clientY)?.leaf)) return;
    if ((t === 'collector' || t === 'placer') && conveyorTools.down(e, here, hitAt(e.clientX, e.clientY))) return;
    // Ctrl with the Pen: the Direct Selection tool for the moment
    const st: ToolId = t === 'pen' && commandKey(e) ? 'direct' : t;
    if (st === 'selection' || st === 'direct') { selection.down(e, here, t, st, view); return; }
    if (here) p.onNote(labels.unavailableTool());
  };

  /** A corner of a drawing (page point `pt`, pulled from the pointer `raw` by guides) put on InDesign's drag grid; an axis a guide pulled stays. Measured with
   *  the rectangle tool, plain, Shift and Alt (docdrag-c1): every edge of what is drawn is on the grid, a Shift square or an Alt drawing is worked out from the
   *  rounded corners. A line's two end points are rounded the same way (draground-c1: 320 of 320 coordinates on the grid; Shift: `shiftLine`). */
  const drawGrid = (page: number, raw: [number, number], pt: [number, number]): [number, number] => {
    // (a turned spread is read as it is shown and the grid there was not measured: those drawings keep the pointer's own position)
    if (turnedPage(page)) return pt;
    const z = inDesignZoom(scale);
    return [pt[0] !== raw[0] ? pt[0] : snapPointer(pt[0], z), pt[1] !== raw[1] ? pt[1] : snapPointer(pt[1], z)];
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>) => {
    const view = vp.viewRef.current!;
    const d = drag.current;
    const here = pageAt(e.clientX, e.clientY);
    if (transforms.move(e)) return;
    if (gradients.move(e)) return;
    if (coordinates.move(e)) return;
    if (live.move(e)) return;
    if (anchorControl.move(e)) return;
    if (tpBrackets.move(e)) return;
    if (tableLinesTool.move(e, here, here ? [here.x, here.y] : undefined)) return;
    if (paths.onMove(e, ownAt(e.clientX, e.clientY))) return;
    if (text.dragging.current) {
      if (here) {
        const h = hitAt(e.clientX, e.clientY);
        if (h?.leaf.kind === 'textFrame') text.move(here.index, here.x, here.y, h.leaf.id);
      }
      return;
    }
    cursor.set(here ? { page: here.index, x: here.x, y: here.y, clientX: e.clientX, clientY: e.clientY } : null);
    if (here && p.onInk && !d) p.onInk(here.index, here.x, here.y);
    if (d?.kind === 'hand') {
      view.scrollLeft = d.sl - (e.clientX - d.x);
      view.scrollTop = d.st - (e.clientY - d.y);
      return;
    }
    if (d?.kind === 'zoom') {
      if (Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4) d.moved = true;
      if (d.moved) setZoomBox([d.x, d.y, e.clientX, e.clientY]);
      return;
    }
    if (d?.kind === 'create' || d?.kind === 'place' || d?.kind === 'thread') {
      const el = view.querySelector<HTMLElement>(`[data-page="${d.page}"]`);
      if (!el) return;
      const r = el.getBoundingClientRect(), k = pxPerPt(el, r);
      let end: [number, number] = [(e.clientX - r.left) / k, (e.clientY - r.top) / k];
      if (d.kind === 'create') {
        // Space held: the drawing goes with the pointer (its start point moves by as much), its size stays
        const raw = end;
        if (d.space) { d.x += raw[0] - d.space[0]; d.y += raw[1] - d.space[1]; d.rx += raw[0] - d.space[0]; d.ry += raw[1] - d.space[1]; d.space = raw; }
        end = d.space ? raw : drawGrid(d.page, end, e.shiftKey ? end : pullPoint(d.snap, end));
        d.end = raw; d.keys = { shift: e.shiftKey, alt: e.altKey };
      }
      if (!d.rect && Math.hypot(end[0] - d.x, end[1] - d.y) * k < 3) return;
      const shape = d.kind === 'place' ? 'rectangleFrame' : d.kind === 'thread' ? 'textFrame' : d.shape;
      d.rect = drawRect(shape, [d.x, d.y], end, e.shiftKey, e.altKey, polygonAspect);
      if (d.kind === 'create') showCreate(d);
      else setDrawing({ page: d.page, kind: shape, rect: d.rect });
      return;
    }
    if (d?.kind === 'measure') {
      const el = view.querySelector<HTMLElement>(`[data-page="${d.page}"]`);
      if (!el) return;
      const r = el.getBoundingClientRect();
      const k = pxPerPt(el, r);
      let x1 = (e.clientX - r.left) / k, y1 = (e.clientY - r.top) / k;
      if (e.shiftKey) {
        // 45° steps
        const a = Math.round(Math.atan2(y1 - d.y, x1 - d.x) / (Math.PI / 4)) * (Math.PI / 4);
        const len = Math.hypot(x1 - d.x, y1 - d.y);
        x1 = d.x + Math.cos(a) * len;
        y1 = d.y + Math.sin(a) * len;
      }
      measureLive.set(d.other ? { page: d.page, x0: d.x, y0: d.y, x1: d.other[0], y1: d.other[1], x2: x1, y2: y1 } : { page: d.page, x0: d.x, y0: d.y, x1, y1 });
      return;
    }
    if (d?.kind === 'content') { selection.moveContent(e, d); return; }
    if (d?.kind === 'move') { selection.moveItems(e, d); return; }
    if (d?.kind === 'resize') { selection.resize(e, d); return; }
    if (d?.kind === 'rotate') { selection.rotate(e, d); return; }
    if (d?.kind === 'marquee') { selection.marquee(e, d, view); return; }
    if (p.tool === 'typePath') {
      // the cursor of the Type on a Path tool: over a path it carries a plus, over text it is the plain I-beam (InDesign 2025, typepath-c14); the same hit as a press
      const h = hitAt(e.clientX, e.clientY, true, 1);
      const kind = h?.leaf.text ? 'text' : h?.leaf.paths?.length ? 'path' : 'free';
      setTpCursor((old) => (old === kind ? old : kind));
      // InDesign 2025 outlines a rectangle or text frame whose edge the pointer is on; an oval, polygon or line is not outlined (one measurement each)
      const id = h && (h.leaf.kind === 'rectangle' || h.leaf.kind === 'textFrame') ? h.leaf.id : null;
      setHover((old) => (old?.id === id && old?.page === h?.page ? old : id && h ? { page: h.page, id } : null));
      return;
    }
    if (p.tool === 'selection' || p.tool === 'direct') {
      if (hoverFrame.current) return;
      const x = e.clientX, y = e.clientY;
      hoverFrame.current = requestAnimationFrame(() => {
        hoverFrame.current = 0;
        const h = hitAt(x, y, false, 4, true);
        const id = h ? (p.tool === 'direct' ? h.leaf.id : h.top.id) : null;
        setHover((old) => (old?.id === id && old?.page === h?.page ? old : id && h ? { page: h.page, id } : null));
      });
    }
  };

  const onPointerUp = (e: React.PointerEvent<HTMLDivElement>) => {
    text.end({ copy: e.ctrlKey || e.metaKey || e.altKey });
    const view = vp.viewRef.current!;
    const d = drag.current;
    drag.current = null;
    view.classList.remove('grabbing');
    try { if (view.hasPointerCapture(e.pointerId)) view.releasePointerCapture(e.pointerId); } catch { /* synthetic */ }
    if (transforms.up(e)) return;
    if (gradients.up(e)) return;
    if (coordinates.up(e)) return;
    if (live.up(e)) return;
    if (anchorControl.up(e, pageAt)) return;
    if (tpBrackets.up(e)) return;
    if (tableLinesTool.active()) { const at = pageAt(e.clientX, e.clientY); if (tableLinesTool.up(e, at ? [at.x, at.y] : undefined)) return; }
    if (paths.onUp(e, ownAt(e.clientX, e.clientY))) return;
    if (d?.kind === 'pagemove') {
      // the page goes by the distance the pointer went (InDesign: against everything else of its spread; the engine lays the spread out again)
      const dx = (e.clientX - d.x) / d.k, dy = (e.clientY - d.y) / d.k;
      if (Math.hypot(dx, dy) * d.k >= 3) p.onPageMove?.(d.page, Math.round(dx * 100) / 100, Math.round(dy * 100) / 100);
      return;
    }
    if (d?.kind === 'thread') {
      setDrawing(null);
      const page = p.info?.pages.find(pg => pg.index === d.page)?.id;
      const items = p.store.peek(d.page);
      if (!page || !items) return;
      const hit = d.rect ? null : hitAt(e.clientX, e.clientY);
      const done = (r: EditResult | null, id?: string) => { if (r && id) p.setSelection([{ page: d.page, id, content: false }]); };
      if (hit?.leaf.kind === 'textFrame') { const id = hit.leaf.id; p.threading.toFrame(id).then(r => done(r, id)); return; }
      if (p.threading.loaded?.port === 'in') { p.onNote(tr('들어가는 곳에서는 기존 텍스트 프레임만 이을 수 있습니다')); p.threading.cancel(); return; }
      const rect = (d.rect ?? columnFrame(items, d.x, d.y)) as [number, number, number, number];
      p.threading.toNewFrame(page, rect).then(r => done(r, r?.created?.[0]));
      return;
    }
    if (d?.kind === 'place') {
      setDrawing(null);
      if (!p.placement.files.length) return;
      const page = p.info?.pages.find(pg => pg.index === d.page)?.id;
      if (!page) return;
      const target = d.rect ? { page, rect: d.rect } : d.id ? { id: d.id } : { page, point: [d.x, d.y] as [number, number] };
      p.placement.place(target).then(result => {
        const id = result?.created?.[0] ?? d.id;
        if (result && id) p.setSelection([{ page: d.page, id, content: false }]);
      });
      return;
    }
    if (d?.kind === 'create') {
      setDrawing(null);
      if (!d.rect) {
        setDrawingSize({ page: d.page, kind: d.shape, x: d.rx, y: d.ry, centered: e.altKey }); // (a click without a drag: where the pointer was, not rounded)
        return;
      }
      const el = view.querySelector<HTMLElement>(`[data-page="${d.page}"]`);
      if (!el) return;
      const r = el.getBoundingClientRect(), k = pxPerPt(el, r);
      const raw: [number, number] = [(e.clientX - r.left) / k, (e.clientY - r.top) / k];
      const rect = drawRect(d.shape, [d.x, d.y], d.space ? raw : drawGrid(d.page, raw, e.shiftKey ? raw : pullPoint(d.snap, raw)), e.shiftKey, e.altKey, polygonAspect);
      if (d.shape === 'line' ? Math.hypot(rect[2] - rect[0], rect[3] - rect[1]) < .01 : rect[2] <= rect[0] || rect[3] <= rect[1]) return;
      if (griddable(d.shape) && (d.grid.cols > 1 || d.grid.rows > 1)) {
        const cells = gridCells(rect, d.grid);
        if (!cells.length) { p.onNote(tr('칸 사이 간격이 너무 넓어 격자를 그릴 수 없습니다.')); return; }
        void createGrid(d.page, d.shape, cells).catch(error => p.onNote(String(error)));
        return;
      }
      void createDrawing(d.page, d.shape, rect).catch(error => p.onNote(String(error)));
      return;
    }
    if (d?.kind === 'move') { selection.dropItems(e, d); return; }
    if (d?.kind === 'content') { selection.dropContent(d); return; }
    if (d?.kind === 'resize') { selection.dropResize(d); return; }
    if (d?.kind === 'rotate') { selection.dropRotation(e, d); return; }
    if (d?.kind === 'zoom') {
      setZoomBox(null);
      if (d.moved) {
        const w = Math.abs(e.clientX - d.x), h = Math.abs(e.clientY - d.y);
        const k = Math.min(view.clientWidth / Math.max(8, w), view.clientHeight / Math.max(8, h));
        vp.zoomTo(scale * k * 0.95, { clientX: (e.clientX + d.x) / 2, clientY: (e.clientY + d.y) / 2 });
      } else {
        vp.zoomTo(stepZoom(scale, d.alt || e.altKey ? -1 : 1), e);
      }
      return;
    }
    if (d?.kind === 'marquee') { selection.dropMarquee(e, d, view); return; }
  };

  const onDoubleClick = (e: React.MouseEvent) => {
    if (p.tool !== 'selection') return;
    // double-click on a port: break the thread there
    const at = pageAt(e.clientX, e.clientY);
    const port = at && portAt(p.store.peek(at.index), p.selection, at.index, [at.x, at.y], scale);
    if (port) { p.threading.unthread(port.item, port.port); return; }
    const h = hitAt(e.clientX, e.clientY, false, 4, true);
    if (!h) return;
    if (h.top.kind === 'group' && h.leaf !== h.top) { p.setSelection([{ page: h.page, id: h.leaf.id, content: false }]); return; }
    if (h.leaf.content) { p.setSelection([{ page: h.page, id: h.leaf.id, content: true }]); return; }
    if (h.leaf.kind === 'textFrame' && p.editable) {
      const here = pageAt(e.clientX, e.clientY);
      if (here) text.point(here.index, here.x, here.y, h.leaf.id);
    }
  };

  const onContextMenu = (e: React.MouseEvent) => {
    if (!p.onContextMenu) return;
    e.preventDefault();
    const guide = guides.hit(e.clientX, e.clientY, true);
    if (guide && (p.tool === 'selection' || p.tool === 'direct')) {
      guides.select(guide);
      p.onContextMenu({ x: e.clientX, y: e.clientY }, 'guide');
      return;
    }
    const h = hitAt(e.clientX, e.clientY);
    let kind: 'layoutNone' | 'layoutObject' | 'layoutGraphic' | 'text' = 'layoutNone';
    if (p.tool === 'type' || p.tool === 'typePath' || p.tool === 'typeVertical') kind = 'text';
    else if (h) {
      const direct = p.tool === 'direct';
      const id = direct ? h.leaf.id : h.top.id;
      const sel = p.selection.find((s) => s.id === id);
      if (!sel) p.setSelection([entryFor(h, direct)]);
      kind = (sel ? sel.content : direct && !!h.leaf.content) ? 'layoutGraphic' : 'layoutObject';
    } else if (p.selection.length) kind = p.selection[0].content ? 'layoutGraphic' : 'layoutObject';
    p.onContextMenu({ x: e.clientX, y: e.clientY }, kind);
  };
  const onPointerCancel = () => { transforms.cancel(); gradients.cancel(); coordinates.cancel(); live.cancel(); anchorControl.cancel(); tpBrackets.cancel(); tableLinesTool.cancel(); text.end(); drag.current = null; setGhost(null); setSmartDraw(null); setMarquee(null); setDrawing(null); };
  const onPointerLeave = () => { cursor.set(null); setHover(null); setTpCursor('free'); p.onInkLeave?.(); };

  return {
    text, hover, marquee, zoomBox, drawing, drawingSize, setDrawingSize, createDrawing, measure,
    placeLoadedCursor, ghost, smartDraw, sides, guides, paths, live, tpBrackets, tpCursor, pivot, smartCss,
    pageAt, onPointerDown, onPointerMove, onPointerUp, onPointerCancel, onPointerLeave, onDoubleClick, onContextMenu,
  };
}
