import { useEffect, useState } from 'react';
import { PolygonDialog } from '../ui/drawing-dialogs';
import { PreviewEditContext,PreviewScopeContext } from '../ui/controls/usePreviewEdit';
import { transformDialogFor } from '../ui/transformtools';
import { EyedropperDialog } from '../ui/eyedropper';
import { PencilPrefsDialog,pencilPrefs,withPencilPrefs } from '../ui/pencilprefs';
import { t as tr } from '../i18n';
import { DialogFrame } from '../ui/dialog-frame';
import { PdfCommentsAlert } from '../features/pdfcomments/panel';
import { STYLE_OVERRIDES_KEY } from '../features/view/styleoverrides';
import { LinkFileDialog } from '../ui/linkxmp';
import { DocumentPresetsDialog,NewDocumentNotice } from '../ui/newdoc';
import '../features/preferences/preferences.css';
import { SpellCheckDialog } from '../features/spelling/spell-check-dialog';
import { DYNAMIC_KEY,cleanUserDictionaries,openUserDictionary,spellOptions } from '../features/spelling/spelling';
import { shortcutCommandList } from '../features/preferences/shortcuts';
import { formatLength,parseLength } from '../inspect';
import {
IdmlExportDialog,
ImportDialog,
LockBanner,
StartScreen,openNewProject,
ProjectHome,documentPath
} from '../project';
import { ControlPanel } from '../ui/control';
import { FloatingControl, useControlBar } from '../ui/controlbar';
import { ActivityBar,CLASSIC_HIDES,DockArea,DockColumn,DockStrip,FOLIO_HIDES,FloatingPanels,type PanelHost,type PanelId } from '../ui/dock';
import { DocWindow,fileName } from '../ui/docview';
import { CornerOptionsDialog,cornerTargets } from '../ui/corners';
import { blendTargets } from '../ui/effects';
import { EffectsDialog,GlobalLightDialog } from '../ui/effectsdialog';
import { LinksPanelOptionsDialog,RelinkExtensionDialog } from '../ui/linkdialogs';
import { CellOptionsDialog,CreateTableDialog,GoToRowDialog,InsertDialog,TableOptionsDialog } from '../ui/tables';
import { TextFindDialog } from '../ui/textfind';
import { QuickApply } from '../ui/quickapply';
import { VariablesDialog } from '../ui/variables';
import { Icon } from '../ui/icons';
import { MenuBar,PopupMenu } from '../ui/menu';
import { findItem } from '../ui/overlay';
import { InkManagerDialog } from '../ui/inkmanager';
import { LimitsDialog } from '../ui/limits';
import { CredentialStoreTestDialog } from '../features/datamerge/db/CredentialStoreTestDialog';
import { AppTabs } from '../ui/apptabs';
import { WindowHost } from '../ui/winhost';
import { ToolsPanel, toolDef, toolNote, type ToolId, type ToolsLayout } from '../ui/tools';
import { ToolHelpContext, ToolHelpSection, type ToolHelp } from '../ui/toolhelpview';
import { helpEntry, isToolHelp, type HelpId } from '../ui/toolhelp';
import { ExplorerView, ToolboxView, type ExplorerActions } from '../ui/sidebar';
import type { Commands } from '../ui/commands';
import type { MenuEntry } from '../ui/menudef';
import { ProofSetupDialog } from '../ui/proofsetup';
import { ParagraphDecorationsDialog } from '../ui/paragraph-decorations';
import { DropCapsDialog } from '../ui/drop-caps';
import { contextItems,sec,Opening,Unsaved,anchorOf,About,Shortcuts,Presentation } from './shared';
import type { AppLayoutContext } from '../App';
import { runTableKey } from '../ui/tablekeys';
import { runPathKey } from '../ui/pathkeys';
import { runPresentationKey } from '../ui/presentationkeys';

/** The engine's New Folder refusals (folio_project::newfile codes), in the user's words. */
const folderError = (e: string) => {
  const [code, rest] = [e.split(':')[0], e.slice(e.indexOf(':') + 1).trim()];
  if (code === 'name_required') return tr('폴더 이름을 넣으세요.');
  if (code === 'bad_name') return tr('폴더 이름으로 쓸 수 없습니다: {0}', { 0: rest });
  if (code === 'name_collision') return tr('같은 이름(대소문자나 글자 모양만 다른 것 포함)이 이미 있습니다: {0}', { 0: rest });
  if (code === 'folder_missing') return tr('폴더가 없습니다: {0}', { 0: rest });
  if (code === 'outside_project') return tr('프로젝트 폴더 밖에는 만들 수 없습니다.');
  return e;
};

/** How the tools view lays its buttons out while it floats (a user setting, Tools panel menu; docked, it follows its area's width). */
const TOOLBOX_ARRANGEMENT = 'folio.toolboxArrangement';
type Arrangement = 'grid' | ToolsLayout;
const arrangementOf = (v: unknown): Arrangement => (v === 'SINGLE_COLUMN' || v === 'DOUBLE_COLUMN' || v === 'SINGLE_ROW' ? v : 'grid');
/** The tool help (uix-e1, DESIGN.md § 17): the card at once on a tool (on by default) and the section under the tools view (open by default). */
const INSTANT_TOOL_HELP = 'folio.instantToolHelp';
const TOOL_HELP_OPEN = 'folio.toolHelpOpen';

export function AppLayout(context: AppLayoutContext) {
  const {
    previewSession, previewScope, showControl,
    allHidden, menus, commands,
    setWsMenu, workspaces, controlProps,
    colorPicker, toolLayout, showTools,
    tool, pickTool, setPolygonDialog,
    setEyedropperDialog, setPencilDialog, editable,
    selection, openTransformDialog, doubleTools,
    setDoubleTools, updateSettings, settingsRef,
    flash, swatchy, toolboxPaint,
    selItem, info, strokeFocus,
    setStrokeFocus, formatText, setFormatText,
    screenMode, setPresenting, setScreenMode,
    appWins, doc, wins,
    rows, readOnly, vp,
    effectiveTool, view, onPainted,
    sepActive, onInk, onInkLeave,
    target, store, layers,
    setSelection, selectedPage, setSelectedPage,
    movePageBy, current, setCurrent,
    guides, edges, hiddenChars,
    showNotes, pdfComments, settings,
    contentGrabber, viewMarks, anchoredObjects,
    viewMarksSt, linkBadges, links,
    linkMenu, dock, showHidden,
    threads, rulers, refPoint,
    units, rulerOrigin, preflight,
    preflightBusy, closeDocument, setTool,
    textActions, textReveal, setTextSelection,
    notes, setTableSel, tableActions, pathFinish, presentationBackground,
    textHistory, setContextMenu, zeroPointLocked,
    setSelectedGuide, editState, pageRev,
    runEdit, afterEdit, placement, threading,
    note, directOpen, setImportSource,
    engineBroken, firstPaint, lastRender,
    project, recentProjects, recent,
    open, openSample, status,
    error, host, contextMenu,
    wsMenu, stage, pageEditing,
    guideEditing, layerEditing,
    overrideLock,
    importSource, loadProject, openDoc,
    newDocNotice, presets, setNewDocNotice,
    pdfImport,
    spellOpen,
    spellHandoff, textSelection, setTextReveal,
    setSpellOpen, setSpellHandoff,
    shortcutDialog, setShortcutDialog,
    proof,
    polygonDialog,
    eyedropperDialog, pencilDialog,
    presetsOpen, setPresetsOpen, idmlExport,
    setIdmlExport,
    presenting,
    colorMenu, strokeStyles,
    libraryMenu, bookMenu, databaseSource, paragraphDecorationsOpen,
    ctx, setParagraphDecorationsOpen, dropCapsOpen,
    setDropCapsOpen, dialog, setDialog,
    unsaved,
    save, setUnsaved, findOpen,
    setFindOpen, textFindSession, glyphFind,
    quickApplyOpen, textFormatting, setQuickApplyOpen,
    selectedIds, typeAids, characterLines,
    paragraphOptions, grepStyles, storyPanel,
    listOptions, numberingLists, storyEditor,
    captions, hyperlinks,
    math,
    lightDialog, setLightDialog,
    tableDialog,
    setTableDialog, tableSel, fxDialog,
    setFxDialog, dragging, setStatus,
    workbench, newDocument, editable: canEdit,
  } = context;
  const folioLayout = workbench.layout === 'folio';
  const [helpRoom, setHelpRoom] = useState(true);
  const controlBarPlace = useControlBar();
  const hide = folioLayout ? FOLIO_HIDES : CLASSIC_HIDES;
  dock.leftOut.current = hide;
  // the tools view floating: the rows or columns a person chose (Tools panel menu), else the grid it has docked
  const arrangement = arrangementOf(settings.preferences?.[TOOLBOX_ARRANGEMENT]);
  const toolboxFloats = dock.where('toolbox')?.place === 'floating';
  const arrange = (value: Arrangement) => {
    void updateSettings({ preferences: { ...settingsRef.current.preferences, [TOOLBOX_ARRANGEMENT]: value } }).then(() => {
      // a floating tools view takes the size its buttons need in the new arrangement (measured once it is drawn)
      window.requestAnimationFrame(() => window.requestAnimationFrame(() => {
        const at = dock.where('toolbox');
        const box = document.querySelector<HTMLElement>('.floating .sb-tools .toolbox');
        const f = at?.place === 'floating' ? dock.state.floating[at.group] : null;
        if (!f || !box) return;
        dock.apply({ op: 'moveFloating', group: at!.group, x: f.x, y: f.y, w: Math.ceil(box.scrollWidth) + 2, h: Math.ceil(box.scrollHeight) + 27, view: { w: window.innerWidth, h: window.innerHeight } });
      }));
    }, (e) => flash(String(e)));
  };
  const toolsPanel = (placement: 'edge' | 'sidebar', own?: { layout: ToolsLayout; onLayout: (v: ToolsLayout) => void }) => (
          <ToolsPanel
            placement={placement}
            tool={tool}
            onTool={pickTool} onToolDoubleClick={tool => {
              if (tool === 'polygon' || tool === 'polygonFrame') { setPolygonDialog(true); return; } if (tool === 'eyedropper') { setEyedropperDialog(true); return; } if (tool === 'pencil') { setPencilDialog(true); return; }
              // InDesign's double clicks on a tool (tools-c1): the Hand tool fits the page in the window, the Zoom tool goes to 100 %, the Measure tool
              // shows the Info panel, the Gradient Swatch tool the Gradient panel, the Gradient Feather tool the Effects window on Gradient Feather
              const run = (id: string) => { const c = commands[id]; if (c?.run && (c.enabled ?? true)) c.run(); };
              if (tool === 'hand') { run('view.fitPage'); return; } if (tool === 'zoom') { run('view.actualSize'); return; }
              if (tool === 'measure') { dock.show('info', false); return; } if (tool === 'gradient') { dock.show('gradient', false); return; }
              if (tool === 'gradientFeather') { run('id.object.effects.gradientFeather'); return; }
              const kind = transformDialogFor(tool); if (editable && kind && selection.length && !selection.some(s => s.content) && (kind !== 'scale' || selection.length === 1)) openTransformDialog({ kind, ids: selection.filter(s => !s.content).map(s => s.id) }); }}
            double={doubleTools}
            onToggleDouble={() => setDoubleTools((v) => !v)}
            layout={own?.layout ?? toolLayout}
            onLayout={own?.onLayout ?? ((value) => void updateSettings({ preferences: { ...settingsRef.current.preferences, 'generalPreferences.toolsPanel': value } }).catch(e => flash(String(e))))}
            fill={{ ...swatchy(toolboxPaint.fill ?? selItem?.fill ?? (selection.length ? undefined : info?.defaultFillStroke?.fill), true), mixed: toolboxPaint.fillMixed }}
            stroke={{ ...swatchy(toolboxPaint.stroke ?? selItem?.stroke ?? (selection.length ? undefined : info?.defaultFillStroke?.stroke), false), mixed: toolboxPaint.strokeMixed }}
            textStyle={toolboxPaint.glyph}
            strokeFocus={strokeFocus}
            onFocusStroke={setStrokeFocus}
            onOpenColorPicker={colorPicker.open}
            onSwap={toolboxPaint.swap} onDefault={toolboxPaint.defaults} onApplyColor={toolboxPaint.applyColor} onApplyGradient={toolboxPaint.applyGradient} onApplyNone={toolboxPaint.applyNone}
            textMode={formatText || toolboxPaint.glyph}
            onTextMode={setFormatText}
            screenMode={screenMode}
            onScreenMode={(m) => (m === 'presentation' ? setPresenting(true) : setScreenMode(m))}
          />
  );
  // what the explorer does with a row: the same commands the menus run
  const projectRoot = project?.info.root ?? null;
  const explorer: ExplorerActions = {
    open: (rel) => { if (projectRoot) void open(documentPath(projectRoot, rel)); },
    openBook: (rel) => bookMenu.openFile(rel),
    openLibrary: (rel) => libraryMenu.openFile(rel),
    place: canEdit && doc ? (file) => void placement.begin([file]) : null,
    placeWhy: doc ? tr('이 문서는 편집할 수 없어 그림을 놓을 수 없습니다.') : tr('그림을 놓으려면 문서를 먼저 여세요.'),
    importDoc: async (file) => { const f = file ?? (await window.folio.openDialog()); if (f) setImportSource(f); },
    reveal: (file) => void window.folio.reveal(file),
    refresh: () => { if (projectRoot) void loadProject(projectRoot); },
    newDocument: () => newDocument(),
    newBook: (folder) => { if (projectRoot) void window.folio.request({ cmd: 'openDialog', dialog: 'newBook', values: { project: projectRoot, folder, book: '', fileName: tr('책 1') + '.foliobook' } }); },
    newLibrary: (folder) => { if (projectRoot) void window.folio.request({ cmd: 'openDialog', dialog: 'newLibrary', values: { project: projectRoot, folder, library: '', sort: 'newest', fileName: tr('라이브러리') + '.foliolibrary' } }); },
    newFolder: async (folder, name) => {
      if (!projectRoot) throw new Error(tr('열린 프로젝트가 없습니다.'));
      const { header } = await window.folio.request({ cmd: 'projectNewFolder', root: projectRoot, folder, name });
      if (!header.ok) throw new Error(folderError(String(header.error)));
      await loadProject(projectRoot);
      return String(header.path);
    },
    newProject: openNewProject,
    openProject: () => commands['file.openProject']?.run?.(),
    openRecent: (root) => void open(root),
    note: flash,
    path: (rel) => (projectRoot ? documentPath(projectRoot, rel) : rel),
  };
  // the Folio views are dock panels (uix-d2): the dock draws them where they are, through this host
  const viewCommands: Commands = {
    'toolbox.grid': { label: tr('격자 (떠 있을 때)'), run: () => arrange('grid'), checked: arrangement === 'grid' },
    'toolbox.column': { label: tr('세로 한 줄 (떠 있을 때)'), run: () => arrange('SINGLE_COLUMN'), checked: arrangement === 'SINGLE_COLUMN' },
    'toolbox.double': { label: tr('세로 두 줄 (떠 있을 때)'), run: () => arrange('DOUBLE_COLUMN'), checked: arrangement === 'DOUBLE_COLUMN' },
    'toolbox.row': { label: tr('가로 한 줄 (떠 있을 때)'), run: () => arrange('SINGLE_ROW'), checked: arrangement === 'SINGLE_ROW' },
    'explorer.refresh': { label: tr('새로 고침'), run: projectRoot ? explorer.refresh : undefined },
    'explorer.import': { label: tr('InDesign 문서 가져오기…'), run: () => void explorer.importDoc() },
    'explorer.reveal': { label: /mac/i.test(navigator.platform) ? tr('Finder 에서 보기') : tr('파일 탐색기에서 보기'), run: projectRoot ? () => explorer.reveal(projectRoot) : undefined },
  };
  const viewMenus: Partial<Record<PanelId, MenuEntry[]>> = {
    toolbox: [{ cmd: 'toolbox.grid' }, { cmd: 'toolbox.column' }, { cmd: 'toolbox.double' }, { cmd: 'toolbox.row' }, { sep: true }, { cmd: 'toolbox.instantHelp' }, { cmd: 'toolbox.helpOpen' }],
    explorer: [{ cmd: 'explorer.import' }, { sep: true }, { cmd: 'explorer.reveal' }, { cmd: 'explorer.refresh' }],
  };
  const floatingLayout = toolboxFloats && arrangement !== 'grid' ? arrangement : null;
  // what the tool help says about an item: its name and keys from this keymap (a tool's own letter, a command's keys as the person set them)
  const toolHelp: ToolHelp = {
    instant: settings.preferences?.[INSTANT_TOOL_HELP] !== false,
    describe: (id: HelpId) => {
      const entry = helpEntry(id);
      if (!entry) return null;
      const tool = isToolHelp(id) ? toolDef(id as ToolId) : null;
      const keys = tool ? (tool.key ? [tool.key] : []) : entry.command ? entry.command.flatMap((c) => commands[c]?.keys?.slice(0, 1) ?? []) : entry.keys ?? [];
      return { name: tool?.label ?? entry.name?.() ?? id, keys, summary: entry.summary(), detail: entry.detail(), note: tool ? toolNote(tool.id) : undefined };
    },
  };
  // the section open or folded as the person chose it; never chosen, open when the tools view has room for it under the tools
  const helpChosen = settings.preferences?.[TOOL_HELP_OPEN];
  const helpOpen = typeof helpChosen === 'boolean' ? helpChosen : helpRoom;
  const setPreference = (key: string, value: boolean) => void updateSettings({ preferences: { ...settingsRef.current.preferences, [key]: value } }).catch((e) => flash(String(e)));
  viewCommands['toolbox.instantHelp'] = { label: tr('도구 설명 즉시 보기'), run: () => setPreference(INSTANT_TOOL_HELP, !toolHelp.instant), checked: toolHelp.instant };
  viewCommands['toolbox.helpOpen'] = { label: tr('아래에 도구 설명 펼치기'), run: floatingLayout ? undefined : () => setPreference(TOOL_HELP_OPEN, !helpOpen), checked: helpOpen,
    why: floatingLayout ? tr('도구를 줄로 띄워 두면 설명 구역이 없습니다. 격자로 바꾸면 보입니다.') : undefined };
  const panels: PanelHost = {
    ...host,
    commands: { ...host.commands, ...viewCommands },
    menu: (id) => viewMenus[id] ?? host.menu(id),
    render: (id) => {
      if (id === 'explorer') return (
        <ExplorerView
          project={project}
          recentProjects={recentProjects}
          current={doc ? { rel: doc.folio?.document.path ?? null, name: fileName(doc.path), pages: doc.pages.length, dirty: editState.dirty, readOnly } : null}
          openDocs={appWins.view && appWins.view.tabs.length > 1 ? appWins.view.tabs.map((t) => ({ id: t.id, title: t.title ?? '', dirty: t.dirty, active: t.active })) : null}
          onActivateDoc={(id) => void appWins.apply({ cmd: 'activate', doc: id })}
          actions={explorer}
        />
      );
      if (id === 'toolbox') return <ToolboxView tools={floatingLayout ? toolsPanel('edge', { layout: floatingLayout, onLayout: (v) => arrange(v) }) : toolsPanel('sidebar')}
        help={floatingLayout ? null : <ToolHelpSection id={tool} icon={toolDef(tool)?.icon} open={helpOpen} onToggle={() => setPreference(TOOL_HELP_OPEN, !helpOpen)} onRoom={setHelpRoom} />} />;
      return host.render(id);
    },
  };
  const sidebar = folioLayout && !allHidden && <ActivityBar dock={dock} hide={hide} />;
  // Edit > Keyboard Shortcuts is the engine's app dialog (next/shared/dialogs/shortcuts.json); it is handed the commands and their default keys
  useEffect(() => {
    if (!shortcutDialog) return;
    setShortcutDialog(false);
    void window.folio.request({ cmd: 'openDialog', dialog: 'shortcuts', scope: 'app', values: { commands: shortcutCommandList(commands) } })
      .then(({ header }) => { if (!header.ok) flash(String(header.error)); });
  }, [shortcutDialog]);  // eslint-disable-line react-hooks/exhaustive-deps


  // where the control bar sits (build-c2): its menu's Dock at Top / Dock at Bottom / Float (engine user settings)
  const controlDock = controlBarPlace.dock;
  return (
    <PreviewEditContext.Provider value={previewSession}>
    <PreviewScopeContext.Provider value={previewScope}>
    <ToolHelpContext.Provider value={toolHelp}>
    <div className={`ws${showControl && !allHidden && controlDock === 'top' ? '' : ' no-control'}${showControl && !allHidden && controlDock === 'bottom' ? ' control-bottom' : ''}`}>
      <MenuBar
        menus={menus}
        commands={commands}
        right={
          <>
            {project && <span className="ws-project" title={project.info.root}><Icon name="project" size={14} />{project.info.name}</span>}
            <button className="ws-switch" onClick={(e) => setWsMenu(e.currentTarget.getBoundingClientRect())} title={tr('작업 영역')}>
              <Icon name="workspace" size={16} /> {workspaces.label} <Icon name="chevronDown" size={12} className="ws-chev" /></button>
          </>
        }
      />
      {showControl && !allHidden && controlDock === 'top' ? (
        <ControlPanel {...controlProps} />
      ) : <div />}
      {/* the control bar docked at the bottom or floating (build-c2: its menu's Dock at Bottom / Float) */}
      {showControl && !allHidden && controlDock === 'float' && <FloatingControl><ControlPanel {...controlProps} /></FloatingControl>}
      {colorPicker.element}
      <div className={`ws-main${!folioLayout && toolLayout === 'SINGLE_ROW' ? ' horizontal-tools' : ''}${folioLayout ? ' folio-layout' : ''}`} data-layout={workbench.layout}>
        {folioLayout ? sidebar : showTools && !allHidden && toolsPanel('edge')}
        <div className="ws-content">
        {/* Tab (Window ▸ Hide Panels) hides every panel: the sidebar with its tools, the docks and the floating panels; a second Tab shows
            them as they were (the dock layout is not touched). Shift+Tab hides the panels but the tools: the sidebar stays (dock.hidden) */}
        {!allHidden && <DockArea dock={dock} host={panels} hide={hide} area="left" sidebar={folioLayout} />}
        <div className="doc-col">
        <AppTabs aw={appWins} />
        <div className="doc-host"
          // a picture or PDF dragged from the explorer: the place cursor is loaded with it (then a click or a drag puts it down)
          onDragOver={(e) => { if (e.dataTransfer.types.includes('application/x-folio-place') && explorer.place) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; } }}
          onDrop={(e) => { const f = e.dataTransfer.getData('application/x-folio-place'); if (f) { e.preventDefault(); if (explorer.place) explorer.place(f); else flash(explorer.placeWhy); } }}>
          {doc ? (
            <WindowHost ws={wins} rows={rows} pane={<DocWindow
              doc={doc}
              readOnly={readOnly}
              rows={rows}
              vp={vp}
              tool={effectiveTool}
              ink={view}
              onPainted={onPainted}
              onInk={sepActive ? onInk : undefined}
              onInkLeave={onInkLeave}
              target={target}
              store={store}
              info={info}
              layers={layers}
              selection={selection}
              setSelection={setSelection}
              selectedPage={selectedPage}
              setSelectedPage={setSelectedPage}
              onPageMove={movePageBy}
              current={current}
              setCurrent={setCurrent}
              screenMode={screenMode}
              guides={guides}
              edges={edges}
              hiddenChars={hiddenChars}
              showNotes={showNotes}
              pdfComments={pdfComments.available && !pdfComments.hidden}
              pdfCommentSelected={pdfComments.selected}
              styleOverrideHighlighter={settings.preferences?.[STYLE_OVERRIDES_KEY]===true}
              dynamicSpelling={{ enabled: settings.preferences?.[DYNAMIC_KEY] === true, options: spellOptions(settings.preferences), userDictionaries: cleanUserDictionaries(settings.userDictionaries), color: settings.preferences?.['spellPreferences.misspelledWordColor'] }}
              contentGrabber={contentGrabber}
              tripleClickSelectsLine={settings.preferences?.['textEditingPreferences.tripleClickSelectsLine'] !== false}
              smartCutAndPaste={settings.preferences?.['textEditingPreferences.smartCutAndPaste'] !== false}
              dragDropText={settings.preferences?.['textEditingPreferences.dragAndDropTextInLayout'] === true}
              showContentGrabber={settings.preferences?.['generalPreferences.showContentGrabber'] !== false}
              pencilPrefs={pencilPrefs(settings.preferences)}
              marks={viewMarks}
              onAnchorOptions={anchoredObjects.open}
              showConveyor={viewMarksSt.conveyor}
              linkBadges={linkBadges}
              onLinkBadge={(owner) => { const row = links?.links.find((l) => l.owner === owner); if (row) linkMenu.select(row); dock.show('links', false); }}
              highlightObjectUnderSelectionTool={settings.preferences?.['generalPreferences.highlightObjectUnderSelectionTool'] !== false}
              showHiddenSpreads={showHidden}
              threads={threads}
              rulers={rulers}
              refPoint={refPoint}
              units={units}
              rulerOrigin={rulerOrigin}
              smart={viewMarksSt.smart}
              preflight={preflight}
              preflightBusy={preflightBusy}
              onPreflight={() => dock.show('preflight', false)}
              onNote={flash}
              onClose={closeDocument}
              onDoubleClickText={() => setTool('type')}
              onExitText={() => setTool('selection')}
              textActions={textActions}
              textReveal={textReveal}
              onTextState={setTextSelection}
              onNoteAt={notes.insertAt}
              onTableState={setTableSel}
              tableActions={tableActions}
              onTableKey={e => runTableKey(e, commands)}
              onPathKey={e => runPathKey(e, commands)}
              pathFinish={pathFinish}
              onTextHistory={textHistory}
              onContextMenu={(at, kind) => setContextMenu({ ...at, kind })}
              zeroPointLocked={zeroPointLocked}
              onGuideSelection={setSelectedGuide}
              editable={editable}
              dirty={editState.dirty}
              pageRev={pageRev}
              onEdit={runEdit}
              placement={placement}
              threading={threading}
              statusExtra={
                <>
                  {note && <span className="note" key={note}>{note}</span>}
                  {directOpen && (
                    <span className="read-only-badge" title={tr('프로젝트 없이 연 문서: 편집·저장은 안 되고 사전 검사·PDF·패키지는 됩니다')}>
                      {tr('읽기 전용 · ')}<button className="link-button" onClick={() => setImportSource(doc.path)}>{tr('프로젝트로 가져오기')}</button>
                    </span>
                  )}
                  {engineBroken && (
                    <span className="read-only-badge" title={tr('편집 엔진 안에서 오류가 나서 이 문서의 편집을 멈췄습니다. 문서를 다시 여세요 (저장하지 않은 편집은 안전하게 남길 수 없습니다)')}>{tr('편집 중지 · 문서를 다시 여세요')}</span>
                  )}
                  {doc.folio?.readOnly && (
                    <span className="read-only-badge" title={tr('다른 사용자나 다른 창이 이 문서를 열고 있어 편집·저장은 안 됩니다')}>{tr('읽기 전용 · 다른 곳에서 열려 있음')}</span>
                  )}
                  {doc.missingFonts.length > 0 && <button className="status-link warn" onClick={() => dock.show('preflight', false)} title={doc.missingFonts.map(([f, s]) => `${f} ${s}`).join('\n')}>{tr('없는 글꼴 {0}', {0: doc.missingFonts.length})}</button>}
                  {links && links.summary.missing > 0 && <button className="status-link warn" onClick={() => dock.show('links', false)}>{tr('없는 링크 {0}', {0: links.summary.missing})}</button>}
                  {doc.overset > 0 && <span className="warn" title={tr('넘친 기사')}>{tr('넘침 {0}', {0: doc.overset})}</span>}
                  <span className="timing" title={tr('열기 {0}초 (변환 {1} · 해석 {2} · 글꼴 {3} · 조판 {4}){5}{6}', {0: sec(doc.timings.totalMs), 1: sec(doc.timings.readerMs), 2: sec(doc.timings.parseMs), 3: sec(doc.timings.fontsMs), 4: sec(doc.timings.layoutMs), 5: firstPaint !== null ? tr(' · 첫 화면 {0}초', {0: sec(firstPaint)}) : '', 6: lastRender !== null ? tr(' · 마지막 그리기 {0}ms', {0: lastRender.toFixed(0)}) : ''})}>{tr('{0}페이지 · {1}', {0: doc.pages.length, 1: fileName(doc.path)})}</span>
                </>
              }
              strokeFocus={strokeFocus}
              toolboxText={toolboxPaint.text}
            />} />
          ) : (
            <section className="docwin empty">
              <div className="doc-tabs" />
              <div className="doc-home">
                {!project ? (
                  <StartScreen
                    recentProjects={recentProjects}
                    recentDocuments={recent}
                    onNewProject={openNewProject}
                    onOpenProject={async () => { const d = await window.folio.chooseFolder(tr('프로젝트 폴더 열기 (folio.project가 있는 폴더)')); if (d) open(d); }}
                    onImport={async (p) => { const f = p ?? (await window.folio.openDialog()); if (f) setImportSource(f); }}
                    onOpenRecentProject={(p) => open(p.root)}
                    onOpenSample={openSample}
                  />
                ) : (
                  <ProjectHome
                    project={project.info}
                    onNewDocument={commands['file.new']?.run ? () => commands['file.new'].run?.() : undefined}
                    onImport={async () => { const f = await window.folio.openDialog(); if (f) setImportSource(f); }}
                    onOpenSample={openSample}
                  />
                )}
              </div>
              <footer className="doc-status">
                <span>{status === 'error' ? error : project ? `${project.info.name} — ${project.info.root}` : tr('프로젝트를 열거나 InDesign 문서를 여세요 (Ctrl+O)')}</span>
                <span className="doc-status-fill" />
                {note && <span className="note" key={note}>{note}</span>}
              </footer>
            </section>
          )}
        </div>
        {!allHidden && <DockArea dock={dock} host={panels} hide={hide} area="bottom" />}
        </div>
        {!allHidden && <DockStrip dock={dock} host={panels} hide={hide} />}
        {!allHidden && <DockColumn dock={dock} host={panels} hide={hide} />}
        </div>
      </div>
      {showControl && !allHidden && controlDock === 'bottom' && <ControlPanel {...controlProps} />}
      {!allHidden && <FloatingPanels dock={dock} host={panels} hide={hide} />}

      {contextMenu && (
        <PopupMenu items={contextItems(contextMenu.kind)} commands={commands} anchor={new DOMRect(contextMenu.x, contextMenu.y - 2, 0, 0)} onClose={() => setContextMenu(null)} />
      )}
      {wsMenu && (
        <PopupMenu items={[{ cmd: 'window.workspaceFolio' }, { cmd: 'window.workspaceClassic' }, ...workspaces.entries, { sep: true }, { cmd: 'window.resetWorkspace' }, { cmd: 'id.window.workspace.newWorkspace' }, { cmd: 'id.window.workspace.deleteWorkspace' }]} commands={commands} anchor={wsMenu} onClose={() => setWsMenu(null)} />
      )}
      {status === 'opening' && <Opening stage={stage} />}
      {pageEditing.dialog}
      {guideEditing.dialog}
      {workspaces.dialogs}
      {layerEditing.dialog}
      {doc?.folio && <LockBanner lock={doc.folio.lock} onOverride={overrideLock} />}
      {importSource !== null && (
        <ImportDialog
          source={importSource}
          current={project?.info ?? null}
          roots={settings.linkRoots}
          onClose={() => setImportSource(null)}
          onDone={async (folio, p) => { setImportSource(null); await loadProject(p.root); open(folio); }}
          onReadOnly={doc?.path === importSource ? undefined : () => { const f = importSource; setImportSource(null); openDoc(f); }}
        />
      )}
      {newDocNotice && !project && (
        <NewDocumentNotice onClose={() => setNewDocNotice(false)} openProject={() => commands['file.openProject'].run?.()} newProject={openNewProject} />
      )}
      <PdfCommentsAlert alert={pdfComments.alert} answer={pdfImport.answer} />
      {spellOpen && doc && <SpellCheckDialog key={spellHandoff?.n ?? 0} handoff={spellHandoff?.handoff ?? null} selection={textSelection} revision={`${editState.history.undo.length}/${editState.history.redo.length}/${editState.undo ?? ''}`} options={spellOptions(settings.preferences)} userDictionaries={cleanUserDictionaries(settings.userDictionaries)}
        saveUserDictionaries={next => updateSettings({ userDictionaries: next })} edit={runEdit} reveal={m => setTextReveal({ ...m })}
        openUserDictionary={() => void openUserDictionary(flash)} close={() => { setSpellOpen(false); setSpellHandoff(null); }} />}
      {proof.dialog && <ProofSetupDialog setup={proof.setup} documentCmyk={doc?.color?.cmyk?.name ?? null} apply={proof.apply} onClose={proof.closeDialog} />}
      {polygonDialog && <PolygonDialog close={() => setPolygonDialog(false)} />}
      {eyedropperDialog && <EyedropperDialog close={() => setEyedropperDialog(false)} />}
      {pencilDialog && <PencilPrefsDialog value={pencilPrefs(settings.preferences)} onSave={v => updateSettings({ preferences: withPencilPrefs(settingsRef.current.preferences, v) })} onClose={() => setPencilDialog(false)} />}
      {presetsOpen && <DocumentPresetsDialog presets={presets} changed={() => void updateSettings({}).catch(e => flash(String(e)))} onClose={() => setPresetsOpen(false)} />}
      {idmlExport && doc?.folio && <IdmlExportDialog folio={doc.folio} onClose={() => setIdmlExport(false)} />}
      {presenting && doc && <Presentation doc={doc} rows={rows} start={current} background={presentationBackground} onKey={e => runPresentationKey(e, commands)} onExit={(p) => { setPresenting(false); vp.scrollToPage(p, false); }} />}
      {placement.dialog}
      {colorMenu.node}
      {libraryMenu.dialog}
      {bookMenu.dialog}
      {databaseSource.dialog}
      {paragraphDecorationsOpen&&<ParagraphDecorationsDialog ctx={ctx} mode={paragraphDecorationsOpen} close={()=>setParagraphDecorationsOpen(false)}/>}
      {dropCapsOpen&&<DropCapsDialog ctx={ctx} close={()=>setDropCapsOpen(false)}/>}
      {dialog === 'about' && <About onClose={() => setDialog(null)} />}
      {unsaved && (
        <Unsaved
          name={doc ? fileName(doc.path) : ''}
          onSave={async () => { const ok = await save(); unsaved.resolve(!!ok); setUnsaved(null); }}
          onDiscard={() => { unsaved.resolve(true); setUnsaved(null); }}
          onCancel={() => { unsaved.resolve(false); setUnsaved(null); }}
        />
      )}
      {findOpen&&<TextFindDialog selection={textSelection} edit={runEdit} reveal={m=>setTextReveal({...m})} close={()=>setFindOpen(false)} session={textFindSession.session} remember={textFindSession.remember} glyph={glyphFind}/>}
      {quickApplyOpen&&<QuickApply commands={commands} text={textFormatting} close={()=>setQuickApplyOpen(false)}
        objectStyle={editable&&!textSelection&&selection.length>0&&selection.every(e=>{const item=findItem(store.peek(e.page),e.id)?.item;return !e.content&&item&&!item.locked&&item.kind!=='group';})
          ?style=>void runEdit(tr('개체 스타일 적용'),[{op:'applyObjectStyle',ids:selectedIds(),style,clear:false}]):undefined}/>}
      {typeAids.dialog}
      {characterLines.dialog}
      {paragraphOptions.dialog}
      {grepStyles.dialog}
      {storyPanel.dialog}
      {listOptions.dialog}
      {numberingLists.dialog}
      {storyEditor.dialog}
      {dialog === 'shortcuts' && <Shortcuts commands={commands} onClose={() => setDialog(null)} />}
      {dialog === 'limits' && <LimitsDialog onClose={() => setDialog(null)} />}
      {dialog === 'credentialStoreTest' && <CredentialStoreTestDialog onClose={() => setDialog(null)} />}
      {dialog === 'inkManager' && <InkManagerDialog key={info?.inks.map((k) => `${k.name}${k.inkType}${k.convertToProcess}${k.alias}`).join('|')} info={info} edit={runEdit} close={() => setDialog(null)} />}
      {dialog === 'variables' && <VariablesDialog info={info} edit={runEdit} close={() => setDialog(null)}
        insert={editable && textSelection ? (id) => textActions.current?.variable(id) : null} />}
      {lightDialog && <GlobalLightDialog edit={runEdit} close={() => setLightDialog(false)} />}
      {linkMenu.optionsOpen && <LinksPanelOptionsDialog columns={linkMenu.columns} apply={linkMenu.setColumns} close={linkMenu.closeOptions} thumbnails={linkMenu.thumbnails} setThumbnails={linkMenu.setThumbnails} />}
      {linkMenu.fileDialog && <LinkFileDialog kind={linkMenu.fileDialog.kind} link={linkMenu.fileDialog.link} close={linkMenu.closeFileDialog} />}
      {linkMenu.extDialog && <RelinkExtensionDialog link={linkMenu.extDialog} apply={linkMenu.applyExt} close={linkMenu.closeExt} />}
      {/* the app dialogs (New Project, Language): no document needed; the frame shows the engine's, then what they made is opened or read */}
      <DialogFrame scope="app" units={units} done={(name, result) => {
        if (name === 'newProject') open((result as { project: { root: string } }).project.root);
        // a new document is made and imported into the project: open it (the project explorer reads the folder again)
        if (name === 'newDocument') { const made = (result as { result?: { path?: string } }).result?.path; if (made) open(made); }
        // a document preset saved (New Document's Save Preset, the preset editor): the menu and lists read the settings again
        if ((result as { type?: string }).type === 'documentPresets') void updateSettings({}).catch(e => flash(String(e)));
        // New Book / New Library: the file is made, its window opens; or the window to open an existing one
        const files = name === 'newBook' ? bookMenu : name === 'newLibrary' ? libraryMenu : null;
        if (files) { if ((result as { screen?: string }).screen === 'openExisting') files.openExisting(); else files.made(result as Record<string, unknown>); }
        // Language and Preferences saved app settings: they are read again (theme, scale, tools and the rest follow them)
        // (User Dictionary, Color Settings, Flattener Presets and Shortcuts too: the spelling marks, colour, flattening and keys read the saved settings)
        if (name === 'language' || name === 'preferences' || name === 'user' || name === 'userDictionary' || name === 'colorSettings' || name === 'flattenerPresets' || name === 'shortcuts' || name === 'printPresets') return updateSettings({}).catch(e => flash(String(e)));
      }} />
      {doc && <DialogFrame path={doc.path} units={units} done={(name, result) => {
        // File ▸ Export PDF's and File ▸ Package's "show preflight": the Preflight panel
        if ((name === 'pdfExport' || name === 'package') && (result as { screen?: string }).screen === 'showPreflight') { dock.show('preflight', false); return; }
        return name === 'preferences' ? updateSettings({}).catch(e => flash(String(e))) : undefined;
      }} applied={r => { afterEdit(r); if(r.created?.length) setSelection(r.created.map(id=>({id,page:selection[0]?.page??current,content:false}))); }} />}
      {tableDialog?.kind === 'create' && textSelection && <CreateTableDialog text={textSelection} edit={runEdit} close={() => setTableDialog(null)} />}
      {tableDialog?.kind === 'insert' && tableSel && <InsertDialog sel={tableSel} what={tableDialog.what} edit={runEdit} close={() => setTableDialog(null)} />}
      {tableDialog?.kind === 'table' && tableSel && <TableOptionsDialog sel={tableSel} info={info} start={tableDialog.tab ?? 'setup'} edit={runEdit} close={() => setTableDialog(null)} />}
      {tableDialog?.kind === 'goToRow' && tableSel && <GoToRowDialog sel={tableSel} close={() => setTableDialog(null)}
        go={(r) => tableActions.current?.select([0, r], [tableSel.info.columns.length - 1, r])} />}
      {tableDialog?.kind === 'cell' && tableSel && (
        <CellOptionsDialog sel={tableSel} info={info} start={tableDialog.tab} edit={runEdit} close={() => setTableDialog(null)}
          format={(pt) => formatLength(pt, units.horizontal)} parse={(text) => parseLength(text, units.horizontal)} />
      )}
      {fxDialog && blendTargets(selection, (pg) => store.peek(pg)).length > 0 && (
        <EffectsDialog targets={blendTargets(selection, (pg) => store.peek(pg))} info={info} start={fxDialog.page} startLevel={fxDialog.level} edit={runEdit} close={() => setFxDialog(null)}
          format={(pt) => formatLength(pt, units.horizontal)} parse={(text) => parseLength(text, units.horizontal)} />
      )}
      {dialog === 'corners' && cornerTargets(selection, (pg) => store.peek(pg)).length > 0 && (
        <CornerOptionsDialog targets={cornerTargets(selection, (pg) => store.peek(pg))} edit={runEdit} close={() => setDialog(null)}
          format={(pt) => formatLength(pt, units.horizontal)} parse={(text) => parseLength(text, units.horizontal)} />
      )}
      {dragging && (
        <div className="overlay drop">
          <div className="card"><b>{tr('여기에 놓으면 엽니다')}</b><p>{tr('.folio · INDD · INDT · IDML (원본은 읽기만 합니다)')}</p></div>
        </div>
      )}
      {status === 'error' && (
        <div className="overlay" onClick={() => setStatus(doc ? 'ready' : 'idle')} onKeyDown={(e) => { if (e.key === 'Escape') setStatus(doc ? 'ready' : 'idle'); }}>
          <div className="card error" role="alertdialog" aria-labelledby="open-error-title" aria-describedby="open-error-detail" onClick={(e) => e.stopPropagation()}>
            <h2 id="open-error-title"><Icon name="warning" size={16} />{tr('열지 못했습니다')}</h2>
            <p className="error-detail" id="open-error-detail">{error}</p>
            <p className="error-lead">{tr('파일이 그 자리에 있는지, 형식(.folio · INDD · INDT · IDML)이 맞는지 확인한 뒤 다시 여세요.')}</p>
            <div className="actions"><button className="primary" autoFocus onClick={() => setStatus(doc ? 'ready' : 'idle')}>{tr('닫기')}</button></div>
          </div>
        </div>
      )}
    </div>
    </ToolHelpContext.Provider>
    </PreviewScopeContext.Provider>
    </PreviewEditContext.Provider>
  );
}
