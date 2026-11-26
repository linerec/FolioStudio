import { useEditingCommands } from './useEditingCommands';
import { useViewCommands } from './useViewCommands';
import { useDocumentCommands } from './useDocumentCommands';
import { useAppState } from './useAppState';
import { useColorMenu } from '../features/color/menu';
import { useColorPanelMenu } from '../features/color/panelmenu';
import { viewMarksState,viewMarksFor } from '../features/view/marks';
import { t as tr } from '../i18n';
import { useHyperlinks } from '../features/hyperlinks/useHyperlinks';
import { useNotes } from '../features/editorial/useNotes';
import { useChanges } from '../features/editorial/useChanges';
import { usePageEditing } from '../ui/pages';
import { useLayerEditing } from '../ui/layeredit';
import { usePageHistory } from '../ui/pagehistory';
import { usePanelBusVersion } from '../ui/panelbus';
import { useObjectClear } from '../ui/objectclear';
import { useLinkMenu } from '../ui/linkmenu';
import { useTransformMenu } from '../ui/transformmenu';
import { useListOptions } from '../ui/listoptions';
import { useStoryEditor } from '../ui/storyeditor';
import { useToolboxPaint } from '../ui/toolboxpaint';
import { useColorPicker } from '../ui/colorpickerhost';
import { useNumberingLists } from '../ui/numberinglists';
import { useObjectStyleMenu } from '../ui/objectstylemenu';
import { useTableStyleMenu } from '../ui/tablestylemenu';
import { useTextStyleMenu } from '../ui/textstylemenu';
import { useStrokeStyles } from '../features/strokes/menu';
import { useLibrary } from '../features/library/menu';
import { useBook } from '../features/book/menu';
import { ZERO_POINT_LOCKED_KEY,rulerMenuCommands } from '../features/ruler/menu';
import { usePreflightProfiles } from '../features/preflight/menu';
import { useCaptions } from '../features/captions/menu';
import { useObjectOptions } from '../features/objopts/menu';
import { useMath } from '../features/math/useMath';
import { useDatabaseSource } from '../features/datamerge/db/useDatabaseSource';
import { useAnchoredObjects } from '../features/anchored/menu';
import { useTextPathMenu } from '../features/tools/textpath-menu';
import { useOutlinesMenu } from '../features/tools/outlines-menu';
import { useLastEditor } from '../ui/editwith';
import { useCallback,useEffect,useMemo,useRef,useState } from 'react';
import {
type EditCommand,
type EditResult,copyObjects,redoEdit,undoEdit
} from '../engine';
import { type ItemInfo } from '../inspect';
import { matches } from '../ui/commands';
import { useAlignment } from '../ui/alignment';
import { type PanelId } from '../ui/dock';
import { isPlaceTarget,shiftHeldAtAsk,usePlacement } from '../ui/place';
import { useThreading } from '../ui/threading';
import { useGuideEditing } from '../ui/guideediting';
import type { TableActions,TableSelection,TextActions,TextSelectionState } from '../ui/textedit';
import { type TableClip,type TableDialog } from '../ui/tables';
import { useTextFindSession } from '../ui/textfindsession';
import { useGlyphFind } from '../ui/glyphfind';
import { useTypeAids } from '../ui/typeaids';
import { useCharacterLines } from '../ui/charlines';
import { useParagraphOptions } from '../ui/paragraphoptions';
import { useGrepStyles } from '../ui/grepstyles';
import { useStoryPanel,useOpticalMarginEnabled } from '../ui/storypanel';
import type { TextFormatting } from '../ui/textformat';
import { type SelEntry,findItem } from '../ui/overlay';
import { selectedPoints,useLive } from '../ui/store';
import type { Commands } from '../ui/commands';

export function useFeatureMenus(state: ReturnType<typeof useAppState>, editing: ReturnType<typeof useEditingCommands>, viewCommands: ReturnType<typeof useViewCommands>, documents: ReturnType<typeof useDocumentCommands>, getCommands: () => Commands) {
  const {
    editState, realInfo, store,
    setSelection, info, inks,
    editable, flash, links,
    linksBusy, settings, doc,
    dock, selection,
    vp, setCurrent, setTool,
    settingsRef, current, rows,
    realDoc, selectedPage, setSelectedPage,
    selectedGuide, project, setUnitsPref,
    textWidgetRef, formatText, textStroke,
    objectStroke, setFormatText, effectiveTool,
  } = state;
  const {
    runEdit, afterEdit,
  } = editing;
  const {
    go, units, spreadItems,
    rulerOrigin,
  } = viewCommands;
  const {
    relinkNow, updateSettings,
  } = documents;
  // Swatches panel ▸ Find this Color: select the page items that use a swatch (every page's items are read)
  const editStateRef = useRef(editState);
  editStateRef.current = editState;
  const selectItems = useCallback(async (ids: string[]) => {
    const entries: SelEntry[] = [];
    for (const page of realInfo?.pages ?? []) {
      const items = store.peek(page.index) ?? await store.load(page.index);
      for (const id of ids) if (findItem(items, id)) entries.push({ page: page.index, id, content: false });
    }
    setSelection(entries);
    return entries.length;
  }, [realInfo, store]);
  const colorMenu = useColorMenu({swatches: info?.swatches ?? [], groups: info?.colorGroups ?? [], mixedGroups: info?.mixedInkGroups ?? {}, inks: inks?.inks ?? [], canEdit: editable, edit: runEdit, selectItems, flash});
  const colorPanel = useColorPanelMenu({ editable, edit: runEdit });
  const linkMenu = useLinkMenu({ links, busy: linksBusy, go, relink: () => relinkNow(settings.linkRoots), note: flash, editable, edit: runEdit });

  const textActions = useRef<TextActions | null>(null);
  const [findOpen,setFindOpen]=useState(false);
  const glyphFind=useGlyphFind();
  const panelBusVersion=usePanelBusVersion();
  const [quickApplyOpen,setQuickApplyOpen]=useState(false);
  useEffect(()=>setQuickApplyOpen(false),[doc?.path]);
  const [textReveal,setTextReveal]=useState<{story:string;start:number;end:number;allowOverset?:boolean}|null>(null);
  const textFindSession=useTextFindSession(doc?.path,setTextReveal,flash);
  useEffect(()=>{setFindOpen(false);setTextReveal(null);},[doc?.path]);
  useEffect(()=>{
    // Capture only this application command before the text editor consumes keys.
    // Platform modifier mapping stays in keys.ts through matches().
    const find=(e:KeyboardEvent)=>{
      if(editable && matches(e,'Ctrl+F') && !e.isComposing && !document.querySelector('[role="dialog"]')){
        e.preventDefault();e.stopPropagation();setFindOpen(true);
      }
    };
    window.addEventListener('keydown',find,true);return()=>window.removeEventListener('keydown',find,true);
  },[editable]);
  const [textSelection,setTextSelection]=useState<TextSelectionState|null>(null);
  const externalOpens = useRef<string[]>([]);
  const hyperlinks = useHyperlinks({info,editable,selection:textSelection,edit:runEdit,show:id=>dock.show(id,false),objects:selection.filter(s=>!s.content).map(s=>s.id),
    document:doc?.path,goto:{pages:info?.pages??[],viewport:vp,setCurrent,clearSelection:()=>setSelection([]),revealText:setTextReveal,note:flash,
      openExternal:async url=>{const r=await window.folio.openExternal(url);if(r.ok)externalOpens.current.push(r.url??url);return r;}}});
  const notes = useNotes({info,editable,selection:textSelection,user:String(settings.preferences?.['app.userName']||'Unknown User Name'),edit:runEdit,show:()=>dock.show('notes',false),reveal:setTextReveal});
  const changes = useChanges({info,editable,selection:textSelection,edit:runEdit,show:()=>dock.show('changes',false),reveal:setTextReveal});
  const [tableSel, setTableSel] = useState<TableSelection | null>(null);
  // rows or columns copied with Edit ▸ Copy (Table ▸ Paste Before / Paste After); gone with the document
  const [tableClip, setTableClip] = useState<TableClip | null>(null);
  useEffect(() => setTableClip(null), [doc?.path]);
  const tableActions = useRef<TableActions | null>(null);
  const pathFinish = useRef<(() => void) | null>(null);
  const [tableDialog, setTableDialog] = useState<TableDialog | null>(null);
  // Edit ▸ Copy / Cut / Paste of objects: the engine keeps what was copied across the documents
  // it opens (server clip.rs; pasting in another brings its swatches, styles and graphics)
  const [objectClip, setObjectClip] = useState(false);
  const opticalMarginEnabled = useOpticalMarginEnabled(textSelection?.story, info);
  const textFormatting:TextFormatting={selection:editable?textSelection:null,info,opticalMarginEnabled,
    apply:props=>textActions.current?.format(props),
    preview:textActions.current?.preview??null,
    style:(kind,id,clear)=>textActions.current?.style(kind,id,clear),
    insertGlyph:(text,props)=>textActions.current?.insertGlyph(text,props),
    insertGlyphById:(family,style,glyph)=>textActions.current?.insertGlyphById(family,style,glyph)};
  const paragraphOptions = useParagraphOptions(textFormatting, { documentPath: doc?.path, edit: runEdit });
  const grepStyles = useGrepStyles(textFormatting, { path: doc?.path, edit: runEdit });
  const storyPanel = useStoryPanel(textFormatting, { documentPath: doc?.path, target: editable && selection.length === 1 && !selection[0].content ? selection[0].id : undefined, edit: runEdit });
  const typeAids = useTypeAids(textFormatting, { documentPath: doc?.path, editable, edit: runEdit, roots: settings.linkRoots });
  const characterLines = useCharacterLines(textFormatting, doc?.path);
  const numberingLists = useNumberingLists({path:doc?.path,editable,edit:runEdit});
  const listOptions = useListOptions(textFormatting,{path:doc?.path,editable,table:tableSel,stories:selection.filter(s=>!s.content).flatMap(s=>{const story=findItem(store.peek(s.page),s.id)?.item.text?.story;return story?[story]:[];}),edit:runEdit});
  const textHistory = useCallback(async (redo: boolean): Promise<EditResult | null> => {
    try { const r = await (redo ? redoEdit() : undoEdit()); afterEdit(r); return r; }
    catch (e) { flash(String((e as Error).message ?? e)); return null; }
  }, [afterEdit, flash]);
  const undo = useCallback(() => { if (textActions.current) textActions.current.history(false); else void textHistory(false); }, [textHistory]);
  /** Window ▸ History: back or forward to the state after `done` steps (0: as opened). */
  const historyGo = useCallback(async (done: number) => {
    textActions.current?.exit();
    let n = done - editStateRef.current.history.undo.length;
    while (n < 0 && await textHistory(false)) n++;
    while (n > 0 && await textHistory(true)) n--;
  }, [textHistory]);
  const redo = useCallback(() => { if (textActions.current) textActions.current.history(true); else void textHistory(true); }, [textHistory]);
  /** Window ▸ History panel menu (build-c): Delete one step (`historyDelete`) and Clear History (`historyClear`) change the undo record; the reply is shown as an edit's. */
  const historyCall = useCallback(async (message: Record<string, unknown>): Promise<EditResult | null> => {
    textActions.current?.exit();
    try { const { header } = await window.folio.request(message); if (!header.ok) throw new Error(String(header.error)); const r = header as unknown as EditResult; afterEdit(r); return r; }
    catch (e) { flash(String((e as Error).message ?? e)); return null; }
  }, [afterEdit, flash]);
  const alignment = useAlignment(selection, info, store, editable, units, runEdit);
  const placeTool = useCallback(() => setTool('selection'), []);
  const showImportOptions = settings.preferences?.['folio.place.showImportOptions'] === true;
  const placement = usePlacement(doc?.path, editable, runEdit, placeTool, flash, { show: showImportOptions,
    set: (show) => { if (show !== showImportOptions) void updateSettings({ preferences: { ...settingsRef.current.preferences, 'folio.place.showImportOptions': show } }).catch((e) => flash(String((e as Error).message ?? e))); } });
  const threading = useThreading(doc?.path, editable, runEdit);
  const beginPlacement = useCallback(async () => {
    const paths = await window.folio.placeDialog();
    const s = selection.length === 1 ? selection[0] : null;
    const f = s ? findItem(store.peek(s.page), s.id)?.item : null;
    await placement.begin(paths, f && isPlaceTarget(paths[0], f.kind) ? f.id : undefined, showImportOptions || shiftHeldAtAsk());
  }, [placement.begin, selection, store, showImportOptions]);

  /** The selected objects themselves (not placed content), outermost first. */
  const selectedIds = useCallback(() => selection.filter((s) => !s.content).map((s) => s.id), [selection]);
  // Object ▸ Convert Point is offered while anchor points are selected (measured: greyed with a whole object selected)
  const pickedPoints = useLive(selectedPoints);
  /** The one selected group, if that is the selection. */
  const selectedGroup = useCallback(() => {
    if (selection.length !== 1 || selection[0].content) return null;
    const f = findItem(store.peek(selection[0].page), selection[0].id);
    return f && f.item.kind === 'group' ? { item: f.item, page: selection[0].page } : null;
  }, [selection, store]);
  /** The frame of the one selected placed graphic (the frame itself or its content). */
  const framedId = useCallback(() => {
    if (selection.length !== 1) return null;
    const f = findItem(store.peek(selection[0].page), selection[0].id);
    return f?.item.content ? f.item.id : null;
  }, [selection, store]);
  /** Items of the current spread, nested ones too. */
  const spreadAll = useCallback(() => {
    const out: ItemInfo[] = [];
    const walk = (it: ItemInfo) => { out.push(it); it.children.forEach(walk); };
    spreadItems().forEach(({ it }) => walk(it));
    return out;
  }, [spreadItems]);
  const arrange = useCallback((how: 'front' | 'forward' | 'backward' | 'back') => {
    // several: each keeps its place among the others (front and forward from the top down)
    const z = (id: string) => { for (const s of selection) { const f = findItem(store.peek(s.page), id); if (f) return f.item.z; } return 0; };
    const ids = selectedIds().sort((a, b) => (how === 'front' || how === 'backward' ? z(a) - z(b) : z(b) - z(a)));
    const label = { front: tr('맨 앞으로 가져오기'), forward: tr('앞으로 가져오기'), backward: tr('뒤로 보내기'), back: tr('맨 뒤로 보내기') }[how];
    runEdit(label, ids.map((id) => ({ op: 'arrange' as const, id, how })));
  }, [selection, selectedIds, store, runEdit]);

  /** The middle of the view on the page nearest to it: page index and page coordinates. */
  const viewCenter = useCallback((): { page: number; x: number; y: number } | null => {
    const view = vp.viewRef.current;
    if (!view || !doc) return null;
    const v = view.getBoundingClientRect();
    const cx = (v.left + v.right) / 2, cy = (v.top + v.bottom) / 2;
    let best: { page: number; r: DOMRect; d: number } | null = null;
    for (const el of view.querySelectorAll<HTMLElement>('.page[data-page]')) {
      const r = el.getBoundingClientRect();
      const d = Math.hypot(cx - Math.min(Math.max(cx, r.left), r.right), cy - Math.min(Math.max(cy, r.top), r.bottom));
      if (!best || d < best.d) best = { page: Number(el.dataset.page), r, d };
    }
    const p = best && doc.pages[best.page];
    if (!best || !p) return null;
    const k = best.r.width / p.width;
    return { page: best.page, x: (cx - best.r.left) / k, y: (cy - best.r.top) / k };
  }, [vp, doc]);
  const copyObjectSelection = useCallback(async (cut: boolean) => {
    const ids = selectedIds();
    if (!ids.length) return;
    try {
      await copyObjects(ids);
      setObjectClip(true);
      if (cut) { await runEdit(tr('잘라내기'), [{ op: 'delete', ids }]); setSelection([]); }
    } catch (e) { flash(String((e as Error).message ?? e)); }
  }, [selectedIds, runEdit, flash]);
  /** Paste (the middle of the view), Paste in Place (the current spread), Paste Into (the selected frame). */
  const layerEditing = useLayerEditing({ info, document: doc?.path, editable, edit: runEdit });
  const pasteObjects = useCallback(async (how: 'paste' | 'inPlace' | 'into') => {
    if (!doc) return;
    let page: number, command: EditCommand, label: string;
    if (how === 'into') {
      const s = selection[0];
      page = s.page; command = { op: 'paste', into: s.id }; label = tr('안쪽에 붙이기');
    } else if (how === 'inPlace') {
      page = current; command = { op: 'paste', page: doc.pages[page]?.id, rememberLayers: layerEditing.pasteRemembers }; label = tr('현재 위치에 붙이기');
    } else {
      const c = viewCenter();
      if (!c) return;
      page = c.page; command = { op: 'paste', page: doc.pages[page]?.id, at: [c.x, c.y], rememberLayers: layerEditing.pasteRemembers }; label = tr('붙이기');
    }
    const r = await runEdit(label, [command]);
    if (!r) return;
    setSelection(how === 'into' ? [{ page, id: selection[0].id, content: false }] : (r.created ?? []).map((id) => ({ page, id, content: false })));
  }, [doc, selection, current, viewCenter, runEdit, layerEditing.pasteRemembers]);

  // --- commands ------------------------------------------------------------------------
  const panelOf = (cmd: string) => cmd.slice('panel.'.length) as PanelId;
  const step = (d: number) => vp.scrollToPage(Math.max(0, Math.min((doc?.pages.length ?? 1) - 1, current + d)));
  const spreadStep = (d: number) => {
    const i = rows.findIndex((r) => r.some((p) => p.index === current));
    const r = rows[Math.max(0, Math.min(rows.length - 1, i + d))];
    if (r) vp.scrollToPage(r[0].index);
  };
  const focusPageBox = () => document.querySelector<HTMLInputElement>('.page-nav input')?.focus();
  const zoomValue = vp.scale;
  const pageEditing = usePageEditing({ doc: realDoc, info: realInfo, editable, current, selectedPage, setSelectedPage, edit: runEdit });
  const guideEditing = useGuideEditing({ editable, info, store, selected: selectedGuide, edit: runEdit });
  const objectStyleMenu = useObjectStyleMenu();
  const textStyleMenu = useTextStyleMenu();
  const tableStyleMenu = useTableStyleMenu();
  const strokeStyles = useStrokeStyles({editable,note:flash});
  const libraryMenu = useLibrary({root:project?.info.root,editable,ids:selectedIds(),page:doc?.pages[current]?.id,pageIds:(store.peek(current)?.items??[]).map(it=>it.id),text:textSelection,edit:runEdit,getCommands:()=>getCommands()});
  const bookMenu = useBook(project?.info.root,()=>getCommands());
  // right-click menus of the rulers and the ruler corner (features/ruler/menu.ts): the units and the origin of the rulers, Lock Zero Point
  const zeroPointLocked = settings.preferences?.[ZERO_POINT_LOCKED_KEY] === true;
  const rulerMenu = useMemo(() => rulerMenuCommands({ editable, hasView: !!info?.view, units, origin: rulerOrigin, locked: zeroPointLocked,
    setLocked: (on) => { void updateSettings({ preferences: { ...settingsRef.current.preferences, [ZERO_POINT_LOCKED_KEY]: on } }).catch((e) => flash(String(e))); },
    followDocumentUnits: () => setUnitsPref('document'), edit: runEdit }), [editable, info?.view, units.horizontal, units.vertical, rulerOrigin, zeroPointLocked, runEdit]);
  const preflightProfiles = usePreflightProfiles({editable,note:flash});
  const captions = useCaptions({info,editable,selection,text:textSelection,link:linkMenu.selected,store,edit:runEdit,note:flash});
  const objectOptions = useObjectOptions({editable,selection,store,edit:runEdit,page:doc?.pages[current]?.id});
  const math = useMath({editable,page:doc?.pages[current]?.id,selection,edit:runEdit});
  const databaseSource = useDatabaseSource({doc:realDoc,info,editable,edit:runEdit,flash,projectEntries:project?.tree.entries??[]});
  const anchoredObjects = useAnchoredObjects({editable,selection,store,edit:runEdit,text:textSelection,revision:editState});
  const textPathMenu = useTextPathMenu({info,editable,selection,story:textSelection?.story,edit:runEdit});
  const selectedObjects = selection.filter((s) => !s.content);
  // the widget shows the text's paint (T glyphs) with characters selected, or T on with a text frame among the selected objects
  textWidgetRef.current = editable && (!!textFormatting.selection || (formatText && selectedObjects.some((s) => !!findItem(store.peek(s.page), s.id)?.item?.text?.story)));
  const strokeFocus = textWidgetRef.current ? textStroke : objectStroke;
  // Formatting Affects Text (T) goes off when the selection changes to other objects; the same objects selected again keep it (InDesign 2025, measured with real input)
  const selectedKey = selectedObjects.map((s) => s.id).sort().join('|');
  const lastSelectedKey = useRef(selectedKey);
  useEffect(() => { if (lastSelectedKey.current !== selectedKey) { lastSelectedKey.current = selectedKey; setFormatText(false); } }, [selectedKey]);
  const toolboxPaint = useToolboxPaint({ editable, textMode: formatText, text: textFormatting, ids: selectedObjects.map((s) => s.id), strokeFocus, edit: runEdit, swatches: info?.swatches, revision: editState,
    items: selectedObjects.map((s) => findItem(store.peek(s.page), s.id)?.item).filter((it): it is NonNullable<typeof it> => !!it) });
  const toolboxPaintRef = useRef(toolboxPaint); toolboxPaintRef.current = toolboxPaint;
  // the Color Picker (double-click the Fill or Stroke chip of the toolbox or of the Color panel)
  const colorPicker = useColorPicker({ editable, textMode: formatText, text: textFormatting, ids: selectedObjects.map((s) => s.id), strokeFocus, edit: runEdit, swatches: info?.swatches, defaults: info?.defaultFillStroke,
    items: selectedObjects.map((s) => findItem(store.peek(s.page), s.id)?.item).filter((it): it is NonNullable<typeof it> => !!it) });
  // View ▸ Extras ▸ Show Link Badge (this computer's view setting): on by default
  const showLinkBadge = settings.preferences?.['folio.view.showLinkBadge'] !== false;
  // View ▸ Extras ▸ Live Corners / Anchored Object Control, Smart Guides: application settings shared with the Preferences dialog (features/view/marks.ts)
  const viewMarksSt = viewMarksState(settings.preferences);
  const viewMarks = viewMarksFor(viewMarksSt, effectiveTool);
  const linkBadges = useMemo(() => (showLinkBadge && links ? new Map(links.links.filter((l) => l.page !== null).map((l) => [`${l.page}:${l.name}`, { owner: l.owner, status: l.status }] as const)) : null), [showLinkBadge, links]);
  const storyEditor = useStoryEditor({ documentPath: doc?.path, editable, info, prefs: settings.preferences, edit: runEdit, undo, redo, reveal: setTextReveal,
    commands: () => getCommands(),
    story: (selection.length === 1 ? findItem(store.peek(selection[0].page), selection[0].id)?.item.text?.story : undefined) ?? textSelection?.story ?? null,
    selection: textSelection ? { story: textSelection.story, start: textSelection.start, end: textSelection.end } : null,
    setPref: (key, value) => { void updateSettings({ preferences: { ...settingsRef.current.preferences, [key]: value } }).catch(e => console.error('story editor setting', e)); } });
  const lastEditor = useLastEditor();
  const objectClear = useObjectClear({ selection, editable, revision: editState, peek: (pg) => store.peek(pg), edit: runEdit });
  const outlinesMenu = useOutlinesMenu({ selection, editable, inText: !!textSelection, range: textSelection && { story: textSelection.story, start: textSelection.start, end: textSelection.end, nested: textSelection.nested }, peek: (pg) => store.peek(pg), edit: runEdit });
  const transformMenu = useTransformMenu({ selection, editable, peek: (pg) => store.peek(pg), edit: runEdit });
  const pageHistory = usePageHistory(doc?.path ?? null, current, doc?.pages.length ?? 0, vp.scrollToPage);

  const selectedInGroup = (() => {
    const s = selection.length === 1 && !selection[0].content ? selection[0] : null;
    const f = s ? findItem(store.peek(s.page), s.id) : null;
    return !!f && f.path.length > 1 && f.path[f.path.length - 2].kind === 'group';
  })();
  // Layers panel ▸ Select Item(s): the objects on the panel's layers (picked, else the active one) on the
  // current spread that can be selected (visible, unlocked, on a visible unlocked layer). Select and Fit Item is
  // View ▸ Fit Selection in Window on the selected object(s).
  const layerSelectable = (() => {
    const layers = new Map((info?.layers ?? []).map((l) => [l.id, l]));
    const want = new Set(layerEditing.targetLayers);
    return spreadItems().filter(({ it }) => it.layer && want.has(it.layer) && it.visible && !it.locked && layers.get(it.layer)?.visible && !layers.get(it.layer)?.locked);
  })();
  return {
    textSelection, glyphFind, setFindOpen,
    textActions, notes, hyperlinks,
    changes, pageEditing, layerEditing,
    layerSelectable, objectStyleMenu, textStyleMenu,
    tableStyleMenu, objectClear, outlinesMenu,
    linkMenu, captions, objectOptions,
    math, anchoredObjects, textPathMenu,
    colorMenu, preflightProfiles, strokeStyles,
    libraryMenu, bookMenu, rulerMenu, databaseSource,
    colorPanel, alignment, transformMenu,
    tableSel, tableActions, setTableDialog, pathFinish,
    tableClip, selectedIds, pickedPoints,
    beginPlacement, setTableClip, copyObjectSelection,
    objectClip, pasteObjects, undo,
    redo, lastEditor, spreadAll,
    selectedGroup, framedId, arrange,
    step, spreadStep, focusPageBox,
    pageHistory, selectedInGroup, zoomValue,
    showLinkBadge, typeAids, characterLines,
    listOptions, numberingLists, storyEditor,
    guideEditing, textFindSession, setQuickApplyOpen,
    paragraphOptions, grepStyles, storyPanel,
    textFormatting, panelOf, toolboxPaintRef,
    placement, editStateRef, externalOpens,
    setTextReveal, colorPicker, historyGo, historyCall,
    toolboxPaint, strokeFocus, viewMarks,
    viewMarksSt, linkBadges, textReveal,
    setTextSelection, setTableSel, textHistory,
    zeroPointLocked, threading, findOpen,
    quickApplyOpen,
    tableDialog,
  };
}
