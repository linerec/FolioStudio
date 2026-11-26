import { useAppState } from './useAppState';
import { saveUserSettings } from '../user-settings';
import { THEME_CHROME,NO_EDITS,type Theme } from './stateDefaults';
import { t as tr } from '../i18n';
import { preflightOnOpen } from '../ui/preflightmenu';
import type { SpellHandoff } from '../features/spelling/spelling-docs';
import { useCallback,useEffect,useRef,useState } from 'react';
import { flushSync } from 'react-dom';
import { setLanguage } from '../i18n';
import {
type Settings,
getInks,
getLinks,openDocument,runPreflight
} from '../engine';
import { getDocumentInfo } from '../inspect';
import {
importRun,
createProject,
baseName,isFolio,
isImportable,
projectInfo,
saveDocument,
saveDocumentCopy,
takeOverLock,useWindowTitle
} from '../project';
import { queue } from '../ui/docview';
import { measure } from '../ui/store';

export function useDocumentCommands(state: ReturnType<typeof useAppState>) {
  const {
    setLinksBusy, setPreflightBusy, setLinks,
    setPreflight, setSettings, settings,
    setTheme, settingsRef, doc,
    setProject, rememberProject, setError,
    setStatus, store, setDoc,
    setInfo, setMasterView, setSelection,
    setSelectedPage, setTarget, setInks,
    setReading, setSep, setEditState,
    setPageRev, setStage, setFirstPaint,
    setOpenedAt, setCurrent, vpRef,
    setSepSupported, setRecent, refreshRecentProjects,
    setImportSource, flash, editState,
    setUnsaved, allowClose, project,
    setDragging, setSpellHandoff, setSpellOpen,
  } = state;
  // --- links, preflight, settings --------------------------------------------------
  // the engine refuses every edit after an internal error (edit.rs editorBroken); the tools look alive, so say it
  const [engineBroken, setEngineBroken] = useState(false);
  const checkDocument = useCallback(async (roots: string[]) => {
    setLinksBusy(true);
    setPreflightBusy(true);
    try { setLinks(await getLinks(roots)); } catch (e) { console.error('links failed', String(e)); } finally { setLinksBusy(false); }
    // Preflight panel ▸ Enable Preflight for All Documents off: checked only on request
    if (!preflightOnOpen()) { setPreflight(null); setPreflightBusy(false); return; }
    try { setPreflight(await runPreflight()); } catch (e) { console.error('preflight failed', String(e)); } finally { setPreflightBusy(false); }
  }, []);
  const rerunPreflight = useCallback(async () => {
    setPreflightBusy(true);
    try { setPreflight(await runPreflight()); } catch (e) { console.error('preflight failed', String(e)); } finally { setPreflightBusy(false); }
  }, []);
  const updateSettings = useCallback(async (patch: Partial<Settings>) => {
    const next = await saveUserSettings(patch, settingsRef.current);
    setSettings(next);
    setLanguage(next.language ?? 'system', next.systemLanguage);
  }, []);
  useEffect(() => {
    const value = settings.preferences?.['folio.theme'];
    if (typeof value === 'string' && Object.hasOwn(THEME_CHROME, value)) setTheme(value as Theme);
  }, [settings.preferences?.['folio.theme']]);
  const chooseTheme = (value: Theme) => {
    void updateSettings({ preferences: { ...settingsRef.current.preferences, 'folio.theme': value } }).catch(e => console.error('theme settings', e));
  };
  const relinkNow = useCallback(async (roots: string[]) => {
    setLinksBusy(true);
    try { setLinks(await getLinks(roots)); } catch (e) { console.error('relink failed', String(e)); } finally { setLinksBusy(false); }
    rerunPreflight();
  }, [rerunPreflight]);
  const addRoot = useCallback(async () => {
    const folder = await window.folio.chooseFolder();
    if (!folder) return;
    const roots = [...settings.linkRoots.filter((r) => r.toLowerCase() !== folder.toLowerCase()), folder];
    await updateSettings({ linkRoots: roots });
    if (doc) relinkNow(roots);
  }, [settings, updateSettings, relinkNow, doc]);
  const removeRoot = useCallback(async (root: string) => {
    const roots = settings.linkRoots.filter((r) => r !== root);
    await updateSettings({ linkRoots: roots });
    if (doc) relinkNow(roots);
  }, [settings, updateSettings, relinkNow, doc]);

  // --- opening ------------------------------------------------------------------------
  const loadProject = useCallback(async (root: string) => {
    try {
      const { project: pinfo, tree } = await projectInfo(root);
      setProject({ info: pinfo, tree });
      rememberProject({ root: pinfo.root, id: pinfo.id, name: pinfo.name });
      return pinfo;
    } catch (e) {
      setError(String((e as Error).message ?? e));
      setStatus('error');
      return null;
    }
  }, [rememberProject]);

  const resetDocument = useCallback(() => {
    queue.clear();
    store.reset();
    setDoc(null);
    setInfo(null);
    setMasterView(null);
    setSelection([]);
    setSelectedPage(null);
    setPreflight(null);
    setLinks(null);
    setTarget(null);
    setInks(null);
    setReading(null);
    setSep((s) => ({ ...s, hidden: [] }));
    setEditState(NO_EDITS);
    setEngineBroken(false);
    setPageRev(new Map());
    measure.set(null);
  }, [store]);

  /** Opens a .folio, or an INDD/IDML read-only (no project), in the engine. */
  /** `discard`: no question about unsaved changes (Save As and Revert have dealt with them). */
  const openDoc = useCallback(async (file: string, discard = false) => {
    if (!discard && !(await confirmDiscardRef.current())) return;
    resetDocument();
    setError('');
    setStage('');
    setFirstPaint(null);
    setStatus('opening');
    const t0 = performance.now();
    setOpenedAt(t0);
    try {
      shown(await openDocument(file));
    } catch (e) {
      setError(String((e as Error).message ?? e));
      setStatus('error');
    }
    window.folio.recent().then(setRecent, () => {});
    refreshRecentProjects();
  }, [resetDocument, checkDocument, loadProject, refreshRecentProjects]);
  /** The window shows a document the engine has opened (its own File ▸ Open, or an attached LLM client's open). */
  const shown = (summary: Awaited<ReturnType<typeof openDocument>>) => {
    setDoc(summary);
    setStatus('ready');
    setCurrent(0);
    vpRef.current.fit('spread', 0);
    if (summary.folio) loadProject(summary.folio.project.root);
    checkDocument(settingsRef.current.linkRoots);
    getDocumentInfo().then(setInfo, (e) => console.error('documentInfo failed', String(e)));
    getInks().then((r) => { setSepSupported(r !== null); setInks(r); }, (e) => console.error('inks failed', String(e)));
  };
  // what an attached LLM client (folio-mcp --attach) opened or wrote in the project is shown at once (engine attach_paths.rs);
  // the engine refuses its open while this document has unsaved changes
  const projectRoot = project?.info.root;
  useEffect(() => window.folio.onEvent((event) => {
    const detail = event.detail as { document?: Awaited<ReturnType<typeof openDocument>>; kind?: string; file?: string } | undefined;
    if (event.event === 'external-open' && detail?.document) {
      // the closed document's views go first (as while File ▸ Open waits), so the same file opened again is read afresh
      flushSync(() => { resetDocument(); setError(''); });
      shown(detail.document);
      flash(tr('LLM 이 문서를 열었습니다: {0}', { 0: baseName(String(event.path ?? '')) }));
      window.folio.recent().then(setRecent, () => {});
    } else if (event.event === 'external-file' && detail?.kind) {
      if (detail.kind === 'save' && doc && event.path === doc.path) setEditState((s) => ({ ...s, dirty: false }));
      flash(detail.kind === 'save' ? tr('LLM 이 저장했습니다: {0}', { 0: baseName(detail.file ?? '') }) : tr('LLM 이 파일을 썼습니다: {0}', { 0: baseName(detail.file ?? '') }));
      if (projectRoot) loadProject(projectRoot);
    }
  }), [doc?.path, projectRoot, resetDocument, loadProject, checkDocument]);

  const open = useCallback(async (path?: string | null) => {
    const file = path ?? (await window.folioProject.openAny());
    if (!file) return;
    // INDD and IDML: import into a project, or open read-only (principle 1, user's exception)
    if (isImportable(file)) { setImportSource(file); return; }
    if (!isFolio(file)) {
      if (await loadProject(file)) { resetDocument(); setStatus('idle'); }
      return;
    }
    openDoc(file);
  }, [loadProject, resetDocument, openDoc]);

  /** Help ▸ Open Feature Sample: the Atlas IDML shipped with the app, in its own project under
   *  Documents, imported again when the app or the sample is new (electron/sample.cjs). */
  const openSample = useCallback(async (kind: 'atlas' | 'magazine' = 'atlas') => {
    if (doc) {
      if (!(await confirmDiscardRef.current())) return;
      try { await window.folio.request({ cmd: 'close' }); } catch { /* engine gone */ }
      resetDocument();
    }
    setError('');
    setStage('');
    setStatus('opening');
    try {
      const s = await window.folio.sample(kind);
      if ('error' in s) throw new Error(s.error === 'missing' ? tr('이 앱에 선택한 샘플이 들어 있지 않습니다.') : s.error);
      if (!s.project) await createProject(s.root, 'Feature Atlas');
      if (s.import) {
        setStage('reader');
        await importRun({ source: s.source, project: s.root, document: baseName(s.document), roots: [] });
        await window.folio.sampleImported(s.document);
      }
      await openDoc(s.document);
      if (s.kept) flash(tr('고친 표본은 그대로 두고 새로 가져왔습니다: {0}', { 0: baseName(s.kept) }));
    } catch (e) {
      setError(String((e as Error).message ?? e));
      setStatus('error');
    }
  }, [doc, resetDocument, openDoc, flash]);

  const closeDocument = useCallback(async () => {
    if (!doc) return;
    if (!(await confirmDiscardRef.current())) return;
    try { await window.folio.request({ cmd: 'close' }); } catch { /* engine gone */ }
    resetDocument();
    setStatus('idle');
  }, [doc, resetDocument]);

  const save = useCallback(async () => {
    if (!doc?.folio || doc.folio.readOnly) return;
    try {
      const r = await saveDocument();
      setEditState((s) => ({ ...s, dirty: false }));
      flash(tr('저장함 {0}초{1}', {0: (r.ms / 1000).toFixed(2), 1: r.healed ? tr(' · 옮겨진 참조 {0}개 고침', {0: r.healed}) : ''}));
      return true;
    } catch (e) {
      flash(String((e as Error).message ?? e));
      return false;
    }
  }, [doc, flash]);

  // File ▸ Save a Copy: the document as it is now to another .folio in the project; File ▸ Save
  // As: the same, then that copy is the open document (this one is closed unsaved: its changes
  // are in the copy). A copy in another project is refused by the engine (principle 1).
  const saveCopy = useCallback(async (andOpen: boolean) => {
    if (!doc?.folio) return;
    const base = doc.path.replace(/\.folio$/i, '');
    const target = await window.folioProject.saveFolio(`${base} ${tr('사본')}.folio`);
    if (!target) return;
    try {
      const r = await saveDocumentCopy(target);
      if (andOpen) await openDoc(r.path, true);
      else flash(tr('사본을 저장했습니다: {0}', { 0: r.path }));
    } catch (e) {
      flash(String((e as Error).message ?? e));
    }
  }, [doc, flash, openDoc]);
  // File ▸ Revert: the document as last saved (asks first when there are changes)
  const revert = useCallback(async () => {
    if (!doc?.folio) return;
    if (editState.dirty && !window.confirm(tr('마지막으로 저장한 상태로 되돌릴까요? 저장하지 않은 변경은 사라집니다.'))) return;
    await openDoc(doc.path, true);
  }, [doc, editState.dirty, openDoc]);

  // unsaved changes: asked before closing, opening another document or quitting
  const confirmDiscard = useCallback(async (): Promise<boolean> => {
    if (!editState.dirty) return true;
    return new Promise<boolean>((resolve) => setUnsaved({ resolve }));
  }, [editState.dirty]);
  const confirmDiscardRef = useRef(confirmDiscard);
  confirmDiscardRef.current = confirmDiscard;
  // Save All from another window: a document that cannot be saved here (read-only, or none) says so instead
  const saveRef = useRef(async () => false as boolean | undefined);
  saveRef.current = async () => {
    if (!doc) return false;
    if (!doc.folio || doc.folio.readOnly) { flash(tr('이 창의 문서는 읽기 전용이라 저장하지 않았습니다.')); return false; }
    return editState.dirty ? save() : true;
  };
  useEffect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (!editState.dirty || allowClose.current) return;
      e.preventDefault();
      e.returnValue = false;
      confirmDiscardRef.current().then((go) => { if (go) { allowClose.current = true; window.close(); } });
    };
    window.addEventListener('beforeunload', onBeforeUnload);
    return () => window.removeEventListener('beforeunload', onBeforeUnload);
  }, [editState.dirty]);

  const overrideLock = useCallback(async () => {
    if (!doc?.folio) return;
    if (!window.confirm(tr('다른 사용자의 잠금을 가져옵니다. 그 사용자가 문서를 이미 닫았다고 확실할 때만 하세요. 계속할까요?'))) return;
    try {
      const lock = await takeOverLock();
      setDoc({ ...doc, folio: { ...doc.folio, lock, readOnly: lock.readOnly } });
    } catch (e) {
      flash(String((e as Error).message ?? e));
    }
  }, [doc, flash]);

  useWindowTitle(project?.info ?? null, doc?.folio, doc?.path ?? null);

  useEffect(() => {
    window.folio.recent().then(setRecent, () => {});
    window.folio.settings().then(setSettings, () => {});
    return window.folio.onEvent(e => { if(e.event === 'user-settings') window.folio.recent().then(setRecent, () => {}); });
  }, []);

  // drop an INDD/IDML/.folio anywhere on the window
  useEffect(() => {
    let depth = 0;
    const hasFiles = (e: DragEvent) => !!e.dataTransfer?.types.includes('Files');
    const onEnter = (e: DragEvent) => { if (hasFiles(e)) { depth++; setDragging(true); } };
    const onLeave = (e: DragEvent) => { if (hasFiles(e) && --depth <= 0) { depth = 0; setDragging(false); } };
    const onOver = (e: DragEvent) => { if (hasFiles(e)) e.preventDefault(); };
    const onDrop = (e: DragEvent) => {
      e.preventDefault();
      depth = 0;
      setDragging(false);
      const files = Array.from(e.dataTransfer?.files ?? []);
      const file = files.map((f) => window.folio.pathForFile(f)).find((p) => /\.(indd|indt|idml|folio)$/i.test(p));
      if (file) open(file);
      else if (files.length) { setError(tr('.folio 문서, 또는 InDesign 문서(INDD·IDML)만 열 수 있습니다.')); setStatus('error'); }
    };
    window.addEventListener('dragenter', onEnter);
    window.addEventListener('dragleave', onLeave);
    window.addEventListener('dragover', onOver);
    window.addEventListener('drop', onDrop);
    return () => {
      window.removeEventListener('dragenter', onEnter);
      window.removeEventListener('dragleave', onLeave);
      window.removeEventListener('dragover', onOver);
      window.removeEventListener('drop', onDrop);
    };
  }, [open]);

  useEffect(
    () =>
      window.folio.onEvent((e) => {
        if (e.event === 'progress' && e.stage) setStage(e.stage);
        if (e.event === 'open-path' && e.path) open(e.path);
        if (e.event === 'spell-handoff' && e.handoff) { setSpellHandoff(h => ({ n: (h?.n ?? 0) + 1, handoff: e.handoff as SpellHandoff })); setSpellOpen(true); }
        if (e.event === 'engine-exit') { setError(tr('엔진 프로세스가 종료되었습니다.')); setStatus('error'); }
        // the close button of this window's tab (electron/appwin-shell.cjs): the same question as the window's own close button
        if (e.event === 'appwin-close') void confirmDiscardRef.current().then((go) => { if (go) { allowClose.current = true; window.close(); } });
        // File ▸ Save All from another window (electron/appwin-shell.cjs saveDoc): this window saves its document as its own Save does
        if (e.event === 'appwin-save') void saveRef.current();
      }),
    [open],
  );

  // a document given on the command line opens once
  const initialDone = useRef(false);
  useEffect(() => {
    if (initialDone.current) return;
    initialDone.current = true;
    window.folio.initialPath().then((p) => { if (p) open(p); });
  }, [open]);

  return {
    checkDocument, setEngineBroken, relinkNow,
    updateSettings, open, closeDocument,
    resetDocument, save, saveCopy,
    revert, chooseTheme, addRoot,
    rerunPreflight, openSample, openDoc,
    loadProject, removeRoot, engineBroken,
    overrideLock,
  };
}
