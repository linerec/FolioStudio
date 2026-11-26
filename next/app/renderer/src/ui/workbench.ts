// Where the window puts the project explorer and the tools (user decision 2026-11-20, CLAUDE.md "화면도 프로젝트 중심이다"):
// `folio` (the default) is a VS Code style sidebar on the left (the explorer above, the tools and the fill and stroke below);
// `classic` is InDesign's layout (the tools down the left edge, the project panel in the dock). The engine keeps it in the user
// settings (`workspaceLayout` / `setWorkspaceLayout`, server/src/workspace_layout.rs), so a tool (MCP) changes it with the same
// command and the window follows the `user-settings` event. Nothing of it is in a project folder.
import { useCallback, useEffect, useRef, useState } from 'react';

export type LayoutKind = 'folio' | 'classic';
export type SidebarState = { open: boolean; width: number; split: number; toolsOpen: boolean; explorerOpen: boolean };
export type WorkbenchState = { layout: LayoutKind; sidebar: SidebarState };
export type WorkbenchPatch = { layout?: LayoutKind; sidebar?: Partial<SidebarState> };

export const WORKBENCH_DEFAULT: WorkbenchState = { layout: 'folio', sidebar: { open: true, width: 264, split: 0.64, toolsOpen: true, explorerOpen: true } };
/** The engine's bounds (workspace_layout.rs): the screen keeps a drag inside them too. */
export const SIDEBAR_WIDTH = { min: 180, max: 640 } as const;
export const SIDEBAR_SPLIT = { min: 0.15, max: 0.85 } as const;

/** A value the engine answered, read defensively (an older engine answers nothing: the default stays). */
function clean(v: unknown): WorkbenchState | null {
  if (!v || typeof v !== 'object') return null;
  const o = v as { layout?: unknown; sidebar?: Partial<Record<keyof SidebarState, unknown>> };
  const d = WORKBENCH_DEFAULT.sidebar, s = o.sidebar ?? {};
  const num = (x: unknown, fallback: number) => (typeof x === 'number' && Number.isFinite(x) ? x : fallback);
  const bool = (x: unknown, fallback: boolean) => (typeof x === 'boolean' ? x : fallback);
  return {
    layout: o.layout === 'classic' ? 'classic' : 'folio',
    sidebar: { open: bool(s.open, d.open), width: num(s.width, d.width), split: num(s.split, d.split), toolsOpen: bool(s.toolsOpen, d.toolsOpen), explorerOpen: bool(s.explorerOpen, d.explorerOpen) },
  };
}

export function useWorkbench() {
  const [state, setState] = useState<WorkbenchState>(WORKBENCH_DEFAULT);
  const origin = useRef(`workbench-${Math.random()}`);
  const pending = useRef<WorkbenchPatch>({});
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => {
    let gone = false;
    const accept = (v: unknown) => { const w = clean(v); if (w && !gone) setState(w); };
    const off = window.folio.onEvent((e) => {
      if (e.event !== 'user-settings') return;
      const d = e.detail as { origin?: unknown; settings?: { workspaceLayout?: unknown } };
      if (d.origin === origin.current || !d.settings?.workspaceLayout) return;
      accept(d.settings.workspaceLayout);
    });
    void window.folio.request({ cmd: 'workspaceLayout' }).then(({ header }) => { if (header.ok) accept(header.workspaceLayout); }, () => {});
    return () => { gone = true; off(); window.clearTimeout(timer.current); };
  }, []);
  /** Changes the layout now; `later` waits for a drag to rest before the engine writes it (width, split). */
  const set = useCallback((patch: WorkbenchPatch, later = false) => {
    setState((s) => ({ layout: patch.layout ?? s.layout, sidebar: { ...s.sidebar, ...patch.sidebar } }));
    const p = pending.current;
    pending.current = { ...p, ...(patch.layout ? { layout: patch.layout } : {}), ...(patch.sidebar || p.sidebar ? { sidebar: { ...p.sidebar, ...patch.sidebar } } : {}) };
    window.clearTimeout(timer.current);
    const flush = () => {
      const send = pending.current;
      pending.current = {};
      if (!send.layout && !send.sidebar) return;
      void window.folio.request({ cmd: 'setWorkspaceLayout', patch: send, origin: origin.current }).catch(() => {});
    };
    if (later) timer.current = window.setTimeout(flush, 250); else flush();
  }, []);
  return { ...state, set };
}

export type Workbench = ReturnType<typeof useWorkbench>;
