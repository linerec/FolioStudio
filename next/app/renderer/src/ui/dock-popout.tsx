//! Renders floating panel groups into their own windows and tracks their lifecycle.

import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { placeGhost } from './dock-drop-target';
import { PANEL_META, type PanelId } from './dock-panels';
import type { Dock } from './dock-state';
import { forwardKeys, freeName, panelWindows } from './dockwindows';

/** The app window is closing: its panel windows close with it and keep their place for the next start (they do not dock back). */
let leaving = false;
window.addEventListener('beforeunload', () => { leaving = true; });

/**
 * A floating group in an OS window of its own (another monitor, uix-d2). The window is an empty page of this same renderer (Electron
 * gives it to the app window, main.cjs), and the group is drawn into it from here: the same React tree, engine session and document
 * state, nothing copied. It takes the app window's styles and theme. Closing it puts the group back where it was docked; where the
 * person moves or sizes it is kept in the layout (the next start opens it there, or inside the app window when that monitor is gone).
 * The window stays open while its group changes (uix-d3: tabs reordered, the first one dragged away, another window's tabs joining):
 * FloatingPanels keeps the same component for it as long as the group keeps one of its panels.
 */
export function PopoutWindow({ dock, group, panels, rect, children }: { dock: Dock; group: number; panels: PanelId[]; rect: { x: number; y: number; w: number; h: number }; children: React.ReactNode }) {
  const [host, setHost] = useState<HTMLElement | null>(null);
  const at = useRef(group);
  at.current = group;
  const lead = panels[0];
  const lead0 = useRef(lead);
  lead0.current = lead;
  const nameRef = useRef<string | null>(null);
  // the registry knows which group the window shows now (a drag finds the group by the window)
  useEffect(() => { const e = nameRef.current ? panelWindows.get(nameRef.current) : undefined; if (e) e.lead = lead; });
  const title = panels.map((id) => PANEL_META[id].title).join(' · ');
  const child = useRef<Window | null>(null);
  useEffect(() => {
    let gone = false;
    let stop = () => {};
    void (async () => {
      // the monitor it was on is gone (unplugged, another computer): inside the app window instead
      const screens = await window.folio.displays?.().catch(() => null);
      const cx = rect.x + rect.w / 2, cy = rect.y + Math.min(rect.h / 2, 40);
      if (gone) return;
      if (screens && screens.length && !screens.some((d) => cx >= d.x && cx < d.x + d.w && cy >= d.y && cy < d.y + d.h)) { dock.apply({ op: 'popIn', group: at.current }, { record: false }); return; }
      // a name of its own for as long as it is open (the group's first panel may change meanwhile: the window stays)
      const name = freeName(lead0.current);
      const w = window.open('', name, `popup=yes,left=${rect.x},top=${rect.y},width=${rect.w},height=${rect.h}`);
      if (!w) { dock.apply({ op: 'popIn', group: at.current }, { record: false }); return; }
      child.current = w;
      // a drag across the windows finds it by name (uix-d3), and its keys reach the app window's shortcuts
      nameRef.current = name;
      panelWindows.set(name, { win: w, lead: lead0.current });
      const keys = forwardKeys(w);
      const doc = w.document;
      doc.title = title;
      doc.head.replaceChildren(...[...document.querySelectorAll('style, link[rel="stylesheet"]')].map((n) => n.cloneNode(true)));
      // the theme, the language and the interface size follow the app window's
      const sync = () => { for (const a of ['data-theme', 'lang', 'class', 'style']) { const v = document.documentElement.getAttribute(a); if (v === null) doc.documentElement.removeAttribute(a); else doc.documentElement.setAttribute(a, v); } };
      sync();
      const watch = new MutationObserver(sync);
      watch.observe(document.documentElement, { attributes: true });
      doc.body.className = 'popout-body';
      const root = doc.createElement('div');
      root.className = 'popout';
      doc.body.replaceChildren(root);
      setHost(root);
      // the person closed the window: the group goes back where it was docked
      const closed = () => { if (gone || leaving) return; gone = true; dock.apply({ op: 'dockFloating', group: at.current }); };
      w.addEventListener('pagehide', closed);
      // where the person put it: looked at twice a second (there is no move event); the layout keeps it
      let last = JSON.stringify(rect);
      const timer = window.setInterval(() => {
        if (w.closed) { closed(); return; }
        const now = { x: w.screenX, y: w.screenY, w: w.outerWidth, h: w.outerHeight };
        const text = JSON.stringify(now);
        if (text !== last && now.w >= 120 && now.h >= 80) { last = text; dock.apply({ op: 'moveWindow', group: at.current, window: now }); }
      }, 500);
      stop = () => { window.clearInterval(timer); watch.disconnect(); keys(); if (panelWindows.get(name)?.win === w) panelWindows.delete(name); w.removeEventListener('pagehide', closed); w.close(); };
    })();
    return () => { gone = true; stop(); child.current = null; nameRef.current = null; setHost(null); };
  }, []);  // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (child.current) child.current.document.title = title; }, [title]);
  // a drag over this window (from it, or from the app window and its other panel windows): where the panel would land shows here
  const d = dock.drag && nameRef.current && dock.drag.win === nameRef.current ? dock.drag : null;
  return host ? createPortal(<>
    {children}
    {d?.preview && <div className="dock-preview" style={{ left: d.preview.x, top: d.preview.y, width: d.preview.w, height: d.preview.h }} />}
    {d?.ghost && <div className="dock-ghost" style={placeGhost({}, d.at.x, d.at.y)}>{d.label}</div>}
  </>, host) : null;
}
