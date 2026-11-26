import { useViewPreference } from '../features/view/preferences';
import { useDataMerge } from '../features/datamerge/state';
import { usePdfComments } from '../features/pdfcomments/state';
import { type TransformKind } from '../ui/transformdialogs';
import { anchorOf } from './shared';
import type { SpellHandoff } from '../features/spelling/spelling-docs';
import { AUTOCORRECT_KEY,spellTyped } from '../features/spelling/spelling';
import { typedAutoCorrect } from '../features/spelling/typing';
import { cleanAutoCorrectTables } from '../features/preferences/autocorrect';
import { cleanPresets } from '../ui/docpresets';
import { openNewDocument } from '../project';
import { useCallback,useEffect,useMemo,useRef,useState } from 'react';
import { useLanguage } from '../i18n';
import {
type DocumentSummary,
type InkReading,
type InksReport,
type LinksReport,
type PreflightReport,
type Settings
} from '../engine';
import { type DocumentInfo,ItemStore } from '../inspect';
import { type Target } from '../print';
import {
type ProjectInfo,
type ProjectTree,useRecentProjects
} from '../project';
import { SEP_DEFAULT,type SepState } from '../separations';
import { isMac } from '../ui/commands';
import { type RefPoint } from '../ui/control';
import { useDock } from '../ui/dock';
import { useWorkbench } from '../ui/workbench';
import { fileName,groupRows,queue,useViewport } from '../ui/docview';
import { type GuideSelection } from '../ui/guides';
import { type FxLevel,type FxPage } from '../ui/effectsdialog';
import { type SelEntry } from '../ui/overlay';
import { type ScreenMode,type ToolId } from '../ui/tools';
import { useWindows } from '../ui/windows';
import { useAppWindows } from '../ui/appwindows';
import { toolsLayout } from '../ui/tools';
import { defaultView,IGNORE_LOCAL_KEY,type ViewDisplay } from '../ui/displayperf';
import { useProofSetup } from '../ui/proofsetup';
import { type DecorationsMode } from '../ui/paragraph-decorations';
import { THEME_CHROME,NO_EDITS,stored,keep,type Theme,type Status } from './stateDefaults';
import { userSettings } from '../user-settings';

export function useAppState() {
  const language = useLanguage();
  const [realDoc, setDoc] = useState<DocumentSummary | null>(null);
  const pdfComments = usePdfComments(realDoc);
  const dataMerge = useDataMerge(realDoc);
  const [status, setStatus] = useState<Status>('idle');
  const [stage, setStage] = useState('');
  const [error, setError] = useState('');
  const [spreads, setSpreads] = useState(true);
  // View ▸ Show Hidden Spreads: a setting of the program (InDesign keeps it outside the document), remembered on this computer
  const [showHidden, setShowHidden] = useState(() => { try { return userSettings.getItem('folio.showHiddenSpreads') === '1'; } catch { return false; } });
  const toggleShowHidden = () => setShowHidden((v) => { try { userSettings.setItem('folio.showHiddenSpreads', v ? '0' : '1'); } catch { /* this computer only */ } return !v; });
  const [lastRender, setLastRender] = useState<number | null>(null);
  const [openedAt, setOpenedAt] = useState(0);
  const [firstPaint, setFirstPaint] = useState<number | null>(null);
  const [recent, setRecent] = useState<string[]>([]);
  const [dragging, setDragging] = useState(false);
  const [settings, setSettings] = useState<Settings>({ linkRoots: [], exportFolder: null, export: null, packageFolder: null, package: null, packageContact: null });
  const [preflight, setPreflight] = useState<PreflightReport | null>(null);
  const [preflightBusy, setPreflightBusy] = useState(false);
  const [links, setLinks] = useState<LinksReport | null>(null);
  const [linksBusy, setLinksBusy] = useState(false);
  const [target, setTarget] = useState<Target | null>(null);
  const [inks, setInks] = useState<InksReport | null>(null);
  const [sepSupported, setSepSupported] = useState(true);
  const [sep, setSep] = useState<SepState>(SEP_DEFAULT);
  const [reading, setReading] = useState<InkReading | null>(null);
  const settingsRef = useRef(settings);
  settingsRef.current = settings;
  const [project, setProject] = useState<{ info: ProjectInfo; tree: ProjectTree } | null>(null);
  const [importSource, setImportSource] = useState<string | null>(null);
  // File ▸ New ▸ Document… with no project open: the notice that asks for one (the dialog itself is the engine's)
  const [newDocNotice, setNewDocNotice] = useState(false);
  const [presetsOpen, setPresetsOpen] = useState(false);
  // Object ▸ Transform ▸ Move…, Scale…, Rotate…, Shear… (the objects it applies to, as selected when it opened)
  const [eyedropperDialog, setEyedropperDialog] = useState(false);
  const [pencilDialog, setPencilDialog] = useState(false);
  const [polygonDialog, setPolygonDialog] = useState(false);
  const [spellOpen, setSpellOpen] = useState(false);
  // Check Spelling > All Documents: the check came from another window (electron/spell-docs.cjs) and goes on in this window's document; `n` makes a new dialog of it
  const [spellHandoff, setSpellHandoff] = useState<{ n: number; handoff: SpellHandoff } | null>(null);
  // Typing rules of the engine (autocorrect words, typographer's quotes): the text editor asks this hook (features/spelling/typing.ts) for every typed text.
  // The switch is an argument (quotes follow the document, not the switch).
  useEffect(() => {
    typedAutoCorrect.current = async (story, at, text, run) => {
      const autoCorrect = settings.preferences?.[AUTOCORRECT_KEY] === true;
      const tables = cleanAutoCorrectTables(settings.autoCorrectTables);
      const capitalization = settings.preferences?.['autoCorrectPreferences.autoCorrectCapitalizationErrors'] === true;
      const { fix, run: next } = await spellTyped({ story, at, text, tables, autoCorrectCapitalizationErrors: capitalization, autoCorrect, typedRun: run });
      if (!fix) return { run: next, fix: null };
      const typed = Array.from(text);
      // a word corrected by one typed character (a space, a full stop ...) is two undo steps, as in InDesign: the correction, then the character
      const split = fix.start < at && typed.length === 1 ? { start: fix.start, end: at, word: Array.from(fix.replacement).slice(0, -1).join(''), typed: text, typedFirst: text === String.fromCharCode(13) } : undefined;
      return { run: next, fix: { command: { op: 'typeText', story, at, text, tables, capitalization, typed_run: run > 0, auto_correct: autoCorrect },
        caretAt: fix.start + Array.from(fix.replacement).length, split } };
    };
    return () => { typedAutoCorrect.current = null; };
  }, [settings.preferences, settings.autoCorrectTables]);
  const [shortcutDialog, setShortcutDialog] = useState(false);
  const presets = useMemo(() => cleanPresets(settings.documentPresets), [settings.documentPresets]);
  const [idmlExport, setIdmlExport] = useState(false);
  const [note, setNote] = useState('');
  const noteTimer = useRef(0);
  const { recentProjects, rememberProject, refreshRecentProjects } = useRecentProjects();
  // editor state
  const store = useMemo(() => new ItemStore(), []);
  const [selectedGuide, setSelectedGuide] = useState<GuideSelection>(null);
  const [realInfo, setInfo] = useState<DocumentInfo | null>(null);
  // A master spread opened like a document (Pages panel, double click): the document window, the rulers, the control bar and the
  // commands then see its pages (numbered from 1,000,000) instead of the document's; the Pages panel keeps the real ones.
  const [masterView, setMasterView] = useState<string | null>(null);
  const masterPages = useMemo(() => (masterView ? (realInfo?.masterPages ?? []).filter((m) => m.masterSpread === masterView) : []), [masterView, realInfo]);
  const doc = useMemo<DocumentSummary | null>(() => {
    if (!realDoc || !masterPages.length) return realDoc;
    const name = realInfo?.masters.find((m) => m.id === masterView)?.name;
    return { ...realDoc, masterName: name, pages: masterPages.map((m) => ({ id: m.id, index: m.index, name: m.name, width: m.size[0], height: m.size[1], spread: m.masterSpread })) };
  }, [realDoc, realInfo, masterView, masterPages]);
  const info = useMemo<DocumentInfo | null>(() => (realInfo && masterPages.length ? { ...realInfo, pages: masterPages } : realInfo), [realInfo, masterPages]);
  const [fxDialog, setFxDialog] = useState<{ page: FxPage; level?: FxLevel } | null>(null);
  const [lightDialog, setLightDialog] = useState(false);
  const [selection, setSelection] = useState<SelEntry[]>([]);
  const [selectedPage, setSelectedPage] = useState<number | null>(null);
  const [current, setCurrent] = useState(0);
  const [tool, setTool] = useState<ToolId>('selection');
  const [tempTool, setTempTool] = useState<ToolId | null>(null);
  const [doubleTools, setDoubleTools] = useState(() => stored<string>('folio.tools.double', '0') === '1');
  const [screenMode, setScreenMode] = useViewPreference<ScreenMode>('mode', 'normal', ['normal', 'preview', 'bleed', 'slug']);
  const [guides, setGuides] = useViewPreference<boolean>('guides', true);
  const [edges, setEdges] = useViewPreference<boolean>('edges', true);
  const [hiddenChars, setHiddenChars] = useViewPreference<boolean>('hiddenChars', false);
  const [showNotes, setShowNotes] = useViewPreference<boolean>('showNotes', true);
  const [threads, setThreads] = useViewPreference<boolean>('threads', false);
  const [rulers, setRulers] = useViewPreference<boolean>('rulers', true);
  const [contentGrabber, setContentGrabber] = useViewPreference<boolean>('contentGrabber', true);
  const toolLayout = toolsLayout(settings.preferences?.['generalPreferences.toolsPanel']);
  const [theme, setTheme] = useState<Theme>(() => stored<Theme>('folio.theme', 'dark'));
  const [unitsPref, setUnitsPref] = useState<string>(() => stored<string>('folio.units', 'document'));
  const [refPoint, setRefPoint] = useState<RefPoint>([0, 0]);
  // Object ▸ Transform ▸ Move…, Scale…, Rotate…, Shear…: the engine session holds the dialog (next/shared/dialogs.json); the control panel's
  // reference point is a screen state, so it goes in as the dialog's anchor
  const refPointRef = useRef(refPoint); refPointRef.current = refPoint;
  const openTransformDialog = useCallback((d: { kind: TransformKind; ids: string[] }) => {
    void window.folio.request({ cmd: 'openDialog', dialog: d.kind, ids: d.ids, values: d.kind === 'move' ? {} : { anchor: anchorOf(refPointRef.current) } });
  }, []);
  const [textMode, setTextMode] = useState<'character' | 'paragraph'>('character');
  // Which of the toolbox's fill and stroke is in front (X): the object widget and the T widget (a text frame with T on, or characters selected) each
  // remember their own (InDesign 2025, real input: alternating X and the T button; toolbox-c3 s6), and neither follows the selection.
  const [objectStroke, setObjectStroke] = useState(false);
  const [textStroke, setTextStroke] = useState(false);
  const textWidgetRef = useRef(false);
  const setStrokeFocus = useCallback((v: boolean | ((x: boolean) => boolean)) => (textWidgetRef.current ? setTextStroke : setObjectStroke)(v), []);
  const [formatText, setFormatText] = useState(false);
  const [allHidden, setAllHidden] = useState(false);
  const [showControl, setShowControl] = useViewPreference<boolean>('control', true);
  const [showTools, setShowTools] = useViewPreference<boolean>('tools', true);
  const [presenting, setPresenting] = useState(false);
  const [presentationBackground, setPresentationBackground] = useState<import('../ui/presentationkeys').PresentationBackground>('black');
  const [dialog, setDialog] = useState<'about' | 'shortcuts' | 'limits' | 'corners' | 'variables' | 'inkManager' | 'credentialStoreTest' | null>(null);
  const [paragraphDecorationsOpen,setParagraphDecorationsOpen]=useState<DecorationsMode|false>(false);
  const [dropCapsOpen,setDropCapsOpen]=useState(false);
  const [wsMenu, setWsMenu] = useState<DOMRect | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; kind: string } | null>(null);
  const dock = useDock();
  // Folio's sidebar or InDesign Classic (ui/workbench.ts: the engine's user settings)
  const workbench = useWorkbench();
  // editing: undo/redo names, unsaved changes, pages to draw again
  const [editState, setEditState] = useState<{ undo: string | null; redo: string | null; dirty: boolean; history: { undo: string[]; redo: string[] } }>(NO_EDITS);
  const [pageRev, setPageRev] = useState<Map<number, number>>(() => new Map());
  const [unsaved, setUnsaved] = useState<{ resolve: (go: boolean) => void } | null>(null);
  const allowClose = useRef(false);

  const readOnly = !!doc && (!doc.folio || doc.folio.readOnly);
  const directOpen = !!doc && !doc.folio;
  const editable = !!doc?.editable && !readOnly;
  // Display Performance (ui/displayperf.ts): the window's view setting, the application default until one is chosen
  const [viewDisplayChoice, setViewDisplayChoice] = useState<ViewDisplay | null>(null);
  // a window starts with the application default of that moment (changing the preference later leaves open windows as they are)
  useEffect(() => setViewDisplayChoice(doc ? defaultView(settingsRef.current.preferences) : null), [doc?.path]);
  const viewDisplay = viewDisplayChoice ?? defaultView(settings.preferences);
  const allowLocalDisplay = settings.preferences?.[IGNORE_LOCAL_KEY] !== true;
  const pageCountRef = useRef(0);
  pageCountRef.current = doc?.pages.length ?? 0;
  // the render queue sends it with every page; the pages already drawn are drawn again
  useEffect(() => {
    queue.display = { view: viewDisplay, ignoreLocal: !allowLocalDisplay };
    setPageRev((m) => { const next = new Map(m); for (let i = 0; i < pageCountRef.current; i++) next.set(i, (m.get(i) ?? 0) + 1); return next; });
  }, [viewDisplay, allowLocalDisplay]);
  // View ▸ Proof Colors / Proof Setup (ui/proofsetup.tsx): the engine holds the setup; the pages are drawn again when it changes
  const redrawAll = useCallback(() => {
    setPageRev((m) => { const next = new Map(m); for (let i = 0; i < pageCountRef.current; i++) next.set(i, (m.get(i) ?? 0) + 1); return next; });
  }, []);
  const proof = useProofSetup({ hasDoc: !!doc, workingCmyk: String(settings.preferences?.['colorSettings.workingSpaceCMYK'] ?? 'U.S. Web Coated (SWOP) v2'), redraw: redrawAll });
  const rows = useMemo(() => (doc ? groupRows(doc.pages, spreads, showHidden ? null : doc.pageSettings?.hidden) : []), [doc, spreads, showHidden]);
  const primaryVp = useViewport(rows);
  // the windows of the document (Window > Arrange); the viewport commands act on the active window's
  const wins = useWindows({ hasDoc: !!doc, docKey: doc?.path ?? null });
  // the app windows (one per document) that sit together as tabs, tiles or floating; Window > Arrange acts on them when more than one is open
  const appWins = useAppWindows(doc ? fileName(doc.path) : null, editState.dirty, wins.handlers);
  const vp = wins.forward(primaryVp);
  // callbacks that only need the latest viewport read it here (vp is a new object each render)
  const vpRef = useRef(vp);
  vpRef.current = vp;
  // opening or leaving a master: the view starts at its first page
  useEffect(() => {
    const first = rows[0]?.[0]?.index;
    if (first === undefined) return;
    setCurrent(first);
    window.setTimeout(() => vpRef.current.scrollToPage(first, false), 60);
  }, [masterView]);
  const effectiveTool = tempTool ?? tool;

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    document.documentElement.dataset.platform = isMac ? 'mac' : 'other';
    keep('folio.theme', theme);
    const [bg, fg] = THEME_CHROME[theme];
    window.folioWindow?.setChrome(bg, fg);
  }, [theme]);
  useEffect(() => keep('folio.units', unitsPref), [unitsPref]);

  const flash = useCallback((text: string) => {
    setNote(text);
    window.clearTimeout(noteTimer.current);
    noteTimer.current = window.setTimeout(() => setNote(''), 6000);
  }, []);
  // File ▸ New ▸ Document… (docpreset: the document preset it starts from, '' [Default]): the engine's dialog for the open project
  const projectRoot = project?.info.root ?? null;
  const newDocument = useCallback((docpreset = '') => {
    if (!projectRoot) { setNewDocNotice(true); return; }
    void openNewDocument(projectRoot, docpreset).then(({ header }) => { if (!header.ok) flash(String(header.error)); });
  }, [projectRoot, flash]);
  return {
    language, realDoc, setDoc,
    pdfComments, dataMerge, status,
    setStatus, stage, setStage,
    error, setError, spreads,
    setSpreads, showHidden, setShowHidden,
    toggleShowHidden, lastRender, setLastRender,
    openedAt, setOpenedAt, firstPaint,
    setFirstPaint, recent, setRecent,
    dragging, setDragging, settings,
    setSettings, preflight, setPreflight,
    preflightBusy, setPreflightBusy, links,
    setLinks, linksBusy, setLinksBusy,
    target,
    setTarget, inks, setInks,
    sepSupported, setSepSupported, sep,
    setSep, reading, setReading,
    settingsRef, project, setProject,
    importSource, setImportSource, newDocNotice,
    setNewDocNotice, newDocument, presetsOpen, setPresetsOpen,
    openTransformDialog, eyedropperDialog,
    setEyedropperDialog, pencilDialog, setPencilDialog,
    polygonDialog, setPolygonDialog,
    spellOpen, setSpellOpen,
    spellHandoff, setSpellHandoff,
    shortcutDialog, setShortcutDialog,
    presets, idmlExport, setIdmlExport,
    note, setNote, noteTimer,
    recentProjects, rememberProject, refreshRecentProjects,
    store, selectedGuide, setSelectedGuide,
    realInfo, setInfo, masterView,
    setMasterView, masterPages, doc,
    info, fxDialog, setFxDialog,
    lightDialog, setLightDialog,
    selection, setSelection,
    selectedPage, setSelectedPage, current,
    setCurrent, tool, setTool,
    tempTool, setTempTool, doubleTools,
    setDoubleTools, screenMode, setScreenMode,
    guides, setGuides, edges,
    setEdges, hiddenChars, setHiddenChars,
    showNotes, setShowNotes, threads,
    setThreads, rulers, setRulers,
    contentGrabber, setContentGrabber, toolLayout,
    theme, setTheme, unitsPref,
    setUnitsPref, refPoint, setRefPoint,
    textMode, setTextMode, objectStroke,
    setObjectStroke, textStroke, setTextStroke,
    textWidgetRef, setStrokeFocus, formatText,
    setFormatText, allHidden, setAllHidden,
    showControl, setShowControl, showTools,
    setShowTools, presenting, setPresenting, presentationBackground, setPresentationBackground,
    dialog, setDialog,
    paragraphDecorationsOpen,
    setParagraphDecorationsOpen, dropCapsOpen, setDropCapsOpen,
    wsMenu,
    setWsMenu, contextMenu, setContextMenu,
    dock, workbench, editState, setEditState,
    pageRev, setPageRev, unsaved,
    setUnsaved, allowClose, readOnly,
    directOpen, editable, viewDisplayChoice,
    setViewDisplayChoice, viewDisplay, allowLocalDisplay,
    pageCountRef, redrawAll, proof,
    rows, primaryVp, wins,
    appWins, vp, vpRef,
    effectiveTool, flash,
  };
}
