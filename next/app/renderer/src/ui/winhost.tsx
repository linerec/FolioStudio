// The document area of the window: the windows of the document (windows.ts) as tabs, as cells of an N-up arrangement, or floating, each
// with a viewport of its own, all showing the one document the app holds. The App gives the document window it would draw for one
// window (`pane`); every window and every half of a split window is that same element with a viewport, a current page and tabs of
// its own.
import { type ReactElement, cloneElement, useCallback, useEffect, useRef, useState } from 'react';
import type { PageInfo } from '../engine';
import { type DocWindowProps, type WindowTabs, fileName, useViewport } from './docview';
import { type Windows, type Win, activate, closeWindow, docked, moveFloat, paneKey, shownId } from './windows';
import { FLOAT_MIN } from './winlayout';

const noop = () => {};

/** One document window with its own viewport and current page. */
function Pane({ ws, pane, rows, win, second, tabs, winNo }: { ws: Windows; pane: ReactElement<DocWindowProps>; rows: PageInfo[][]; win: Win; second: boolean; tabs: WindowTabs | null | undefined; winNo?: number }) {
  const key = paneKey(win.id, second);
  const vp = useViewport(rows);
  const { registry, touch } = ws;
  registry.current.set(key, vp);
  useEffect(() => () => { registry.current.delete(key); }, [key, registry]);
  const p = pane.props;
  const isActive = ws.state.active === win.id && ws.activeKey === key;
  const activeRef = useRef(isActive);
  activeRef.current = isActive;
  const [current, setLocal] = useState(p.current);
  // the app's current page (the control bar, the commands) is the active pane's
  const appSet = p.setCurrent;
  const setCurrent = useCallback((i: number) => { setLocal(i); if (activeRef.current) appSet(i); }, [appSet]);
  useEffect(() => { if (isActive) appSet(current); }, [isActive]); // eslint-disable-line react-hooks/exhaustive-deps
  // the app draws again when a pane's zoom changes, so what reads the active viewport (zoom steps, the control bar) sees it
  useEffect(() => { if (isActive) touch(); }, [vp.scale, isActive, touch]);
  const many = ws.state.wins.length > 1;
  return cloneElement(pane, {
    vp, current, setCurrent, tabs, winNo,
    // text editing and table actions of the app belong to the active pane
    textActions: isActive ? p.textActions : { current: null },
    tableActions: isActive ? p.tableActions : undefined,
    onTableKey: isActive ? p.onTableKey : undefined,
    onPathKey: isActive ? p.onPathKey : undefined,
    pathFinish: isActive ? p.pathFinish : undefined,
    onTextState: isActive ? p.onTextState : noop,
    onTableState: isActive ? p.onTableState : undefined,
    onClose: many ? () => ws.update((s) => closeWindow(s, win.id)) : p.onClose,
    // the notes of the status line (saved, missing fonts) are shown in the active window
    statusExtra: many && !isActive ? undefined : p.statusExtra,
  });
}

/** A window: one pane, or two side by side when it is split. */
function WinPanes({ ws, pane, rows, win, tabs, winNo }: { ws: Windows; pane: ReactElement<DocWindowProps>; rows: PageInfo[][]; win: Win; tabs: WindowTabs | undefined; winNo?: number }) {
  return (
    <>
      <div className="win-pane" onPointerDownCapture={() => ws.update((s) => activate(s, win.id, paneKey(win.id)))}>
        <Pane ws={ws} pane={pane} rows={rows} win={win} second={false} tabs={tabs} winNo={winNo} />
      </div>
      {win.split && (
        <div className="win-pane second" onPointerDownCapture={() => ws.update((s) => activate(s, win.id, paneKey(win.id, true)))}>
          <Pane ws={ws} pane={pane} rows={rows} win={win} second tabs={null} winNo={winNo} />
        </div>
      )}
    </>
  );
}

export function WindowHost({ ws, pane, rows }: { ws: Windows; pane: ReactElement<DocWindowProps>; rows: PageInfo[][] }) {
  const s = ws.state;
  const dock = docked(s);
  const shown = shownId(s);
  const many = s.wins.length > 1;
  const name = fileName(pane.props.doc.path);
  const items = dock.map((w) => ({ id: w.id, name: `${name}:${w.id}`, dirty: pane.props.dirty }));
  const tabsOf = (w: Win): WindowTabs | undefined => {
    if (!many) return undefined;
    const own = { id: w.id, name: `${name}:${w.id}`, dirty: pane.props.dirty };
    // as tabs the strip lists the docked windows; in a grid and floating a window shows only its own
    return { items: !w.float && !ws.rects ? items : [own], select: (id) => ws.update((x) => activate(x, id)), close: (id) => ws.update((x) => closeWindow(x, id)) };
  };

  // a floating window is moved by its title strip and resized by its corner; both keep it inside the area
  const drag = useRef<{ id: number; dx: number; dy: number } | null>(null);
  useEffect(() => {
    const move = (e: PointerEvent) => {
      const d = drag.current;
      const host = ws.hostRef.current;
      if (!d || !host) return;
      const b = host.getBoundingClientRect();
      ws.update((x) => {
        const w = x.wins.find((q) => q.id === d.id);
        return w?.float ? moveFloat(x, d.id, { ...w.float, x: e.clientX - b.left - d.dx, y: e.clientY - b.top - d.dy }, { w: b.width, h: b.height }) : x;
      });
    };
    const up = () => { drag.current = null; };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', up);
    return () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', up); };
  }, [ws]);

  // One list in the order the windows were made, so that a window keeps its place in the tree (and its zoom, scroll and text editing)
  // when it floats, docks or joins an arrangement; only its style changes.
  return (
    <div className="win-area" ref={ws.hostRef}>
      {s.wins.map((w) => {
        const f = w.float;
        const i = dock.indexOf(w);
        const r = ws.rects?.[i] ?? { x: 0, y: 0, w: 1, h: 1 };
        const visible = f ? true : !!ws.rects || w.id === shown;
        const cls = `win-cell${f ? ' floating-win' : ws.rects ? ' grid' : ''}${visible ? '' : ' hidden'}${many && s.active === w.id ? ' active' : ''}`;
        return (
          <div
            key={w.id}
            className={cls}
            data-win={w.id}
            style={f
              ? { left: f.x, top: f.y, width: f.w, height: f.h, minWidth: FLOAT_MIN.w, minHeight: FLOAT_MIN.h }
              : { left: `${r.x * 100}%`, top: `${r.y * 100}%`, width: `${r.w * 100}%`, height: `${r.h * 100}%` }}
            onPointerDown={f ? (e) => {
              const t = e.target as HTMLElement;
              if (!t.closest('.doc-tabs') || t.closest('button')) return;
              const b = e.currentTarget.getBoundingClientRect();
              drag.current = { id: w.id, dx: e.clientX - b.left, dy: e.clientY - b.top };
            } : undefined}
            onPointerUp={f ? (e) => {
              // remember a size changed with the resize corner
              const el = e.currentTarget;
              if (Math.abs(el.offsetWidth - f.w) > 2 || Math.abs(el.offsetHeight - f.h) > 2) {
                const host = ws.hostRef.current?.getBoundingClientRect();
                ws.update((x) => moveFloat(x, w.id, { ...f, w: el.offsetWidth, h: el.offsetHeight }, { w: host?.width ?? 900, h: host?.height ?? 600 }));
              }
            } : undefined}
          >
            <WinPanes ws={ws} pane={pane} rows={rows} win={w} tabs={tabsOf(w)} winNo={many ? w.id : undefined} />
          </div>
        );
      })}
    </div>
  );
}
