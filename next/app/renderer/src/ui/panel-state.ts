import { useEffect, useRef, type Dispatch, type SetStateAction, type RefObject } from 'react';
import { CLASSIC, normalizeDock, type DockEnv } from './docklayout';
import type { DockState, PanelId } from './dock';

type Flyout = {group: number; active: PanelId} | null;
type State = {dock: DockState; hidden: boolean; flyout: Flyout};
type Context = {state: DockState; stateRef: RefObject<DockState>; setState: Dispatch<SetStateAction<DockState>>;
  hidden: boolean; setHidden: Dispatch<SetStateAction<boolean>>; flyout: Flyout; setFlyout: Dispatch<SetStateAction<Flyout>>};

/** Synchronizes the existing dock owner with the engine's user settings, including open flyouts. */
export function usePanelState(c: Context, env: DockEnv, legacy: () => unknown, fix: (dock: DockState, seed?: unknown) => DockState = (d) => d) {
  const current = useRef(c); current.current = c;
  const loaded = useRef(false), saved = useRef(''), timer = useRef<number | undefined>(undefined);
  const origin = useRef(`dock-${Math.random()}`);
  const received = useRef('');
  useEffect(() => {
    let gone = false;
    // `seed`: what an older layout needs to grow into the current one (the sidebar's settings, for the Folio views); a layout that
    // had to grow is written back once
    const accept = (state: State, migrate = false, seed?: unknown) => {
      if (gone) return;
      window.clearTimeout(timer.current);
      const plain = normalizeDock(state.dock, env) as DockState;
      const grown = fix(plain, seed);
      if (grown !== plain) migrate = true;
      const next = {...state, dock: grown};
      received.current = JSON.stringify(state);
      saved.current = migrate ? '' : JSON.stringify(next);
      loaded.current = true;
      current.current.stateRef.current = next.dock;
      current.current.setState(next.dock); current.current.setHidden(next.hidden); current.current.setFlyout(next.flyout);
    };
    const off = window.folio.onEvent(e => {
      if (e.event !== 'user-settings') return;
      const detail = e.detail as {origin?: string; settings: {dock?: DockState; panelVisibility?: {hidden: boolean; flyout: Flyout}}};
      if (!detail.settings.dock) return;
      const v = detail.settings.panelVisibility;
      const next = {dock: detail.settings.dock, hidden: v?.hidden ?? false, flyout: v?.flyout ?? null};
      const text = JSON.stringify(next);
      if (detail.origin === origin.current) { received.current = text; return; }
      if (received.current !== text) accept(next);
    });
    void window.folio.settings().then(async settings => {
      if (gone || loaded.current) return;
      const seed = (settings as {workspaceLayout?: {sidebar?: unknown}}).workspaceLayout?.sidebar;
      if (!settings.dock) {
        const old = legacy();
        if (old) { accept({dock: normalizeDock(old, env) as DockState, hidden: false, flyout: null}, true, seed); return; }
      }
      const {header} = await window.folio.request({cmd:'panelState'});
      if (header.ok && !loaded.current) accept(header.panelState as State, false, seed);
    }).catch(() => accept({dock: CLASSIC as DockState, hidden: false, flyout: null}));
    return () => { gone = true; off(); window.clearTimeout(timer.current); };
  }, []);
  useEffect(() => {
    if (!loaded.current) return;
    const patch: State = {dock:c.state, hidden:c.hidden, flyout:c.flyout};
    const text = JSON.stringify(patch);
    if (text === saved.current) return;
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => {
      saved.current = text;
      void window.folio.request({cmd:'setPanelState', patch, origin:origin.current}).then(({header}) => {
        if (!header.ok) saved.current = '';
        else localStorage.removeItem('folio.workspace.classic.v1');
      }, () => { saved.current = ''; });
    }, 100);
    return () => window.clearTimeout(timer.current);
  }, [c.state, c.hidden, c.flyout]);
}
