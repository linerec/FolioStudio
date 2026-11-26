import { conveyorActions,conveyorKey,loadedCursor } from '../ui/conveyor';
import { t as tr } from '../i18n';
import { useCallback,useEffect,useMemo,useRef } from 'react';
import {
type EditCommand
} from '../engine';
import { commandFor,matches,typing } from '../ui/commands';
import { type PanelId } from '../ui/dock';
import { useStyleShortcuts } from '../ui/styleshortcuts';
import { commandOwner } from '../ui/commandowner';
import { buildMenus } from '../ui/menudef';
import { type SelEntry,findItem } from '../ui/overlay';
import { pointSelection } from '../ui/store';
import { ALL_TOOLS,type ToolId } from '../ui/tools';
import { ownDelta } from '../ui/turned';
import type { AppContext } from './context';
import type { Commands } from '../ui/commands';
import type { RefObject } from 'react';

export function useAppEvents(context: AppContext, commandsRef: RefObject<Commands>) {
  const {
    recentProjects, presets, workspaces,
    language, setTool, textFormatting,
    editable, selection, runEdit,
    store, info, setTempTool,
    presenting, setPresenting, tool,
    dock, setAllHidden, setStrokeFocus,
    toolboxPaintRef, setFormatText, flash,
    selectionRef, vp, open,
    openDoc, loadProject, setImportSource,
    save, setIdmlExport, placement,
    setSelection, appWins, editStateRef,
    externalOpens, current, status,
    doc, project, importSource,
    textSelection, setTextReveal, setSelectedPage,
    setMasterView, showHidden,
  } = context;
  const menus = useMemo(() => buildMenus(recentProjects.map((_, i) => ({ cmd: `recent.${i}` })), presets.map((_, i) => ({ cmd: `docpreset.${i}` })), workspaces.entries), [recentProjects, presets, language, workspaces.entries]);

  /** Picks a tool. */
  const pickTool = useCallback((t: ToolId) => {
    setTool(t);
  }, []);
  useStyleShortcuts(textFormatting, commandsRef, { editable, selected: selection.filter((s) => !s.content).length, edit: runEdit,
    frames: selection.filter((s) => !s.content && !!findItem(store.peek(s.page), s.id)?.item?.text?.story).map((s) => s.id) });
  const editableRef = useRef(editable);
  editableRef.current = editable;
  // the document's cursor key increment (Preferences > Units & Increments): the arrow keys move by it, with Shift by ten times
  const cursorStepRef = useRef(1);
  cursorStepRef.current = info?.view?.cursorKeyIncrement ?? 1;
  const runEditRef = useRef(runEdit);
  runEditRef.current = runEdit;
  const docRef = useRef(doc);
  docRef.current = doc;
  useEffect(() => {
    const onDown = (e: KeyboardEvent) => {
      if (document.querySelector('.dropdown')) return;
      if (document.querySelector('.overlay') && e.key !== 'Escape') return;
      if (typing(e)) return;
      const plain = !e.ctrlKey && !e.metaKey && !e.altKey;
      // temporary tools: Space = hand, Ctrl+Space = zoom (Alt for out)
      if (e.code === 'Space') {
        e.preventDefault();
        if (!e.repeat) setTempTool(e.ctrlKey || e.metaKey ? 'zoom' : 'hand');
        return;
      }
      if (e.key === 'Escape') {
        if (presenting) { setPresenting(false); return; }
        // the cursor loaded by Edit ▸ Place and Link is emptied first
        if (loadedCursor.get()) { void conveyorActions.unload().catch(() => undefined); return; }
        if (tool !== 'selection' && tool !== 'direct') { setTool('selection'); return; }
        if (selection.some((s) => s.content) || selection.length) { commandsRef.current['object.selectContainer']?.run?.(); return; }
        return;
      }
      if (plain && e.code === 'Tab') { e.preventDefault(); if (e.shiftKey) dock.setHidden(!dock.hidden); else setAllHidden((v) => !v); return; }
      if (plain && !e.shiftKey && e.code === 'KeyX') { setStrokeFocus((v) => !v); return; }
      if (plain && e.shiftKey && e.code === 'KeyX') { toolboxPaintRef.current.swap?.(); return; }
      if (plain && !e.shiftKey && e.code === 'KeyD') { toolboxPaintRef.current.defaults?.(); return; }
      if (plain && !e.shiftKey && e.code === 'Comma') { toolboxPaintRef.current.applyColor?.(); return; }
      if (plain && !e.shiftKey && e.code === 'Period') { toolboxPaintRef.current.applyGradient?.(); return; }
      if (plain && !e.shiftKey && e.code === 'Slash') { toolboxPaintRef.current.applyNone?.(); return; }
      if (plain && !e.shiftKey && e.code === 'KeyJ') { setFormatText((v) => !v); return; }
      const id = commandFor(e, commandsRef.current);
      if (id) {
        const c = commandsRef.current[id];
        e.preventDefault();
        if (c.run && (c.enabled ?? true)) c.run();
        else if (c.why) flash(`${c.label}: ${c.why}`);
        return;
      }
      // the Content Placer: the arrow keys choose the conveyor item placed next (ui/conveyor.ts)
      if (conveyorKey(e, tool)) return;
      // arrows nudge the selection by the cursor key increment (1 pt), Shift ten times that
      // Alt: a copy moves instead; with placed content selected, the content moves in its frame
      // Ctrl+Shift+arrow (⌘ on a Mac) nudges a tenth of the increment (Shift+Alt a copy ten times, Ctrl+Shift+Alt a copy a tenth: InDesign's Nudge 1/10)
      const tenth = (e.ctrlKey || e.metaKey) && e.shiftKey;
      if ((tenth || (!e.ctrlKey && !e.metaKey)) && e.key.startsWith('Arrow') && editableRef.current && selectionRef.current.length && !pointSelection.get()) {
        e.preventDefault();
        const step = cursorStepRef.current * (tenth ? 0.1 : e.shiftKey ? 10 : 1);
        const sel = selectionRef.current;
        // on a spread turned by View ▸ Rotate Spread the selection goes the way the arrow points on the screen
        const pg = docRef.current?.pages.find((q) => q.index === sel[0]?.page);
        const by = ownDelta(pg ? docRef.current?.pageSettings?.rotation?.[pg.spread] : undefined,
          e.key === 'ArrowLeft' ? [-step, 0] : e.key === 'ArrowRight' ? [step, 0] : e.key === 'ArrowUp' ? [0, -step] : [0, step]);
        if (sel.length === 1 && sel[0].content) runEditRef.current(tr('내용 이동'), [{ op: 'moveContent', id: sel[0].id, by }]);
        else if (e.altKey) runEditRef.current(tr('복제'), [{ op: 'duplicate', ids: sel.filter((s) => !s.content).map((s) => s.id), by }]);
        else runEditRef.current(tr('이동'), [{ op: 'move', ids: sel.filter((s) => !s.content).map((s) => s.id), by }]);
        return;
      }
      if (plain && (e.code === 'PageDown' || e.code === 'PageUp') && !e.shiftKey) {
        e.preventDefault();
        const v = vp.viewRef.current;
        // a key: the view jumps at once (no smooth scroll on keyboard work)
        if (v) v.scrollBy({ top: (e.code === 'PageDown' ? 1 : -1) * v.clientHeight * 0.9 });
        return;
      }
      if (plain || (!e.ctrlKey && !e.metaKey && !e.altKey && e.shiftKey)) {
        const t = ALL_TOOLS.find((x) => x.key && matches(e, x.key));
        // B switches between the Content Collector and the Content Placer (InDesign: Toggle Content Collector and Content Placer)
        if (t) { e.preventDefault(); pickTool(t.id === 'collector' && tool === 'collector' ? 'placer' : t.id); }
      }
    };
    const onUp = (e: KeyboardEvent) => {
      if (e.code === 'Space') setTempTool(null);
    };
    const onBlur = () => setTempTool(null);
    window.addEventListener('keydown', onDown);
    window.addEventListener('keyup', onUp);
    window.addEventListener('blur', onBlur);
    return () => { window.removeEventListener('keydown', onDown); window.removeEventListener('keyup', onUp); window.removeEventListener('blur', onBlur); };
  }, [tool, selection, dock, vp, presenting, pickTool, flash]);

  // test hooks (FOLIO_TEST_STEPS): drive the flows without native dialogs
  useEffect(() => {
    (window as unknown as { __folioTest: unknown }).__folioTest = {
      open,
      openReadOnly: openDoc,
      openProject: loadProject,
      import: (p: string) => setImportSource(p),
      save,
      exportIdml: () => setIdmlExport(true),
      run: (id: string) => commandsRef.current[id]?.run?.(),
      /** Whether a command is offered now (a menu item not greyed). */
      enabled: (id: string) => !!commandsRef.current[id]?.run && (commandsRef.current[id]?.enabled ?? true),
      /** Whether a menu item shows its check mark. */
      checked: (id: string) => !!commandsRef.current[id]?.checked,
      /** The text a menu item shows now (Show/Hide switch with the setting). */
      label: (id: string) => commandsRef.current[id]?.label,
      /** The menu command that uses a key in the spelling of style shortcuts (`ctrl+alt+z`), named as the Style Options field names it. */
      styleShortcutOwner: (spec: string) => commandOwner(spec, commandsRef.current)?.name ?? null,
      tool: (t: ToolId) => setTool(t),
      place: placement.begin,
      placing: () => placement.files,
      select: (s: SelEntry[]) => setSelection(s),
      panel: (id: PanelId) => dock.show(id, false),
      /** File > Open in New Window without the dialog: a new app window that opens `path`. */
      newWindow: (path?: string) => window.folio.newWindow(path),
      /** Window > Arrange across documents (electron/appwin.cjs): a command or query with a structured answer, and the strip this window shows. */
      appWindows: (cmd: Parameters<typeof appWins.apply>[0]) => appWins.apply(cmd),
      appWinView: () => appWins.view,
      /** Panel dock commands (ui/docklayout.ts DockCommand) with a structured result, and the layout as one row per panel. */
      dock: (cmd: Parameters<typeof dock.apply>[0]) => dock.apply(cmd),
      dockLayout: () => dock.layout(),
      items: (page: number) => store.peek(page),
      theme: () => getComputedStyle(document.documentElement).getPropertyValue('--panel').trim(),
      edit: (label: string, commands: EditCommand[]) => runEditRef.current(label, commands),
      editState: () => ({ editable: editableRef.current, ...editStateRef.current }),
      /** The addresses Go To Destination handed to the system browser (a test run records them and never starts a browser). */
      openedUrls: externalOpens.current,
      /** The document view: zoom (screen pixels per point; 1 = actual size) and the zoom setting. */
      view: () => ({ scale: vp.scale, zoom: vp.zoom, current }),
      state: () => ({ status, showHidden, doc: doc && { path: doc.path, folio: doc.folio }, project: project?.info ?? null, importing: importSource, selection, tool, info: !!info, current, text: textSelection && { story: textSelection.story, start: textSelection.start, end: textSelection.end, format: textSelection.format },
        /** The selected table cells ([column, row]), as the Table menu sees them. */
        table: context.tableSel && { table: context.tableSel.table, from: context.tableSel.from, to: context.tableSel.to, cursor: context.tableSel.cursor ?? null } }),
      revealText: (story: string, start: number, end: number) => setTextReveal({ story, start, end }),
      /** Chooses a page as the Page tool does (the control bar then shows its size and place); null: no page. */
      selectPage: (index: number | null) => setSelectedPage(index),
      /** Shows a parent (its `MasterSpread` id) in the document window as a double click in the Pages panel does; null: back to the pages. */
      openMaster: (id: string | null) => { setSelection([]); setSelectedPage(null); setMasterView(id); },
    };
  }, [open, openDoc, loadProject, save, status, doc, project, importSource, selection, tool, info, current, textSelection, dock, menus, placement.begin, placement.files, vp.scale, vp.zoom, context.tableSel]);

  return {
    menus, pickTool,
  };
}
