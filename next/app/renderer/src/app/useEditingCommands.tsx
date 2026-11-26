import { useViewCommands } from './useViewCommands';
import { useDocumentCommands } from './useDocumentCommands';
import { useAppState } from './useAppState';
import { NO_EDITS } from './stateDefaults';
import { PreviewSession } from '../ui/controls/previewsession';
import { t as tr } from '../i18n';
import { authored } from '../features/editorial/authored';
import { usePdfCommentsImport } from '../features/pdfcomments/import';
import { useCallback,useEffect,useMemo,useRef } from 'react';
import { flushSync } from 'react-dom';
import {
type EditCommand,
type EditResult,getInks,editDocument
} from '../engine';
import { getDocumentInfo } from '../inspect';
import { engineText } from '../ui/place';
import { findItem } from '../ui/overlay';

export function useEditingCommands(state: ReturnType<typeof useAppState>, documents: ReturnType<typeof useDocumentCommands>, viewCommands: ReturnType<typeof useViewCommands>) {
  const {
    selection, setEditState, setDoc,
    setCurrent, setSelectedPage, setPageRev,
    store, setSelection, setInfo,
    setInks, settingsRef, editable,
    current, flash, pdfComments,
    project, dock, doc,
  } = state;
  const {
    checkDocument, setEngineBroken,
  } = documents;
  const {
    go,
  } = viewCommands;
  // --- editing (folio-edit: the same changes InDesign makes, next/oracle) --------------
  const selectionRef = useRef(selection);
  selectionRef.current = selection;
  const checkTimer = useRef(0);
  const afterEdit = useCallback((r: EditResult) => {
    // History commands must reflect this reply before another engine response can trigger the next command.
    flushSync(() => setEditState({ undo: r.undo, redo: r.redo, dirty: r.dirty, history: r.history ?? NO_EDITS.history }));
    if (r.document) {
      const last = r.document.pages.length - 1;
      setDoc(r.document);
      setCurrent(p => Math.min(p, last));
      setSelectedPage(p => p === null ? null : Math.min(p, last));
    }
    const livePages = r.pages.filter(p => !r.document || p < r.document.pages.length);
    if (r.pages.length) {
      setPageRev((m) => {
        const n = new Map(m);
        for (const pg of r.pages) n.set(pg, (n.get(pg) ?? 0) + 1);
        return n;
      });
      store.invalidate(r.pages);
      // what was deleted (or undone into being) leaves the selection once its page is read
      Promise.all(livePages.map((pg) => store.load(pg))).then(() =>
        setSelection((sel) => sel.filter((e) => { const items = store.peek(e.page); return items === undefined || items === null || !!findItem(items, e.id); })),
      );
    }
    if (r.parts.some((p) => p === 'designmap.xml' || p.startsWith('Resources/') || p.startsWith('Stories/') || p.startsWith('Spreads/') || p.startsWith('MasterSpreads/'))) getDocumentInfo().then(setInfo, () => {});
    // the plates follow the Ink Manager (conversions, aliases)
    if (r.parts.some((p) => p === 'Resources/Graphic.xml')) getInks().then((i) => { if (i) setInks(i); }, () => {});
    if (r.overset !== undefined) setDoc((d) => (d ? { ...d, overset: r.overset! } : d));
    // links and preflight follow the edits, a moment after the last
    window.clearTimeout(checkTimer.current);
    checkTimer.current = window.setTimeout(() => checkDocument(settingsRef.current.linkRoots), 1500);
  }, [store, checkDocument]);
  // what an attached LLM client (folio-mcp --attach) changed in this engine session is drawn at once, as after the screen's own edit
  useEffect(() => window.folio.onEvent((event) => {
    if (event.event !== 'external-edit' || !doc || event.path !== doc.path) return;
    const r = (event.detail as { edit: EditResult }).edit;
    afterEdit(r);
    flash(tr('LLM 이 문서를 바꿨습니다: {0}', { 0: r.label }));
  }), [afterEdit, flash, doc?.path]);

  const selectionStale = useRef(false);
  const selectionKey = selection.map((s) => `${s.id}${s.content ? '*' : ''}`).join(',');
  useEffect(() => { selectionStale.current = true; }, [selectionKey]);
  // inside previewScope the edits runEdit is asked for are shown provisionally by the engine's preview session instead of being made
  const divertEdits = useRef(false);
  const previewSessionRef = useRef<PreviewSession | null>(null);
  const previewScope = useCallback((fn: () => void) => {
    divertEdits.current = true;
    try { fn(); } finally { divertEdits.current = false; }
  }, []);
  const runEdit = useCallback(async (label: string, commands: EditCommand[], extra?: Record<string, unknown>): Promise<EditResult | null> => {
    if (!editable || !commands.length) return null;
    if (divertEdits.current && previewSessionRef.current) {
      previewSessionRef.current.apply(label, authored(commands, String(settingsRef.current.preferences?.['app.userName'] || 'Unknown User Name')) as unknown as Record<string, unknown>[]);
      return null;
    }
    try {
      // a new selection since the last edit starts a new Transform Sequence Again sequence (engine: Editor::selection_changed)
      const changed = selectionStale.current;
      selectionStale.current = false;
      const r = await editDocument(label, authored(commands,String(settingsRef.current.preferences?.['app.userName']||'Unknown User Name')), changed, extra);
      afterEdit(r);
      // what an edit made (copies, a group) becomes the selection, on the page of the first selected
      if (r.created?.length && !commands.some(c => c.op === 'insertPages' || c.op === 'duplicateSpread' || c.op === 'overrideParentItems' || c.op === 'insertFootnote' || c.op === 'saveAsParent' || c.op === 'loadParentPages' || c.op === 'saveTrapPreset' || c.op === 'duplicateTrapPreset')) {
        const toc = commands.some(c=>c.op==='createTOC'&&!c.frame);
        const page = toc ? 0 : selectionRef.current[0]?.page ?? current;
        setSelection(r.created.map((id) => ({ page, id, content: false })));
        if(toc) go({page:0,bounds:[36,36,324,420]});
      }
      return r;
    } catch (e) {
      const msg = String((e as Error).message ?? e);
      if ((e as { code?: string }).code === 'editorBroken') setEngineBroken(true);
      // what the engine does not support is refused with a plain sentence; the document has not been touched (screens-doc)
      flash((e as { code?: string }).code === 'unsupported' ? tr('{0}: 지원하지 않는 작업이라 거절했고 문서는 바뀌지 않았습니다. ({1})', { 0: label, 1: engineText(msg) }) : `${label}: ${engineText(msg)}`);
      return null;
    }
  }, [editable, afterEdit, flash, current, go]);

  // File ▸ Import PDF Comments…: pick a project PDF, store its comments in the document, open the PDF Comments panel
  const pdfImport = usePdfCommentsImport({ state: pdfComments, editable, projectRoot: project?.info.root, apply: afterEdit, flash, showPanel: () => dock.show('pdfComments', false) });
  // The engine's preview session for the number fields (ui/controls): a value tried in a field is shown provisionally in the
  // document and taken back exactly, outside the undo history. Requests are kept in order and coalesced there.
  const previewRefs = useRef({ afterEdit, flash, editable });
  previewRefs.current = { afterEdit, flash, editable };
  const previewSession = useMemo(() => new PreviewSession(async (label, commands) => {
    if (!previewRefs.current.editable) return null;
    try {
      const r = await editDocument(label, commands as unknown as EditCommand[]);
      previewRefs.current.afterEdit(r);
      return r;
    } catch (e) {
      previewRefs.current.flash(`${label}: ${String((e as Error).message ?? e)}`);
      return null;
    }
  }), []);
  previewSessionRef.current = previewSession;
  useEffect(() => () => previewSession.discard(), [doc?.path, previewSession]);

  return {
    runEdit, afterEdit, pdfImport,
    selectionRef, previewSession, previewScope,
  };
}
