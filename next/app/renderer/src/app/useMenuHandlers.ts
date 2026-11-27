import { TEXT_VARIABLES, SPECIAL_CHARACTERS, TYPE_SIZES, NOT_YET, READ_ONLY } from './menuDefaults';
import { anchorOf } from './shared';
import { viewCommands } from '../features/view/commands';
import { viewMarkCommands } from '../features/view/marks';
import { conveyorActions } from '../ui/conveyor';
import { windowCommands } from '../features/window/panels';
import { t as tr } from '../i18n';
import { selectAllGuidesRequest } from '../ui/store';
import { footnoteCommands } from '../features/footnotes/commands';
import { declaredPanelCommands,panelCommand } from '../ui/panelbus';
import { lastStepRepeat } from '../ui/steprepeat';
import { textWrapMenu } from '../ui/textwrapmenu';
import { scaleAdjust } from '../ui/transformmenu';
import { characterMenu } from '../ui/charmenu';
import { paragraphComposerMenu } from '../ui/parcomp';
import { textKeyCommands } from '../ui/textkeys';
import { wordSpaceCommands } from '../ui/wordspace';
import { editWithHandlers } from '../ui/editwith';
import { type TransformKind } from '../ui/transformdialogs';
import { preferenceCommands } from '../features/preferences/commands';
import { preferenceExtraCommands } from '../features/preferences/extra-commands';
import { spellingCommands } from '../features/spelling/commands';
import { openUserDictionary } from '../features/spelling/spelling';
import { applyShortcuts } from '../features/preferences/shortcuts';
import { actionLabel,setLanguage } from '../i18n';
import {
type EditCommand,textStory
} from '../engine';
import { type ItemInfo } from '../inspect';
import { openNewProject } from '../project';
import { type Command,type Commands } from '../ui/commands';
import { FOLIO_HIDES,PANEL_META,sidebarOpen,toggleSidebar,type PanelId } from '../ui/dock';
import { ZOOM_STOPS,stepZoom } from '../ui/docview';
import { spreadGuides } from '../ui/guides';
import { cornerTargets } from '../ui/corners';
import { blendTargets } from '../ui/effects';
import { type FxPage } from '../ui/effectsdialog';
import { frameTargets } from '../ui/textframeoptions';
import { tableClipOf,tableCommands } from '../ui/tables';
import { shapeCommands } from '../ui/shapes';
import { glyphFindCommands } from '../ui/glyphfind';
import { SPECS } from '../ui/menudef';
import { findItem } from '../ui/overlay';
import { displayPerformanceMenu,IGNORE_LOCAL_KEY } from '../ui/displayperf';
import type { RefObject } from 'react';
import { useHandlerRegistry } from '../ui/handlers';
import {ai, runningLabel, useTurn} from '../ai/runtime';
import {askDocument,type Spec} from '../ai/attachments';
import type { AppContext } from './context';
import { presentationKeyCommands } from '../ui/presentationkeys';
import { pathKeyCommands } from '../ui/pathkeys';
import { spotKeyCommands } from '../ui/spotkeys';
import { focusControlPanel, focusLastPanelField, rememberPanelFields } from '../features/window/panelfocus';

export function useMenuHandlers(context: AppContext): RefObject<Commands> {
  const turn=useTurn();
  const {
    textSelection, selection, store,
    doc, screenMode, setScreenMode,
    contentGrabber, setContentGrabber, editable,
    info, runEdit, dock,
    glyphFind, setFindOpen, flash,
    textActions,
    notes,
    hyperlinks, changes, pageEditing,
    layerEditing, layerSelectable, setSelection,
    fitSelection, objectStyleMenu, textStyleMenu,
    tableStyleMenu, objectClear, outlinesMenu,
    linkMenu, captions, objectOptions,
    math, anchoredObjects, textPathMenu,
    colorMenu, preflightProfiles, strokeStyles,
    libraryMenu, bookMenu, rulerMenu,
    colorPanel, preflightMenu, infoMenu,
    panelOptions, masterView, proof,
    viewDisplay, setViewDisplayChoice, allowLocalDisplay,
    updateSettings, settings, alignment,
    transformMenu, tableSel, tableActions,
    setTableDialog, tableClip, selectedIds,
    pickedPoints, newDocument, setPresetsOpen,
    setImportSource, open, beginPlacement,
    closeDocument, project, setProject,
    resetDocument, setStatus, save,
    directOpen, saveCopy, editState,
    revert, preflight,
    setIdmlExport,
    selectAll, setTableClip, copyObjectSelection,
    objectClip, pasteObjects,
    undo, redo, setDialog,
    links, lastEditor, realInfo,
    spreadAll, refPoint, openTransformDialog,
    setFxDialog, setLightDialog, selectedGroup,
    framedId, arrange, chooseTheme,
    theme, setUnitsPref, unitsPref,
    vp, step, spreadStep,
    focusPageBox, pageHistory, setTextMode,
    inks, soloPlate, setSep,
    selectContainer, selectContent, selectStep,
    selectedInGroup, groupStep, sepSupported,
    sep, zoomValue, current,
    setPresenting, presenting, setRulers,
    rulers, setEdges, edges,
    settingsRef, setShowNotes, showNotes,
    showLinkBadge, setHiddenChars, hiddenChars,
    setThreads, threads, setGuides,
    guides, setShortcutDialog,
    setSettings, setSpellOpen,
    setSpreads, spreads, toggleShowHidden,
    showHidden, workspaces, setShowTools,
    showTools, setShowControl, showControl,
    setAllHidden, allHidden, appWins, workbench,
    addRoot, relinkNow, rerunPreflight,
    openSample, typeAids, characterLines,
    listOptions, numberingLists, storyEditor,
    guideEditing, textFindSession, setQuickApplyOpen,
    flattener,
    paragraphOptions, grepStyles, storyPanel,
    setParagraphDecorationsOpen, setDropCapsOpen, textFormatting,
    pdfComments, pdfImport, recentProjects,
    presets, readOnly, units, pathFinish, tool, setPresentationBackground,
    setTextReveal,
  } = context;
  // Layout ▸ Table of Contents (…Styles): the engine's dialog, starting from the document's first TOC style
  const openToc = (dialog: 'toc' | 'tocStyles') => void window.folio.request({ cmd: 'openDialog', dialog,
    values: { style: info?.tocStyles?.[0]?.id ?? '', namePrefix: tr('새 목차'), titleDefault: tr('목차') } }).then(({ header }) => { if (!header.ok) flash(String(header.error)); });

    const tocStory = textSelection?.story ?? (selection.length===1 ? findItem(store.peek(selection[0].page),selection[0].id)?.item.text?.story : undefined);
    // Type ▸ Type Orientation: the story's orientation as the engine reports it on its frames (pageItems `text.orientation`)
    const typeOrientationCommands = () => {
      let orientation: string | undefined;
      for (const pg of info?.pages ?? []) {
        const it = tocStory ? store.peek(pg.index)?.items.find((i) => i.text?.story === tocStory) : undefined;
        if (it) { orientation = it.text?.orientation ?? 'Horizontal'; break; }
      }
      const set = (to: 'Horizontal' | 'Vertical') => editable && tocStory && !(textSelection && textSelection.nested)
        ? () => void runEdit(to === 'Vertical' ? tr('세로 문자 방향') : tr('가로 문자 방향'), [{ op: 'setStoryOrientation', story: tocStory, orientation: to }]) : undefined;
      return {
        'type.orientationHorizontal': { run: set('Horizontal'), checked: orientation === 'Horizontal' },
        'type.orientationVertical': { run: set('Vertical'), checked: orientation === 'Vertical' },
      };
    };
    const has = !!doc;
    // File ▸ Export PDF is the engine's document dialog `pdfExport` (a preset asked for: File ▸ PDF Presets ▸ <preset>)
    const openPdfExport = (preset?: string) => void window.folio.request({ cmd: 'openDialog', dialog: 'pdfExport', values: {
      ...(preset ? { preset } : {}), ...(project ? { project: project.info.root } : {}), preflightErrors: preflight?.summary.errors ?? 0 } })
      .then(({ header }) => { if (!header.ok) flash(String(header.error)); });
    // File ▸ Print is the engine's document dialog `printing` (pages handed in: Layout ▸ Pages ▸ Print Pages; a preset asked for: File ▸ Print
    // Presets ▸ [Default]; Window ▸ Output ▸ Flattener Preview's Apply Settings to Print: its flattener preset over the preset's)
    const openPrint = (more: Record<string, unknown> = {}) => void window.folio.request({ cmd: 'openDialog', dialog: 'printing', values: {
      newName: tr('새 사전 설정'), ...(flattener.printApply ?? {}), ...more } }).then(({ header }) => { if (!header.ok) flash(String(header.error)); });
  return useHandlerRegistry([
    viewCommands({ mode: screenMode, setMode: setScreenMode, contentGrabber, toggleGrabber: () => setContentGrabber(v => !v), editable, info, edit: runEdit }),
    windowCommands(dock),
    glyphFindCommands(glyphFind, textSelection, () => setFindOpen(true), flash),
    footnoteCommands(editable, textSelection, textActions, () => void window.folio.request({ cmd: 'openDialog', dialog: 'footnoteOptions' }), () => void window.folio.request({ cmd: 'openDialog', dialog: 'convertNotes' })),
    glyphFindCommands(glyphFind, textSelection, () => setFindOpen(true), flash),
    {
      // the engine session holds the dialog (next/shared/dialogs/endnoteOptions.json)
      'id.type.documentEndnoteOptions': { run: editable ? () => void window.folio.request({ cmd: 'openDialog', dialog: 'endnoteOptions' }) : undefined },
      // the engine session holds the dialog (next/shared/dialogs/toc.json, tocStyles.json): the first TOC style, and the words a new one is named with
      'layout.toc': {run: editable ? ()=>openToc('toc') : undefined},
      'id.layout.tableOfContentsStyles': {run: editable ? ()=>openToc('tocStyles') : undefined},
      // Type ▸ Fill with Placeholder Text: the engine fits the text to the story's frames (empty `text`)
      'type.placeholder': { run: editable && tocStory && !(textSelection && textSelection.nested) ? () => {
        void (async () => {
          const at = textSelection ? textSelection.end : (await textStory(tocStory)).length;
          await runEdit(tr('자리 표시자 텍스트로 채우기'), [{ op: 'fillPlaceholderText', story: tocStory, at, text: '' }]);
        })();
      } : undefined },
      'id.layout.updateTableOfContents': {run: editable && tocStory && info?.tocStories?.[tocStory] ? ()=>{void runEdit(tr('목차 업데이트'),[{op:'updateTOC',story:tocStory,sources:[]}]);} : undefined},
      // Type ▸ Type Orientation ▸ Horizontal / Vertical (vtext-c2): the engine's setStoryOrientation on the story being edited or of the selected frame
      ...typeOrientationCommands(),
    },
    notes.commands,
    hyperlinks.commands,
    changes.commands,
    pageEditing.handlers,
    layerEditing.handlers,
    {
      'id.panel.layers.selectItemS': { run: layerSelectable.length ? () => setSelection(layerSelectable.map(({ page, it }) => ({ page, id: it.id, content: false }))) : undefined },
      'id.panel.layers.selectAndFitItem': { run: selection.some((s) => !s.content) ? fitSelection : undefined },
    },
    objectStyleMenu.handlers,
    textStyleMenu.handlers,
    tableStyleMenu.handlers,
    objectClear,
    outlinesMenu,
    linkMenu.handlers,
    captions.handlers,
    objectOptions.handlers,
    math.handlers,
    anchoredObjects.handlers,
    textPathMenu.handlers,
    colorMenu.handlers,
    preflightProfiles.handlers,
    strokeStyles.handlers,
    libraryMenu.handlers,
    bookMenu.handlers,
    rulerMenu,
    colorPanel.handlers,
    preflightMenu.handlers,
    infoMenu.handlers,
    panelOptions.handlers,
    textWrapMenu({ selection, editable, onParent: masterView !== null, peek: (page) => store.peek(page), edit: runEdit }),
    proof.handlers,
    displayPerformanceMenu({
        editable, hasDoc: !!doc, view: viewDisplay, setView: setViewDisplayChoice, allowLocal: allowLocalDisplay,
        setAllowLocal: (allow) => { void updateSettings({ preferences: { ...settings.preferences, [IGNORE_LOCAL_KEY]: !allow } }); },
        selection, peek: (page) => store.peek(page), edit: runEdit,
      }),
    {
      // Align panel menu: Cancel Key Object puts Align To back on the selection
      'id.panel.align.cancelKeyObject': { run: alignment.bounds === 'key' ? () => alignment.setBounds('selection') : undefined },
    },
    transformMenu.handlers,
    characterMenu({ editable, selection: textSelection, apply: (props) => textActions.current?.format(props), list: (numbers) => textActions.current?.list(numbers), restart: () => textActions.current?.restartNumbering() }),
    tableCommands({ editable, text: textSelection, cells: tableSel, actions: tableActions, edit: runEdit, open: setTableDialog, clip: tableClip, units }),
    pathKeyCommands(editable && tool === 'pen' ? () => pathFinish.current?.() : null),
    spotKeyCommands(has ? inks : null, soloPlate),
    presentationKeyCommands(presenting, setPresentationBackground),
    shapeCommands({ editable, ids: selectedIds(), compound: selectedIds().filter((id) => { const f = selection.length ? findItem(store.peek(selection[0].page), id) : null; return !!f && (f.item.outline?.length ?? 0) > 1; }), paths: (id) => { const f = selection.length ? findItem(store.peek(selection[0].page), id) : null; return f?.item.paths ? f.item.paths.map((p) => ({ open: p.open, count: p.points.length })) : null; }, points: () => pickedPoints, edit: runEdit }),
    {
      'file.new': { run: () => newDocument() },
      'id.file.documentPresets.default': { run: () => newDocument() },
      'id.file.documentPresets.define': { run: () => setPresetsOpen(true) },
      'file.newProject': { run: openNewProject },
      'file.open': { run: () => open() },
      'file.openProject': { run: async () => { const d = await window.folio.chooseFolder(tr('프로젝트 폴더 열기 (folio.project가 있는 폴더)')); if (d) open(d); } },
      'file.import': { run: async () => { const f = await window.folio.openDialog(); if (f) setImportSource(f); } },
      // another document at the same time: a window of its own, with its own engine session, undo history and lock
      'file.openInNewWindow': { run: async () => { const f = await window.folioProject.openAny(); if (f) await window.folio.newWindow(f); } },
      'file.place': { run: editable ? beginPlacement : undefined, why: !editable ? READ_ONLY() : undefined },
      'file.close': { run: has ? closeDocument : undefined },
      // tools-c1: InDesign's view keys without a menu item. New Default Document (Ctrl+Alt+N): File ▸ New's window applied at once with its
      // defaults (it stays open when they cannot be used, e.g. no project); Save All (Ctrl+Alt+Shift+S): this window's document (each app
      // window holds one document and saves its own); Close All (Ctrl+Alt+Shift+W): every other app window is asked to close as its close
      // button does, then this one; Force Redraw (Shift+F5): every page's objects are read again from the engine; Open/Close All Side-Tab
      // Panels (Ctrl+Alt+Shift+Tab): the right dock area folded to icons or unfolded
      'file.key.newDefault': { run: () => { newDocument(); window.setTimeout(() => { void window.folio.request({ cmd: 'applyDialog', scope: 'app', action: 'ok' }); }, 400); } },
      // Save All (tools-c2): every app window's document with unsaved changes (each window holds its own engine session and saves it as its
      // File ▸ Save does), then this one
      'file.key.saveAll': { run: has ? async () => {
        const v = appWins.view, r = v ? await appWins.apply({ cmd: 'state' }) as { state?: { docs?: Record<string, { dirty?: boolean; title?: string | null }> } } : null;
        const others = Object.entries(r?.state?.docs ?? {}).filter(([id, d]) => Number(id) !== v?.doc && d.dirty && d.title);
        for (const [id] of others) await appWins.apply({ cmd: 'saveDoc', doc: Number(id) });
        if (doc?.folio && !doc.folio.readOnly) await save();
        else if (others.length === 0) flash(tr('이 창의 문서는 읽기 전용이라 저장하지 않았습니다.'));
      } : undefined },
      'file.key.closeAll': { run: has ? async () => {
        const v = appWins.view, r = v ? await appWins.apply({ cmd: 'state' }) as { state?: { docs?: Record<string, unknown> } } : null;
        for (const id of Object.keys(r?.state?.docs ?? {}).map(Number).filter((id) => id !== v?.doc)) await appWins.apply({ cmd: 'closeDoc', doc: id });
        closeDocument();
      } : undefined },
      'view.key.forceRedraw': { run: has ? () => { store.invalidate((info?.pages ?? []).map((pg) => pg.index)); } : undefined },
      'window.key.sideTabs': { run: () => { dock.apply({ op: 'collapseColumn', collapsed: !dock.state.columnCollapsed }); } },
      // tools-c2: the two Type preferences turned on and off from the keys (InDesign's commands without a default key; the user setting
      // Preferences ▸ Type shows): Drag and Drop Text Editing in Layout View, Adjust Spacing Automatically When Cutting and Pasting Words
      ...Object.fromEntries(([['type.key.dragDropText', 'textEditingPreferences.dragAndDropTextInLayout', false], ['type.key.smartSpacing', 'textEditingPreferences.smartCutAndPaste', true]] as const).map(([id, key, on]) => {
        const now = settings.preferences?.[key] === undefined ? on : settings.preferences?.[key] === true;
        const name = id === 'type.key.dragDropText' ? tr('레이아웃에서 글 끌어 놓기') : tr('잘라내기·붙이기 공백 자동 조정');
        return [id, { run: () => { void updateSettings({ preferences: { ...settingsRef.current.preferences, [key]: !now } }).then(() => flash(!now ? tr('켰습니다: {0}', { 0: name }) : tr('껐습니다: {0}', { 0: name }))); }, checked: now }];
      })),
      // tools-c2: the keyboard into the last field used in a panel (Ctrl+Alt+`) or the control bar's first field (Ctrl+6)
      'window.key.lastPanelField': { run: () => { rememberPanelFields(); if (!focusLastPanelField()) flash(tr('입력칸이 있는 패널이 보이지 않습니다.')); } },
      'window.key.controlPanel': { run: () => { if (allHidden) setAllHidden(false); window.setTimeout(() => { if (!focusControlPanel()) flash(tr('컨트롤 패널에 입력칸이 없습니다.')); }, allHidden ? 120 : 0); } },
      'file.closeProject': { run: project ? async () => { await closeDocument(); setProject(null); resetDocument(); setStatus('idle'); } : undefined },
      'file.save': { run: doc?.folio && !doc.folio.readOnly ? save : undefined, why: directOpen ? READ_ONLY() : doc?.folio?.readOnly ? tr('다른 사용자가 열고 있습니다 (읽기 전용)') : undefined },
      'file.saveAs': { run: doc?.folio ? () => void saveCopy(true) : undefined, why: directOpen ? READ_ONLY() : undefined },
      'file.saveCopy': { run: doc?.folio ? () => void saveCopy(false) : undefined, why: directOpen ? READ_ONLY() : undefined },
      'file.revert': { run: doc?.folio && editState.dirty ? () => void revert() : undefined },
      'file.exportPdf': { run: has ? () => openPdfExport() : undefined },
      // (the engine's app dialog `pdfPresets`: the project's PDF presets)
      'id.file.pdfPresets.define': { run: project ? () => void window.folio.request({ cmd: 'openDialog', scope: 'app', dialog: 'pdfPresets', values: { project: project.info.root, newName: tr('새 PDF 사전 설정') } }).then(({ header }) => { if (!header.ok) flash(String(header.error)); }) : undefined },
    },

      // File ▸ PDF Presets
          Object.fromEntries(([['highQualityPrint', 'high'], ['pressQuality', 'press'], ['smallestFileSize', 'small'], ['pdfX1a2001', 'x1a'], ['pdfX32002', 'x3'], ['pdfX42008', 'x4']] as const).map(([id, preset]) =>
        [`id.file.pdfPresets.${id}`, { run: has ? () => openPdfExport(preset) : undefined }])),
    {
      'file.exportIdml': { run: doc?.folio ? () => setIdmlExport(true) : undefined, why: directOpen ? tr('{0} — IDML 내보내기는 프로젝트로 가져온 뒤에 됩니다', {0: READ_ONLY()}) : undefined },
      // (the shared dialog `package`: the engine plans it, makes it on its own thread and keeps what it made; the screen hands in the
      // words of the folder's name, the folder to start in, the app's version and the clock's offset, which only it knows)
      'file.package': { run: has ? () => void (window.folio.appInfo ? window.folio.appInfo() : Promise.resolve({ version: '', documents: '' })).catch(() => ({ version: '', documents: '' }))
        .then(i => window.folio.request({ cmd: 'openDialog', dialog: 'package', values: { namePattern: tr('{name} 폴더'), documents: i.documents, appVersion: i.version,
          utcOffsetMinutes: -new Date().getTimezoneOffset(), preflightErrors: preflight?.summary.errors ?? 0 } }))
        .then(({ header }) => { if (!header.ok) flash(String(header.error)); }) : undefined },
      'file.quit': { run: () => window.close() },
      'edit.selectAll': { run: textSelection ? ()=>textActions.current?.selectAll() : has ? selectAll : undefined },
      'edit.findChange': {run:editable?()=>setFindOpen(true):undefined},
      // only text has a clipboard so far: outside a story these stay marked unfinished (red)
      'edit.copy': textSelection ? { run: textSelection.start!==textSelection.end ? ()=>textActions.current?.clipboard('copy') : undefined }
        : tableSel ? { run: tableClipOf(tableSel) ? () => setTableClip(tableClipOf(tableSel)) : undefined }
        : { run: has && selectedIds().length ? () => void copyObjectSelection(false) : undefined },
      'edit.cut': textSelection ? { run: editable && textSelection.start!==textSelection.end ? ()=>textActions.current?.clipboard('cut') : undefined }
        : { run: editable && !tableSel && selectedIds().length ? () => void copyObjectSelection(true) : undefined },
      'edit.paste': textSelection ? { run: editable ? ()=>textActions.current?.clipboard('paste') : undefined }
        : { run: editable && objectClip && !tableSel ? () => void pasteObjects('paste') : undefined },
      'edit.pasteNoFormat': { run: editable && textSelection ? () => textActions.current?.clipboard('pastePlain') : undefined },
      'edit.pasteInPlace': { run: editable && objectClip && !textSelection && !tableSel ? () => void pasteObjects('inPlace') : undefined },
      'edit.pasteInto': { run: editable && objectClip && !textSelection && !tableSel && selection.length === 1 && !selection[0].content
        && ['rectangle', 'oval', 'polygon'].includes(findItem(store.peek(selection[0].page), selection[0].id)?.item.kind ?? '') ? () => void pasteObjects('into') : undefined },
      // Edit ▸ Duplicate copies by the offset Step and Repeat last used (ui/steprepeat.tsx)
      'edit.duplicate': { run: editable && !textSelection && !tableSel && selectedIds().length
        ? () => { const s = lastStepRepeat(); void runEdit(tr('복제'), [{ op: 'duplicate', ids: selectedIds(), by: [s.h, s.v] }]); } : undefined },
      'edit.stepRepeat': { run: editable && !textSelection && !tableSel && selectedIds().length ? () => { void window.folio.request({cmd:'openDialog',dialog:'stepRepeat',ids:selectedIds()}).then(({header})=>{if(!header.ok)flash(String(header.error));}); } : undefined },
      'edit.undo': { label: editState.undo ? tr('{0} 실행 취소', {0: actionLabel(editState.undo)}) : tr('실행 취소'), run: editable && editState.undo ? undo : undefined },
      'edit.redo': { label: editState.redo ? tr('{0} 다시 실행', {0: actionLabel(editState.redo)}) : tr('다시 실행'), run: editable && editState.redo ? redo : undefined },
    },
    Object.fromEntries(Object.entries(SPECIAL_CHARACTERS).map(([id, name]) => [id, { run: editable && textSelection ? () => textActions.current?.special(name) : undefined }])),
    {
      // Type ▸ Text Variables ▸ Define…: the list, New… and Edit… (ui/variables.tsx)
      'id.type.textVariables.define': { run: editable ? () => setDialog('variables') : undefined },
      'id.type.textVariables.convertVariableToText': { run: editable && textSelection && textSelection.variables > 0 ? () => textActions.current?.convertVariables() : undefined },
    },
    Object.fromEntries(Object.entries(TEXT_VARIABLES).map(([id, want]) => {
        const of = info?.textVariables?.filter((v) => v.kind === want.kind) ?? [];
        const variable = of.find((v) => v.name === want.name) ?? of[0];
        return [id, { run: editable && textSelection && variable ? () => textActions.current?.variable(variable.id) : undefined }];
      })),
    Object.fromEntries((['UPPERCASE', 'LOWERCASE', 'TITLECASE', 'SENTENCECASE'] as const).map((mode, k) => [
        ['id.type.changeCase.uppercase', 'id.type.changeCase.lowercase', 'id.type.changeCase.titleCase', 'id.type.changeCase.sentenceCase'][k],
        { run: editable && textSelection && textSelection.start !== textSelection.end ? () => textActions.current?.changeCase(mode) : undefined }])),
    Object.fromEntries(TYPE_SIZES.map((size) => [`id.type.size.${size}Pt`, { run: editable && textSelection ? () => textActions.current?.format({ pointSize: size }) : undefined }])),
    {
      'edit.clear': { run: textSelection ? ()=>textActions.current?.clear() : editable && selectedIds().length ? () => { runEdit(tr('지우기'), [{ op: 'delete', ids: selectedIds() }]); setSelection([]); } : undefined },
    },
    (() => {
        // right-click ▸ Stroke Weight: the selection's stroke weight (as the Stroke panel sets it)
        const ids = selectedIds(), first = selection[0] && !selection[0].content ? findItem(store.peek(selection[0].page), selection[0].id)?.item : undefined;
        const weights: [number, string][] = [[0, '0Pt'], [0.25, '025Pt'], [0.5, '05Pt'], [0.75, '075Pt'], [1, '1Pt'], [2, '2Pt'], [3, '3Pt'], [4, '4Pt'], [5, '5Pt'],
          [6, '6Pt'], [7, '7Pt'], [8, '8Pt'], [9, '9Pt'], [10, '10Pt']];
        return Object.fromEntries(weights.map(([w, k]) => [`id.ctx.layoutObject.strokeWeight.${k}`, {
          run: editable && ids.length ? () => runEdit(tr('획 두께'), [{ op: 'setPaint', ids, strokeWeight: w }]) : undefined,
          checked: !!first && Math.abs(first.stroke.weight - w) < 1e-9,
        }]));
      })(),

      // Graphics ▸ Reveal in Explorer (and the Links panel's): the selected graphic's file, as the
      // link check found it (by its name, on its page first)
          (() => {
        const s = selection.length === 1 ? selection[0] : null;
        const name = s ? findItem(store.peek(s.page), s.id)?.item.content?.link : null;
        const rows = name ? (links?.links ?? []).filter((l) => l.name === name && l.path) : [];
        const row = rows.find((l) => l.page === s!.page) ?? rows[0] ?? (linkMenu.selected?.path ? linkMenu.selected : undefined);
        const file = row && row.status !== 'embedded' ? row.path : null;
        return {
          'id.panel.links.revealInExplorer': { run: row?.path ? () => void window.folio.reveal(row.path!) : undefined },
          // Edit ▸ Go To Source: the link's source is its file, shown in the file manager (the picked link's, or the selected graphic's)
          'id.edit.goToSource': { run: file ? () => void window.folio.reveal(file) : undefined },
          // Edit ▸ Edit Original: the file opens in the program the computer has for it; once saved the link check calls the link modified
          'id.edit.editOriginal': { run: file ? () => void window.folio.openPath(file).then((err) => { if (err) flash(tr('원본을 열지 못했습니다: {0}', { 0: err })); }) : undefined },
          ...editWithHandlers(file, flash, lastEditor),
        };
      })(),

      // Allow Parent Item Overrides on Selection (items of a parent page, opened in the window)
          (() => {
        const items = masterView ? selection.map((s) => findItem(store.peek(s.page), s.id)?.item).filter((i): i is ItemInfo => !!i) : [];
        const allowed = items.length > 0 && items.every((i) => i.allow_overrides);
        const toggle = { run: () => void runEdit(tr('상위 항목 재정의 허용'), [{ op: 'allowParentOverrides' as const, ids: items.map((i) => i.id), allow: !allowed }]), enabled: editable && items.length > 0, checked: allowed };
        return { 'id.panel.pages.parentPages.allowParentItemOverridesOnSelection': toggle, 'id.ctx.layoutObject.allowParentItemOverrides': toggle };
      })(),

      // Object ▸ Primary Text Frame: an empty text frame of the parent opened in the window becomes (or stops being) the parent's primary one
          (() => {
        const frame = masterView && selection.length === 1 ? findItem(store.peek(selection[0].page), selection[0].id)?.item : undefined;
        const frames = realInfo?.parentTextFrames;
        const checked = !!frame && !!frames?.primary.includes(frame.id);
        const on = editable && !!frame?.text && !!frames?.empty.includes(frame.id);
        return { 'id.ctx.layoutObject.primaryTextFrame': { run: on ? () => void runEdit(tr('기본 텍스트 프레임'), [{ op: 'setPrimaryTextFrame' as const, id: frame!.id, primary: !checked }]) : undefined, checked } };
      })(),
    {
      'object.lock': { run: editable && selectedIds().length ? () => { runEdit(tr('잠금'), selectedIds().map((id) => ({ op: 'setItem' as const, id, locked: true }))); setSelection([]); } : undefined },
      'object.unlockAll': { run: editable && spreadAll().some((it) => it.locked) ? () => runEdit(tr('펼침면의 모두 잠금 해제'), spreadAll().filter((it) => it.locked).map((it) => ({ op: 'setItem' as const, id: it.id, locked: false }))) : undefined },
    },
    (() => {
        // Object ▸ Transform: the selection turned or mirrored about the reference point, as the
        // control panel's buttons do (oracle basic-transform-menu)
        const ids = selectedIds(), on = editable && !textSelection && !tableSel && ids.length > 0;
        const turn = (by: number) => ({ run: on ? () => runEdit(tr('회전'), [{ op: 'rotate' as const, ids, by, anchor: anchorOf(refPoint) }]) : undefined });
        const flip = (axis: 'horizontal' | 'vertical') => ({ run: on
          ? () => runEdit(axis === 'horizontal' ? tr('가로로 뒤집기') : tr('세로로 뒤집기'), [{ op: 'flip' as const, ids, axis, anchor: anchorOf(refPoint) }]) : undefined });
        // Transform Again repeats the last move, rotation, scale or flip (oracle basic-transform-again)
        const again = (sequence: boolean, individually: boolean) => ({ run: on
          ? () => runEdit(tr('다시 변형'), [{ op: 'transformAgain' as const, ids, sequence, individually }]) : undefined });
        const open = (kind: TransformKind, ok = true) => ({ run: on && ok ? () => openTransformDialog({ kind, ids }) : undefined, why: on && !ok ? tr('개체를 하나만 고르세요.') : undefined });
        return {
          'object.move': open('move'), 'id.object.transform.scale': open('scale', ids.length === 1), 'id.object.transform.rotate': open('rotate'), 'id.object.transform.shear': open('shear'),
          'object.transformAgain': again(false, false), 'object.transformSequenceAgain': again(true, false),
          'id.object.transformAgain.transformAgainIndividually': again(false, true), 'id.object.transformAgain.transformSequenceAgainIndividually': again(true, true),
          'id.object.transform.rotate90DegCw': turn(-90), 'id.object.transform.rotate90DegCcw': turn(90), 'id.object.transform.rotate180Deg': turn(180),
          'id.object.transform.flipHorizontal': flip('horizontal'), 'id.object.transform.flipVertical': flip('vertical'),
        };
      })(),
    {
      'object.hide': { run: editable && selectedIds().length ? () => { runEdit(tr('숨기기'), selectedIds().map((id) => ({ op: 'setItem' as const, id, visible: false }))); setSelection([]); } : undefined },
      'object.showAll': { run: editable && spreadAll().some((it) => !it.visible) ? () => runEdit(tr('펼침면의 모두 표시'), spreadAll().filter((it) => !it.visible).map((it) => ({ op: 'setItem' as const, id: it.id, visible: true }))) : undefined },
      'object.textFrameOptions': { run: editable && selection.length && frameTargets(selection, (pg) => store.peek(pg)).length === selection.length ? () => void window.folio.request({ cmd: 'openDialog', dialog: 'textFrameOptions', ids: frameTargets(selection, (pg) => store.peek(pg)).map((it) => it.id) }) : undefined },
    },
    (() => {
        // Object ▸ Effects: the dialog on that effect's page
        const open = (page: FxPage) => ({ run: editable && selection.length && blendTargets(selection, (pg) => store.peek(pg)).length === selection.length ? () => setFxDialog({ page }) : undefined });
        return {
          'object.effects': open('transparency'), 'id.object.effects.transparency': open('transparency'), 'object.dropShadow': open('dropShadow'),
          'id.object.effects.innerShadow': open('innerShadow'), 'id.object.effects.outerGlow': open('outerGlow'), 'id.object.effects.innerGlow': open('innerGlow'),
          'id.object.effects.bevelAndEmboss': open('bevelAndEmboss'), 'id.object.effects.satin': open('satin'), 'id.object.effects.basicFeather': open('basicFeather'),
          'id.object.effects.directionalFeather': open('directionalFeather'), 'id.object.effects.gradientFeather': open('gradientFeather'),
          'id.object.effects.globalLight': { run: editable ? () => setLightDialog(true) : undefined },
        };
      })(),
    {
      'object.cornerOptions': { run: editable && cornerTargets(selection, (pg) => store.peek(pg)).length === selection.length && selection.length ? () => setDialog('corners') : undefined },
      'object.group': { run: editable && selectedIds().length >= 2 ? () => runEdit(tr('그룹'), [{ op: 'group', ids: selectedIds() }]) : undefined },
      'object.ungroup': { run: editable && selectedGroup() ? () => {
        const g = selectedGroup()!;
        runEdit(tr('그룹 해제'), [{ op: 'ungroup', id: g.item.id, rememberLayers: layerEditing.ungroupRemembers }]).then((r) => { if (r) setSelection(g.item.children.map((c) => ({ page: g.page, id: c.id, content: false }))); });
      } : undefined },
      'object.fillProportional': { run: editable && framedId() ? () => runEdit(tr('프레임을 비율에 맞게 채우기'), [{ op: 'fit', id: framedId()!, how: 'fill' }]) : undefined },
      'object.fitProportional': { run: editable && framedId() ? () => runEdit(tr('내용을 비율에 맞게 맞추기'), [{ op: 'fit', id: framedId()!, how: 'proportionally' }]) : undefined },
      'object.fitContentFrame': { run: editable && framedId() ? () => runEdit(tr('내용을 프레임에 맞추기'), [{ op: 'fit', id: framedId()!, how: 'contentToFrame' }]) : undefined },
      'object.fitFrameContent': { run: editable && framedId() ? () => runEdit(tr('프레임을 내용에 맞추기'), [{ op: 'fit', id: framedId()!, how: 'frameToContent' }]) : undefined },
      'object.centerContent': { run: editable && framedId() ? () => runEdit(tr('내용 가운데 배치'), [{ op: 'fit', id: framedId()!, how: 'center' }]) : undefined },
      'object.bringFront': { run: editable && selectedIds().length ? () => arrange('front') : undefined },
      'object.forward': { run: editable && selectedIds().length ? () => arrange('forward') : undefined },
      'object.backward': { run: editable && selectedIds().length ? () => arrange('backward') : undefined },
      'object.sendBack': { run: editable && selectedIds().length ? () => arrange('back') : undefined },
      'edit.deselectAll': { run: has ? () => setSelection([]) : undefined },
      'theme.dark': { run: () => chooseTheme('dark'), checked: theme === 'dark' },
      'theme.mediumDark': { run: () => chooseTheme('mediumDark'), checked: theme === 'mediumDark' },
      'theme.mediumLight': { run: () => chooseTheme('mediumLight'), checked: theme === 'mediumLight' },
      'theme.light': { run: () => chooseTheme('light'), checked: theme === 'light' },
      'pref.unitsDocument': { run: () => setUnitsPref('document'), checked: unitsPref === 'document' },
      'pref.unitsMillimeters': { run: () => setUnitsPref('Millimeters'), checked: unitsPref === 'Millimeters' },
      'pref.unitsPoints': { run: () => setUnitsPref('Points'), checked: unitsPref === 'Points' },
      'layout.firstPage': { run: has ? () => vp.scrollToPage(0) : undefined },
      'layout.prevPage': { run: has ? () => step(-1) : undefined },
      'layout.nextPage': { run: has ? () => step(1) : undefined },
      'layout.lastPage': { run: has ? () => vp.scrollToPage(doc!.pages.length - 1) : undefined },
      'layout.nextSpread': { run: has ? () => spreadStep(1) : undefined },
      'layout.prevSpread': { run: has ? () => spreadStep(-1) : undefined },
      'layout.goToPage': { run: has ? focusPageBox : undefined },
      'layout.firstSpread': { run: has ? () => vp.scrollToPage(0) : undefined },
      'layout.lastSpread': { run: has ? () => vp.scrollToPage(doc!.pages.length - 1) : undefined },
      'layout.goBack': { run: has && pageHistory.canBack ? () => pageHistory.go(-1) : undefined },
      'layout.goForward': { run: has && pageHistory.canForward ? () => pageHistory.go(1) : undefined },
      'control.textMode': { run: () => setTextMode((m) => (m === 'character' ? 'paragraph' : 'character')) },
      'view.fitSelection': { run: has && selection.length ? fitSelection : undefined },
      'sep.cyan': { run: has && inks ? () => soloPlate('Cyan') : undefined },
      'sep.magenta': { run: has && inks ? () => soloPlate('Magenta') : undefined },
      'sep.yellow': { run: has && inks ? () => soloPlate('Yellow') : undefined },
      'sep.black': { run: has && inks ? () => soloPlate('Black') : undefined },
      'sep.all': { run: has && inks ? () => setSep((s) => ({ ...s, mode: 'separations', hidden: [] })) : undefined },
      'help.help': { run: () => setDialog('shortcuts') },
      'object.selectContainer': { run: selection.length ? selectContainer : undefined },
      'object.selectContent': { run: selection.length ? selectContent : undefined },
      'object.selectAbove': { run: has ? () => selectStep(1, true) : undefined },
      'object.selectNextAbove': { run: has ? () => selectStep(1, false) : undefined },
      'object.selectNextInGroup': { run: selectedInGroup ? () => groupStep(1) : undefined },
      'object.selectPrevInGroup': { run: selectedInGroup ? () => groupStep(-1) : undefined },
      'object.selectNextBelow': { run: has ? () => selectStep(-1, false) : undefined },
      'object.selectBelow': { run: has ? () => selectStep(-1, true) : undefined },
      'id.panel.separations.inkManager': { run: editable ? () => setDialog('inkManager') : undefined },
      'id.panel.swatches.inkManager': { run: editable ? () => setDialog('inkManager') : undefined },
      'id.panel.separations.showSinglePlatesInBlack': { run: has && sepSupported ? () => setSep((s) => ({ ...s, singleBlack: !s.singleBlack })) : undefined, checked: sep.singleBlack },
      'id.panel.separations.desaturateBlack': { run: has && sepSupported ? () => setSep((s) => ({ ...s, desaturate: !s.desaturate })) : undefined, checked: sep.desaturate },
      'view.overprint': { run: has && sepSupported ? () => setSep((s) => ({ ...s, overprint: !s.overprint })) : undefined, checked: sep.overprint },
      'view.zoomIn': { run: has ? () => vp.zoomTo(stepZoom(zoomValue, 1)) : undefined },
      'view.zoomOut': { run: has ? () => vp.zoomTo(stepZoom(zoomValue, -1)) : undefined },
      'view.fitPage': { run: has ? () => vp.fit('page', current) : undefined },
      'view.fitSpread': { run: has ? () => vp.fit('spread', current) : undefined },
      'view.actualSize': { run: has ? () => vp.zoomTo(1) : undefined },
      'view.pasteboard': { run: has ? () => vp.zoomTo(Math.max(ZOOM_STOPS[0], (vp.zoom.kind === 'value' ? zoomValue : zoomValue) * 0.5)) : undefined },
      'view.zoom50': { run: has ? () => vp.zoomTo(0.5) : undefined },
      'view.zoom200': { run: has ? () => vp.zoomTo(2) : undefined },
      'view.zoom400': { run: has ? () => vp.zoomTo(4) : undefined },
      // tools-c1: back to the view before the last zoom change and again (Ctrl+Alt+2); every guide of the spread selected (Ctrl+Alt+G)
      'view.key.previousView': { run: has ? () => { if (!vp.togglePrevious()) flash(tr('아직 바뀐 보기가 없습니다.')); } : undefined },
      'view.key.selectAllGuides': { run: has ? () => selectAllGuidesRequest.set(selectAllGuidesRequest.get() + 1) : undefined },
      // Go to the next / previous / first / last frame in the thread (Ctrl+Alt+PageDown / PageUp, with Shift the first and last; tools-c1):
      // with a text frame selected as an object, that frame of its thread is selected and shown
      ...(() => {
        const first = selection.length === 1 && !selection[0].content ? selection[0] : null;
        const frame = first && findItem(store.peek(first.page), first.id)?.item;
        const story = frame?.kind === 'textFrame' ? frame.text : null;
        const find = async (id: string) => {
          for (const pg of info?.pages ?? []) { const hit = findItem(await store.load(pg.index), id)?.item; if (hit) return { page: pg.index, item: hit }; }
          return null;
        };
        const go = (where: 'next' | 'previous' | 'first' | 'last') => async () => {
          if (!first || !frame?.text) return;
          let at: { page: number; item: NonNullable<typeof frame> } | null = { page: first.page, item: frame };
          for (let guard = 0; guard < 10000 && at; guard++) {
            const id = where === 'next' || where === 'last' ? at.item.text?.next : at.item.text?.previous;
            if (!id) break;
            const there = await find(id);
            if (!there) break;
            at = there;
            if (where === 'next' || where === 'previous') break;
          }
          if (!at || at.item.id === frame.id) return;
          setSelection([{ page: at.page, id: at.item.id, content: false }]);
          vp.scrollToTarget({ page: at.page, bounds: at.item.bounds });
        };
        const on = (prev: boolean) => has && !!story && !!(prev ? story.previous : story.next);
        // tools-c2: with the text cursor in a story, the engine's text layout (textLayout) says which frame holds the cursor; the cursor goes to
        // the first character of that frame of the thread (a frame the text does not reach: the end of the text laid out)
        const caret = textSelection && !textSelection.nested ? textSelection : null;
        const goText = (where: 'next' | 'previous' | 'first' | 'last') => async () => {
          if (!caret) return;
          const r = await window.folio.request({ cmd: 'textLayout', story: caret.story });
          const frames = ((r.header as { frames?: { frame: string; lines: { start: number; end: number }[] }[] }).frames ?? []);
          const at = caret.end;
          const holds = (f: typeof frames[number], end: boolean) => f.lines.some((l) => l.start <= at && (end ? at <= l.end : at < l.end));
          const here = frames.find((f) => holds(f, false)) ?? [...frames].reverse().find((f) => holds(f, true)) ?? frames[frames.length - 1];
          if (!here) return;
          let item = (await find(here.frame))?.item ?? null;
          const startId = item?.id;
          for (let guard = 0; guard < 10000 && item; guard++) {
            const id = where === 'next' || where === 'last' ? item.text?.next : item.text?.previous;
            if (!id) break;
            const there = await find(id);
            if (!there) break;
            item = there.item;
            if (where === 'next' || where === 'previous') break;
          }
          if (!item || item.id === startId) { flash(where === 'next' || where === 'last' ? tr('연결된 다음 프레임이 없습니다.') : tr('연결된 이전 프레임이 없습니다.')); return; }
          const target = frames.find((f) => f.frame === item!.id);
          const laidOut = Math.max(0, ...frames.flatMap((f) => f.lines.map((l) => l.end)));
          const first = target && target.lines.length ? Math.min(...target.lines.map((l) => l.start)) : laidOut;
          setTextReveal({ story: caret.story, start: first, end: first });
        };
        const run = (where: 'next' | 'previous' | 'first' | 'last', prev: boolean) =>
          caret && has ? () => void goText(where)() : on(prev) ? () => void go(where)() : undefined;
        return {
          'object.key.nextFrameInThread': { run: run('next', false) },
          'object.key.previousFrameInThread': { run: run('previous', true) },
          'object.key.firstFrameInThread': { run: run('first', true) },
          'object.key.lastFrameInThread': { run: run('last', false) },
        };
      })(),
      // the context menu from the keyboard (Shift+F10, the menu key): the menu the right button would open, at the middle of the selection (else of the view)
      'view.key.contextMenu': { run: has ? () => {
        const view = vp.viewRef.current?.getBoundingClientRect();
        let at = view ? { x: Math.round(view.left + view.width / 2), y: Math.round(view.top + view.height / 2) } : { x: 200, y: 200 };
        const first = selection[0], item = first && findItem(store.peek(first.page), first.id)?.item;
        const el = first && vp.viewRef.current?.querySelector<HTMLElement>(`.page[data-page="${first.page}"]`);
        if (item && el) { const r = el.getBoundingClientRect(), k = r.width / (store.peek(first.page)?.size[0] ?? r.width); at = { x: Math.round(r.left + (item.bounds[0] + item.bounds[2]) / 2 * k), y: Math.round(r.top + (item.bounds[1] + item.bounds[3]) / 2 * k) }; }
        context.setContextMenu({ ...at, kind: textSelection ? 'text' : first ? (first.content ? 'layoutGraphic' : 'layoutObject') : 'layoutNone' });
      } : undefined },
      'view.mode.presentation': { run: has ? () => setPresenting(true) : undefined, checked: presenting },
      'view.toggleMode': { run: () => setScreenMode((m) => (m === 'normal' ? 'preview' : 'normal')) },
      'view.rulers': { run: () => setRulers((r) => !r), checked: rulers },
      'view.frameEdges': { run: () => setEdges((v) => !v), checked: edges },
    },
    viewMarkCommands({ prefs: settings.preferences, set: (key, value) => { void updateSettings({ preferences: { ...settingsRef.current.preferences, [key]: value } }).catch((e) => flash(String(e))); } }),
    {
      'id.view.extras.hideNotes': { run: doc ? () => setShowNotes((v) => !v) : undefined, checked: showNotes },
      'id.view.extras.showLinkBadge': { run: doc ? () => { void updateSettings({ preferences: { ...settingsRef.current.preferences, 'folio.view.showLinkBadge': !showLinkBadge } }).catch((e) => flash(String(e))); } : undefined, checked: showLinkBadge },
      'type.hiddenChars': { run: doc ? () => setHiddenChars((v) => !v) : undefined, checked: hiddenChars },
      'view.threads': { run: doc ? () => setThreads((v) => !v) : undefined, checked: threads },
      'view.guides': { run: () => setGuides((v) => !v), checked: guides },
    },
    preferenceCommands(!!(editable && info?.view), flash),
    {
      'edit.shortcuts': { run: () => setShortcutDialog(true) },
      // the engine session holds the dialog (next/shared/dialogs/fileInfo.json); it starts from the document's metadata
      'file.fileInfo': { run: editable && info?.fileInfo ? () => void window.folio.request({ cmd: 'openDialog', dialog: 'fileInfo', scope: 'document' }) : undefined },
      // the engine holds these dialogs (next/shared/dialogs/<id>.json): Color Settings is the app's, the profiles the document's
      'edit.colorSettings': { run: () => void window.folio.request({ cmd: 'openDialog', dialog: 'colorSettings', scope: 'app' }) },
      // (the user's presets are managed in the engine's dialog; a copy is named with the screen's words)
      'id.edit.transparencyFlattenerPresets': { run: () => void window.folio.request({ cmd: 'openDialog', dialog: 'flattenerPresets', scope: 'app', values: { namePrefix: tr('새 사전 설정 {n}') } }) },
      'id.edit.assignProfiles': { run: editable && info?.colorPreferences ? () => void window.folio.request({ cmd: 'openDialog', dialog: 'assignProfiles', scope: 'document' }) : undefined },
      'id.edit.convertToProfile': { run: editable && info?.colorPreferences ? () => void window.folio.request({ cmd: 'openDialog', dialog: 'convertToProfile', scope: 'document' }) : undefined },
      // Edit ▸ Place and Link: the selection loads the Content Placer's cursor; the next click on a page places it, linked to the originals
      // (engine `conveyor` load, then `placeAndLink`; InDesign 2025 menu action 139008, measured with real input, conveyor-c4)
      'id.edit.placeAndLink': {
        // off while only locked objects are selected (InDesign 2025: a locked rectangle switches the action off)
        run: editable && selection.length && !selection.every((s) => findItem(store.peek(s.page), s.id)?.item.locked) ? () => {
          const ids = [...new Set(selection.map((s) => s.id))];
          void conveyorActions.load(ids).then(() => flash(tr('놓을 곳을 누르세요 · Esc 로 취소'))).catch((err) => flash(String((err as Error).message ?? err)));
        } : undefined,
      },
    },
    // File > User is an app dialog (next/shared/dialogs/user.json)
    preferenceExtraCommands({ openUser: () => void window.folio.request({ cmd: 'openDialog', dialog: 'user', scope: 'app' }), editable, info, edit: runEdit, transfer: mode => {
        void window.folio.transferSettings(mode).then(next => { if (next) { setSettings(next); setLanguage(next.language ?? 'system', next.systemLanguage); } }).catch(e => flash(String(e)));
      } }),
    spellingCommands({ hasDocument: !!doc, prefs: settings.preferences, openCheck: () => setSpellOpen(true), openUserDictionary: () => void openUserDictionary(flash),
        setPref: (key, value) => { void updateSettings({ preferences: { ...settingsRef.current.preferences, [key]: value } }).catch((e) => flash(String(e))); } }),

      // the document's own settings (saved in the file), not just how this window looks
          Object.fromEntries(([
        ['view.lockGuides', 'guidesLocked'], ['view.lockColumnGuides', 'columnGuidesLocked'], ['view.snapGuides', 'guidesSnap'],
        ['view.baselineGrid', 'baselineGridShown'], ['view.documentGrid', 'documentGridShown'], ['id.view.gridsAndGuides.snapToDocumentGrid', 'documentGridSnap'],
        ['id.ctx.layoutNone.gridsAndGuides.guidesInBack', 'guidesInBack'], ['id.ctx.layoutNone.gridsAndGuides.gridsInBack', 'gridsInBack'],
      ] as const).map(([id, key]) => {
        const on = !!info?.view?.[key];
        return [id, { run: editable && info?.view ? () => { void runEdit(tr('격자·안내선 설정'), [{ op: 'setViewPref', key, value: !on }]); } : undefined, checked: on }];
      })),
    {
      'id.view.gridsAndGuides.deleteAllGuidesOnSpread': { run: editable && doc ? () => { void spreadGuides(store, info, current).then((ids) => { if (ids.length) void runEdit(tr('펼침면의 모든 안내선 삭제'), ids.map((id) => ({ op: 'deleteGuide', id }) as EditCommand)); }); } : undefined },
      'view.spreads': { run: () => setSpreads((v) => !v), checked: spreads },
      'id.view.showHiddenSpreads': { run: doc ? toggleShowHidden : undefined, checked: showHidden },
    },
    workspaces.handlers,
    {
      // Window ▸ Tools: InDesign Classic's tools down the left edge; in the Folio workspace the tools view (a dock panel, uix-d2), shown
      // where it is (closed: back at its home in the left area) or hidden as the activity bar hides it
      'window.tools': workbench.layout === 'folio'
        ? { run: () => { setAllHidden(false); if (!allHidden && dock.showing('toolbox')) dock.hideView('toolbox'); else dock.showView('toolbox'); }, checked: !allHidden && dock.showing('toolbox') }
        : { run: () => setShowTools((v) => !v), checked: showTools },
      'window.control': { run: () => setShowControl((v) => !v), checked: showControl },
      'window.hidePanels': { run: () => setAllHidden((v) => !v), checked: allHidden },
      // Window ▸ Sidebar (Ctrl+Alt+5; not Ctrl+B, InDesign's Text Frame Options): the Folio workspace's sidebar (the left dock area,
      // uix-d2) folded to the activity bar or opened
      'window.sidebar': workbench.layout === 'folio'
        ? { run: () => { if (allHidden) { setAllHidden(false); if (!sidebarOpen(dock, FOLIO_HIDES)) toggleSidebar(dock, FOLIO_HIDES); } else toggleSidebar(dock, FOLIO_HIDES); }, checked: !allHidden && sidebarOpen(dock, FOLIO_HIDES) }
        : { why: tr('사이드바는 Folio 작업 영역에 있습니다. 창 ▸ 작업 영역 ▸ Folio (기본)을 고르세요.') },
    },

      // Window > Arrange: the windows of the document, minimize (ui/windows.ts)
          appWins.handlers,
    {
      'id.window.history': { run: () => dock.show('history'), checked: dock.isOpen('history') },
      'id.window.styles.cellStyles': { run: () => dock.show('cellStyles'), checked: dock.isOpen('cellStyles') },
      'id.window.styles.tableStyles': { run: () => dock.show('tableStyles'), checked: dock.isOpen('tableStyles') },
      'help.about': { run: () => setDialog('about') },
      'help.shortcuts': { run: () => setDialog('shortcuts') },
      // panel menus
      'links.addRoot': { label: tr('링크 검색 폴더 추가…'), run: has ? addRoot : undefined },
      'links.refresh': { label: tr('링크 다시 찾기'), run: has ? () => relinkNow(settings.linkRoots) : undefined },
      'preflight.rerun': { label: tr('다시 검사'), run: has ? rerunPreflight : undefined },
      'panel.none': { label: tr('패널 옵션…'), why: NOT_YET() },
      'help.limits': { run: () => setDialog('limits') },
      'help.credentialStoreTest': { run: () => setDialog('credentialStoreTest') },
      'help.sample': { run: () => { void openSample(); } },
      'help.magazineSample': { run: () => { void openSample('magazine'); } },
      'recent.none': { label: tr('(없음)') },
      'pref.language': { run: () => void window.folio.request({ cmd: 'openDialog', dialog: 'language' }) },
    },
    typeAids.handlers,
    characterLines.handlers,
    listOptions.handlers,
    numberingLists.handlers,
    storyEditor.handlers,
    guideEditing.handlers,
    {
      'edit.findNext': {run:editable&&textFindSession.ready?()=>void textFindSession.next():undefined},
      // tools-c1: InDesign's Find/Change load keys (text context): the selected text into Find What / Change To, Replace (and Find Next)
      ...(()=>{const range=textSelection&&textSelection.end>textSelection.start?{story:textSelection.story,start:textSelection.start,end:textSelection.end}:null;
        return {
          'edit.key.loadFind': {run:editable&&range?()=>void textFindSession.load(range,'find'):undefined},
          'edit.key.loadFindNext': {run:editable&&range?()=>void textFindSession.load(range,'find',true):undefined},
          'edit.key.loadChange': {run:editable&&range?()=>void textFindSession.load(range,'change'):undefined},
          'edit.key.replace': {run:editable&&textFindSession.ready?()=>void textFindSession.replace(range,runEdit):undefined},
          'edit.key.replaceFindNext': {run:editable&&textFindSession.ready?()=>void textFindSession.replace(range,runEdit,true):undefined},
          // tools-c2: Ctrl+Alt+Enter turns the search direction of the Find Next keys (inside the Find/Change window its own buttons)
          'edit.key.findDirection': {run:has?()=>void textFindSession.toggleDirection():undefined},
        };})(),
      'id.ctx.text.clearAllOverrides': {run:editable&&textSelection?()=>textActions.current?.clearOverrides():undefined},
      'edit.quickApply': {run:()=>setQuickApplyOpen(true)},
      'file.print': {run:has?()=>openPrint():undefined},
      'id.layout.pages.printPages': {run:has&&pageEditing.ids.length?()=>openPrint({pageIds:[...pageEditing.ids]}):undefined},
      // (the app dialog `printPresets`: the user's print presets, a list of its own until OK)
      'id.file.printPresets.define': {run:()=>void window.folio.request({cmd:'openDialog',dialog:'printPresets',scope:'app',values:{namePrefix:tr('새 사전 설정')}}).then(({header})=>{if(!header.ok)flash(String(header.error));})},
      // (the shared dialog `booklet`: the engine plans, writes and prints it)
      'id.file.printBooklet': {run:has?()=>void window.folio.request({cmd:'openDialog',dialog:'booklet'}).then(({header})=>{if(!header.ok)flash(String(header.error));}):undefined},
      'id.file.printPresets.default': {run:has?()=>openPrint({preset:'[Default]'}):undefined},
    },
    paragraphOptions.handlers,
    grepStyles.handlers,
    storyPanel.handlers,
    {
      'id.panel.paragraph.paragraphRules': {run:editable&&textSelection?()=>setParagraphDecorationsOpen('rules'):undefined},
      'id.panel.paragraph.paragraphBordersAndShading': {run:editable&&textSelection?()=>setParagraphDecorationsOpen('boxes'):undefined},
      'id.panel.paragraph.dropCapsAndNestedStyles': {run:editable&&textSelection?()=>setDropCapsOpen(true):undefined},
    },
    paragraphComposerMenu(editable, textSelection, (props) => textActions.current?.format(props)),

      // keys that have no menu item (InDesign's shortcut editor only): text size, leading, kerning, baseline, scales, bold/italic, alignment (ui/textkeys.ts)
          textKeyCommands(textFormatting, info?.view),
          wordSpaceCommands(textSelection && { story: textSelection.story, start: textSelection.start, end: textSelection.end }, info?.view?.kerningKeyIncrement ?? 20, editable, runEdit),

      // Ctrl+. / Ctrl+, scale the selected objects 1 % (Alt: 5 %) about the reference point (InDesign's shortcut editor: Increase/Decrease scale by 1% / 5%)
          (() => {
        const ids = selectedIds(), on = editable && !textSelection && !tableSel && ids.length > 0;
        const scale = (f: number) => ({ fallThrough: true, run: on ? () => runEdit(tr('크기 조정'), ids.map((id) => ({ op: 'scale' as const, id, sx: f, sy: f, anchor: anchorOf(refPoint), adjust: scaleAdjust() }))) : undefined });
        return { 'object.key.scaleUp': scale(1.01), 'object.key.scaleDown': scale(0.99), 'object.key.scaleUp5': scale(1.05), 'object.key.scaleDown5': scale(0.95) };
      })(),
      // Increase / Decrease size by 1 % / 5 % (InDesign's shortcut editor, no default key; tools-c1): the frame's own width and height about the
      // reference point, as the control bar's W and H fields change them (the content keeps its size)
          (() => {
        const sel = selection.filter((s) => !s.content), on = editable && !textSelection && !tableSel && sel.length > 0;
        const size = (f: number) => ({ fallThrough: true, run: on ? () => runEdit(tr('크기 조정'), sel.flatMap((s) => {
          const it = findItem(store.peek(s.page), s.id)?.item;
          if (!it) return [];
          const m = it.transform, w = (it.inner[2] - it.inner[0]) * Math.hypot(m[0], m[1]), h = (it.inner[3] - it.inner[1]) * Math.hypot(m[2], m[3]);
          return [{ op: 'resize' as const, id: s.id, size: [w * f, h * f] as [number, number], anchor: anchorOf(refPoint), includeStroke: false }];
        })) : undefined });
        return { 'object.key.sizeUp': size(1.01), 'object.key.sizeDown': size(0.99), 'object.key.sizeUp5': size(1.05), 'object.key.sizeDown5': size(0.95) };
      })(),
    {
      // File ▸ Import PDF Comments…: InDesign's file window, its five warnings, the panel that opens and the notice, one undo step; the IDML it
      // makes is InDesign's for the measured document (oracle/pdfcomments-c3-ui-check.py compares them). A PDF outside the project is copied in first.
      'id.file.importPdfComments': {run:editable&&pdfComments.available?pdfImport.start:undefined},
      // Equations: Object ▸ Insert MathML… (features/math: the engine lays the MathML out and draws it as vector outlines) and Window ▸ Math Expressions (the panel in features/window/panels.tsx)
      // Window ▸ Output ▸ Flattener Preview is built (features/flattener, rules/windowmenu-c2.ko.md)
      'id.window.output.flattenerPreview': {run:()=>dock.show('flattenerPreview'),checked:dock.isOpen('flattenerPreview')},
    },
  ], (h) => {
    // the menu items of the panels that need the panel's own state (ui/panelbus.ts): on while the panel is open and offers them
    for (const id of declaredPanelCommands()) { const o = panelCommand(id); h[id] = o ? { run: () => panelCommand(id)?.run(), checked: o.checked } : {}; }
    for (const id of Object.keys(PANEL_META) as PanelId[]) {
      h[`panel.${id}`] = { run: () => dock.show(id), checked: dock.isOpen(id) };
    }
    const askSelection = (fromCanvas = false) => {
      let specs:Spec[];
      if (!fromCanvas && document.activeElement?.closest('.pp-page')) {
        specs=(doc?.pages??[]).filter(p=>pageEditing.ids.includes(pageEditing.idOf(p))).map(p=>({kind:'page',page:p.index}));
      } else if (textSelection && textSelection.start!==textSelection.end) {
        const {story,start,end}=textSelection;
        specs=[{kind:'text',story,page:selection.find(s=>findItem(store.peek(s.page),s.id)?.item.text?.story===story)?.page??current,
          start:Math.min(start,end),end:Math.max(start,end)}];
      } else specs=selection.map(s=>({kind:'objects',page:s.page,ids:[s.id]}));
      askDocument(specs);
    };
    h['ai.askSelection']={label:tr('AI에게 묻기'),run:()=>askSelection(true)};
    h['panel.ai'] = { run: askSelection, checked: !allHidden && dock.showing('ai') };
    h['ai.showComposer']={run:()=>{
      setAllHidden(false);
      dock.showView('ai');
    }};
    h['ai.settings'] = { run: () => void ai('openDialog',{dialog:'aiSettings',scope:'app'}).catch(e=>flash(String(e))) };
    // in the Folio workspace the project is the sidebar's explorer, not a dock panel: Window ▸ Project shows it and puts the keys in it
    if (workbench.layout === 'folio') {
      // (the explorer view wherever it is docked or floats: it opens, and the keys go to its tree)
      h['panel.project'] = { run: () => { setAllHidden(false); dock.showView('explorer'); window.setTimeout(() => document.querySelector<HTMLElement>('.sb-explorer .ex-tree')?.focus(), 0); }, checked: !allHidden && dock.showing('explorer') };
    }
    recentProjects.forEach((p, i) => { h[`recent.${i}`] = { label: p.name, run: () => open(p.root) }; });
    presets.forEach((p, i) => { h[`docpreset.${i}`] = { label: `${p.name}…`, run: () => newDocument(p.name) }; });
    // A command without a handler is unavailable.
    const all: Commands = {};
    for (const [id, spec] of Object.entries(SPECS)) {
      const x = h[id];
      all[id] = x
        ? ({ label: spec.label, keys: spec.keys, why: readOnly && /^(edit|object|type|table|layout)\./.test(id) && !x.run ? READ_ONLY() : undefined, ...x } as Command)
        : { label: spec.label, keys: spec.keys, why: NOT_YET() };
    }
    for (const [id, x] of Object.entries(h)) if (!all[id]) all[id] = { label: x.label ?? id, ...x } as Command;
    if (turn) {
      for (const [id,cmd] of Object.entries(all)) {
        const editing = /^(?:id\.)?(?:object|type|table)\./.test(id) || /^(?:id\.)?edit\.(?:undo|redo|cut|paste|clear|delete|duplicate|stepAndRepeat)/i.test(id);
        const selecting = /(?:select|selection)/i.test(id);
        if ((editing && !selecting) || (!cmd.run && /^(?:id\.)?(?:edit|object|type|table|layout)\./.test(id))) all[id] = {...cmd,run:undefined,why:runningLabel()};
      }
    }
    return applyShortcuts(all, settings.shortcuts);
  });
}
