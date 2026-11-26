import { windowPanel } from '../features/window/panels';
import { t as tr } from '../i18n';
import { STYLE_OVERRIDES_KEY } from '../features/view/styleoverrides';
import { PreflightView } from '../print';
import {
ProjectExplorer,documentPath,openNewProject
} from '../project';
import { SeparationsView } from '../separations';
import { type PanelId } from '../ui/dock';
import { loadGlyphIntoFind } from '../ui/glyphfind';
import { TableStylesPanel } from '../ui/tablestyles';
import { type MenuEntry,PANEL_MENUS } from '../ui/menudef';
import {
AlignPanel,
CharacterPanel,
ColorPanel,
EffectsPanel,
GradientPanel,
HistoryPanel,
InfoPanel,
LayersPanel,
LinksPanel,
PagesPanel,
type PanelCtx,
ParagraphPanel,
StrokePanel,
StylesPanel,
SwatchesPanel,
TextWrapPanel,
TransformPanel,
} from '../ui/panels';
import type { AppContext } from './context';
import type { Commands } from '../ui/commands';
import type { ComponentType, RefObject } from 'react';

const CONTEXT_PANELS = new Map<PanelId, ComponentType<{ ctx: PanelCtx }>>([
  ['layers', LayersPanel],
  ['links', LinksPanel],
  ['stroke', StrokePanel],
  ['color', ColorPanel],
  ['swatches', SwatchesPanel],
  ['gradient', GradientPanel],
  ['info', InfoPanel],
  ['transform', TransformPanel],
  ['effects', EffectsPanel],
  ['character', CharacterPanel],
  ['paragraph', ParagraphPanel],
  ['textWrap', TextWrapPanel],
  ['history', HistoryPanel],
]);

export function createPanels(context: AppContext, commandsRef: RefObject<Commands>, commands: Commands) {
  const {
    pdfComments, pdfImport, setTextReveal,
    flattener, math, dataMerge, databaseSource,
    settings, updateSettings, settingsRef,
    flash, objectStyleMenu, textStyleMenu,
    tableStyleMenu, pageEditing, pageRev,
    layerEditing, doc, info,
    readOnly, store, selection,
    setSelection, current, selectedPage,
    setSelectedPage, masterView, setMasterView,
    vpRef, vp, setContextMenu,
    go, units, rulerOrigin,
    links, linksBusy, addRoot,
    removeRoot, relinkNow, spreads,
    editable, runEdit, textFormatting,
    setFxDialog, tableSel, colorPicker,
    editState, historyGo, historyCall, colorMenu,
    colorPanel, linkMenu, infoMenu,
    panelOptions, tool, formatText,
    setFormatText, glyphFind, setFindOpen,
    notes, hyperlinks, changes,
    realDoc, realInfo, alignment,
    preflight, preflightBusy, rerunPreflight,
    preflightMenu, sepSupported, inks,
    sep, setSep, reading,
    project, open, loadProject,
    setImportSource, newDocument, directOpen,
  } = context;
  // --- panels --------------------------------------------------------------------------
  const ctx: PanelCtx = {
    pdfComments: { ...pdfComments, start: pdfImport.start, reveal: setTextReveal },
    flattener,
    math: math.actions,
    dataMerge,
    databaseSource: databaseSource.actions,
    styleOverrideHighlighter:{enabled:settings.preferences?.[STYLE_OVERRIDES_KEY]===true,toggle:()=>{void updateSettings({preferences:{...settingsRef.current.preferences,[STYLE_OVERRIDES_KEY]:settingsRef.current.preferences?.[STYLE_OVERRIDES_KEY]!==true}}).catch(e=>flash(String(e)));}},
    objectStyleActions: objectStyleMenu.publish,
    textStyleActions: textStyleMenu.publish,
    tableStyleActions: tableStyleMenu.publish,
    commands: () => commandsRef.current,
    pageEditing, pageRev, layerEditing,
    doc, info, readOnly, store, selection, setSelection, current, selectedPage, setSelectedPage,
    // the Pages panel goes to a page of the document: out of a master first
    goToPage: (i) => { if (masterView) { setMasterView(null); window.setTimeout(() => vpRef.current.scrollToPage(i, false), 120); } else vp.scrollToPage(i); },
    // a comment of the PDF Comments panel without text to select: InDesign shows the page that fits the window
    fitPage: (i) => { if (masterView) { setMasterView(null); window.setTimeout(() => vpRef.current.fit('page', i), 120); } else vp.fit('page', i); },
    openMaster: (id) => { setSelection([]); setSelectedPage(null); setMasterView(id); },
    openedMaster: masterView, openContext: (kind, at) => setContextMenu({ ...at, kind }), go, units, rulerOrigin, links, linksBusy, settings,
    onAddRoot: addRoot, onRemoveRoot: removeRoot, onRelink: () => relinkNow(settings.linkRoots), spreads,
    editable, edit: runEdit, textFormatting, openEffects: (page, level) => setFxDialog({ page, level }), tableSelection: editable ? tableSel : null,
    openColorPicker: colorPicker.open, history: editState.history, historyGo, historyCall, colorMenu, colorPanel, linkMenu, infoOptions: infoMenu.info, panelOptions: panelOptions.options,
    tool, formatText, setFormatText,
    revealText: setTextReveal, loadGlyph: (which, g) => loadGlyphIntoFind(glyphFind, which, g, () => setFindOpen(true)),
  };
  const render = (id: PanelId): React.ReactNode => {
    if (id==='notes') return notes.panel;
    if (id==='hyperlinks'||id==='crossReferences') return hyperlinks.panel(id==='crossReferences');
    if (id==='changes') return changes.panel;
    const registered = windowPanel(id, ctx, commands);
    if (registered) return registered;
    const Panel = CONTEXT_PANELS.get(id);
    if (Panel) return <Panel ctx={ctx} />;
    switch (id) {
      case 'pages': return <PagesPanel ctx={{ ...ctx, doc: realDoc, info: realInfo }} />;
      case 'paragraphStyles': return <StylesPanel ctx={ctx} kind="paragraph" />;
      case 'characterStyles': return <StylesPanel ctx={ctx} kind="character" />;
      case 'objectStyles': return <StylesPanel ctx={ctx} kind="object" />;
      case 'cellStyles': return <TableStylesPanel ctx={ctx} kind="cell" />;
      case 'tableStyles': return <TableStylesPanel ctx={ctx} kind="table" />;
      case 'align': return <AlignPanel control={alignment} options={panelOptions.options.align} />;
      case 'preflight':
        return doc ? <PreflightView report={preflight} busy={preflightBusy} onRerun={rerunPreflight} onGo={go} limit={preflightMenu.limit} /> : <p className="p-empty">{tr('문서가 열려 있지 않습니다.')}</p>;
      case 'separations':
        return doc && sepSupported ? (
          <SeparationsView report={inks} state={sep} setState={setSep} reading={reading} page={reading?.page ?? doc.pages[current]?.index ?? 0} />
        ) : <p className="p-empty">{doc ? tr('이 엔진에는 분판 미리 보기가 없습니다.') : tr('문서가 열려 있지 않습니다.')}</p>;
      case 'project':
        return project ? (
          <ProjectExplorer
            tree={project.tree}
            current={doc?.folio?.document.path ?? null}
            onOpen={(rel) => open(documentPath(project.info.root, rel))}
            onRefresh={() => loadProject(project.info.root)}
            onImport={async () => { const f = await window.folio.openDialog(); if (f) setImportSource(f); }}
            onNew={() => newDocument()}
          />
        ) : (
          <div className="p-empty">
            <p>{tr('열린 프로젝트가 없습니다.{0}', {0: directOpen ? tr(' 지금 문서는 읽기 전용으로 열었습니다.') : ''})}</p>
            <p><button onClick={() => commands['file.openProject'].run?.()}>{tr('프로젝트 열기…')}</button> <button onClick={openNewProject}>{tr('새 프로젝트…')}</button></p>
          </div>
        );
    }
  };
  // InDesign's panel menus (menutree.ts); Folio's own entries are in them
  const panelMenu = (id: PanelId): MenuEntry[] => (PANEL_MENUS as Record<string, MenuEntry[] | undefined>)[id] ?? [];
  const badge = (id: PanelId) => (id === 'links' ? links?.summary.missing || null : id === 'preflight' ? preflight?.summary.errors || null : null);
  const host = { render, menu: panelMenu, commands, badge };

  return {
    ctx, host,
  };
}
