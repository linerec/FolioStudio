import { t as tr } from '../i18n';
// Window > Workspace: named panel layouts (the dock: which panels are where, open or folded, split or floating; and whether the Tools
// and Control panels show). New Workspace… saves the layout as it is now under a name; picking a name brings it back; Rename Workspace…
// renames the one in use; Delete Workspace… removes a saved one. A layout belongs to this computer, never to a project folder: the
// saved workspaces are a user setting (`workspaces` in settings.json, written with the engine's `userSettings` command, so a tool
// can read and write them too; uix-d1 moved them there from the window's own storage), and a project moved to another computer
// carries no window layout.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import type { Command } from './commands';
import { type Dock, type DockState } from './dock';
import type { MenuEntry } from './menudef';
import { NameDialog } from './tablestylegroups';
import type { LayoutKind, Workbench } from './workbench';

/** The window's own storage of older versions: read once, moved into the user settings, then removed. */
const LEGACY_KEY = 'folio.workspaces.v1';

/** `layout`: where the explorer and the tools go (ui/workbench.ts); a workspace saved before there was a choice has none and keeps the one in use. */
export type Workspace = { name: string; dock: DockState; tools: boolean; control: boolean; layout?: LayoutKind };
type Stored = { list: Workspace[]; current: string | null };

/** The built-in workspaces: Folio's (the default: the sidebar) and InDesign's Essentials Classic (its panels are dock.tsx CLASSIC). Neither can be
 *  saved over or deleted. The command of the classic one keeps InDesign's id (`window.workspaceClassic`). */
export const BUILT_IN = (layout: LayoutKind = 'folio') => (layout === 'classic' ? tr('InDesign 클래식') : tr('Folio (기본)'));

/** Saved workspaces out of anything (the settings file can be edited by hand or by a tool). */
function clean(s: unknown): Stored | null {
  if (!s || typeof s !== 'object' || !Array.isArray((s as Stored).list)) return null;
  const raw = s as { list: unknown[]; current?: unknown };
  const list = raw.list.filter((w): w is Workspace => !!w && typeof w === 'object' && typeof (w as Workspace).name === 'string' && !!(w as Workspace).dock
    && Array.isArray((w as Workspace).dock.strip) && Array.isArray((w as Workspace).dock.column));
  return { list, current: typeof raw.current === 'string' && list.some((w) => w.name === raw.current) ? raw.current : null };
}

function legacy(): Stored | null {
  try { return clean(JSON.parse(localStorage.getItem(LEGACY_KEY) ?? 'null')); } catch { return null; }
}

const copy = <T,>(v: T): T => JSON.parse(JSON.stringify(v));

type Parts = {
  dock: Dock;
  tools: boolean;
  setTools: (v: boolean) => void;
  control: boolean;
  setControl: (v: boolean) => void;
  flash: (text: string) => void;
  workbench: Workbench;
};

export function useWorkspaces({ dock, tools, setTools, control, setControl, flash, workbench }: Parts) {
  const [store, setStore] = useState<Stored>({ list: [], current: null });
  const storeRef = useRef(store);
  const [dialog, setDialog] = useState<'new' | 'delete' | 'rename' | null>(null);
  const { list, current } = store;
  // the saved workspaces come from the user settings (and follow them when another window or a tool changes them)
  useEffect(() => {
    let gone = false;
    const take = (v: unknown) => { const s = clean(v); if (s && !gone) { storeRef.current = s; setStore(s); } };
    const off = window.folio.onEvent((e) => {
      if (e.event !== 'user-settings') return;
      const d = e.detail as { settings?: { workspaces?: unknown } };
      if (d.settings?.workspaces !== undefined) take(d.settings.workspaces);
    });
    void window.folio.settings().then((s) => {
      const saved = (s as { workspaces?: unknown }).workspaces;
      if (saved !== undefined && saved !== null) { take(saved); return; }
      const old = legacy();
      if (old) { take(old); void window.folio.saveSettings({ workspaces: old }).then(() => { try { localStorage.removeItem(LEGACY_KEY); } catch { /* kept */ } }, () => {}); }
    }, () => {});
    return () => { gone = true; off(); };
  }, []);
  const update = useCallback((next: Stored) => {
    storeRef.current = next;
    setStore(next);
    void window.folio.saveSettings({ workspaces: next }).catch(() => flash(tr('작업 영역을 저장하지 못했습니다')));
  }, [flash]);

  const layout = workbench.layout;
  // The layout's own undo (dock.tsx, uix-d3) keeps with each layout what a workspace change also changes: the workspace in use, its
  // tools and control panel and its arrangement (folio, classic). Undo brings them back with the panels: a workspace change is one step.
  const mine = useRef({ current, tools, control, layout });
  mine.current = { current, tools, control, layout };
  useEffect(() => {
    dock.companion.current = {
      capture: () => ({ ...mine.current }),
      restore: (v) => {
        const b = v as typeof mine.current;
        const now = mine.current;
        if (b.tools !== now.tools) setTools(b.tools);
        if (b.control !== now.control) setControl(b.control);
        if (b.layout !== now.layout) workbench.set({ layout: b.layout });
        const s = storeRef.current;
        const named = b.current !== null && s.list.some((w) => w.name === b.current) ? b.current : null;
        if (named !== s.current) update({ list: s.list, current: named });
      },
    };
    return () => { dock.companion.current = null; };
  });  // eslint-disable-line react-hooks/exhaustive-deps

  // `built`: the built-in layout to use (null: a saved workspace by `name`). Going from one built-in to the other keeps the panels as they
  // are (only the explorer and the tools move); coming from a saved workspace, and Reset, give the built-in panels back. One step of the
  // layout's undo.
  const apply = useCallback((name: string | null, built: LayoutKind | null = null, reset = false) => {
    const w = name === null ? null : list.find((x) => x.name === name);
    if (name !== null && !w) return;
    dock.step(() => applyNow(w ?? null, name, built, reset));
  }, [list, current, layout, dock, setTools, setControl, update, workbench.set]);  // eslint-disable-line react-hooks/exhaustive-deps
  const applyNow = (w: Workspace | null, name: string | null, built: LayoutKind | null, reset: boolean) => {
    if (w) {
      dock.load(copy(w.dock));
      dock.setFlyout(null);
      dock.setHidden(false);
      setTools(w.tools);
      setControl(w.control);
      if (w.layout) workbench.set({ layout: w.layout });
    } else {
      if (reset || current !== null) {
        dock.reset();
        setTools(true);
        setControl(true);
      }
      workbench.set({ layout: built ?? layout });
    }
    update({ list, current: name });
  };

  const save = useCallback((name: string) => {
    update({ list: [...list, { name, dock: copy(dock.state), tools, control, layout }], current: name });
    flash(tr('작업 영역 "{0}"을(를) 저장했습니다', { 0: name }));
  }, [list, dock.state, tools, control, layout, update, flash]);

  const rename = useCallback((name: string) => {
    if (current === null) return;
    update({ list: list.map((w) => (w.name === current ? { ...w, name } : w)), current: name });
    flash(tr('작업 영역 이름을 "{0}"(으)로 바꿨습니다', { 0: name }));
  }, [list, current, update, flash]);

  const remove = useCallback((name: string) => {
    update({ list: list.filter((w) => w.name !== name), current: current === name ? null : current });
    flash(tr('작업 영역 "{0}"을(를) 삭제했습니다', { 0: name }));
  }, [list, current, update, flash]);

  // a tool's dockAction on the workspaces (dock.tsx `act`, the engine's dockAction): save the layout under a name, load a saved one
  // (or a built-in one: folio, classic), reset the one in use; the same as Window ▸ Workspace
  useEffect(() => {
    dock.extra.current = (a) => {
      if (a.op === 'saveWorkspace') {
        const name = typeof a.name === 'string' ? a.name.trim() : '';
        if (!name) return { ok: false, error: 'invalid', message: 'name is required' };
        if (taken(name)) return { ok: false, error: 'invalid', message: `the workspace name is taken: ${name}` };
        save(name);
        return { ok: true, changed: true, layout: dock.layout() };
      }
      if (a.op === 'loadWorkspace') {
        if (a.name === 'folio' || a.name === 'classic') { apply(null, a.name); return { ok: true, changed: true, layout: dock.layout() }; }
        if (typeof a.name !== 'string' || !list.some((w) => w.name === a.name)) return { ok: false, error: 'notFound', message: `no saved workspace ${String(a.name)} (saved: ${list.map((w) => w.name).join(', ') || 'none'}; built-in: folio, classic)` };
        apply(a.name);
        return { ok: true, changed: true, layout: dock.layout() };
      }
      if (a.op === 'resetWorkspace') { apply(current, null, true); return { ok: true, changed: true, layout: dock.layout() }; }
      return null;
    };
    return () => { dock.extra.current = null; };
  });  // eslint-disable-line react-hooks/exhaustive-deps

  /** The saved workspaces that can be deleted: all but the one in use. */
  const deletable = useMemo(() => list.filter((w) => w.name !== current), [list, current]);

  const handlers = useMemo(() => {
    const h: Record<string, Partial<Command>> = {
      'window.workspaceFolio': { run: () => apply(null, 'folio'), checked: current === null && layout === 'folio' },
      'window.workspaceClassic': { label: BUILT_IN('classic'), run: () => apply(null, 'classic'), checked: current === null && layout === 'classic' },
      'window.resetWorkspace': { label: tr('{0} 재설정', { 0: current ?? BUILT_IN(layout) }), run: () => apply(current, null, true) },
      'id.window.workspace.newWorkspace': { run: () => setDialog('new') },
      'id.window.workspace.deleteWorkspace': { run: deletable.length ? () => setDialog('delete') : undefined },
      'window.renameWorkspace': { label: tr('작업 영역 이름 바꾸기…'), run: current !== null ? () => setDialog('rename') : undefined, why: tr('저장한 작업 영역을 쓰고 있을 때 이름을 바꿀 수 있습니다.') },
    };
    list.forEach((w, i) => { h[`workspace.user.${i}`] = { label: w.name, run: () => apply(w.name), checked: current === w.name }; });
    // the layout's own undo (uix-d3): apart from Edit ▸ Undo, which undoes document edits only
    h['window.undoLayout'] = { run: dock.steps.undo ? () => { dock.undo(); } : undefined, why: tr('되돌릴 배치 변경이 없습니다.') };
    h['window.redoLayout'] = { run: dock.steps.redo ? () => { dock.redo(); } : undefined, why: tr('다시 할 배치 변경이 없습니다.') };
    return h;
  }, [list, current, layout, deletable.length, apply, dock.steps, dock.undo, dock.redo]);

  /** The saved workspaces as menu items, for Window > Workspace and the switch button (and renaming the one in use). */
  const entries: MenuEntry[] = useMemo(() => [...list.map((_, i) => ({ cmd: `workspace.user.${i}` })), ...(current !== null ? [{ cmd: 'window.renameWorkspace' }] : [])], [list, current]);

  const taken = (n: string) => n === BUILT_IN('folio') || n === BUILT_IN('classic') || list.some((w) => w.name === n);
  const dialogs = dialog === 'new' ? (
    <NameDialog title={tr('새 작업 영역')} label={tr('이름:')} initial="" taken={taken} onOk={save} close={() => setDialog(null)} />
  ) : dialog === 'rename' && current !== null ? (
    <NameDialog title={tr('작업 영역 이름 바꾸기')} label={tr('이름:')} initial={current} taken={(n) => n !== current && taken(n)} onOk={rename} close={() => setDialog(null)} />
  ) : dialog === 'delete' && deletable.length ? (
    <DeleteDialog names={deletable.map((w) => w.name)} onOk={remove} close={() => setDialog(null)} />
  ) : null;

  return { current, label: current ?? BUILT_IN(layout), names: list.map((w) => w.name), entries, handlers, dialogs };
}

function DeleteDialog({ names, onOk, close }: { names: string[]; onOk: (name: string) => void; close: () => void }) {
  const [name, setName] = useState(names[0]);
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('작업 영역 삭제')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); onOk(name); close(); }}>
        <h2>{tr('작업 영역 삭제')}</h2>
        <label>{tr('작업 영역:')}
          <select aria-label={tr('작업 영역:')} autoFocus value={name} onChange={(e) => setName(e.target.value)}>
            {names.map((n) => <option key={n} value={n}>{n}</option>)}
          </select></label>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit">{tr('삭제')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
