// Tool help on the screen (uix-e1, 2026-11-21): the section under the tools that says what the chosen tool does (it folds), and the
// card that shows the moment the pointer (or the keyboard focus) is on a tool, a small toolbox button or the reference point.
// The words are one table (ui/toolhelp.ts, also next/shared/tool-help.json); the setting is the engine's user settings
// (`folio.instantToolHelp`, Preferences ▸ Interface and the Tools panel menu). Off: the system tooltips (`title`) as before.
import { createContext, useContext, useEffect, useId, useLayoutEffect, useRef, useState, type FocusEvent, type PointerEvent } from 'react';
import { createPortal } from 'react-dom';
import { tr } from '../i18n';
import { Icon } from './icons';
import { keyLabel } from './keys';
import type { HelpId } from './toolhelp';
import './toolhelp.css';

/** What the screen shows for one item: its name, the keys of this keymap, one line, and more. */
export type HelpText = { name: string; keys: string[]; summary: string; detail: string; note?: string };
export type ToolHelp = { instant: boolean; describe: (id: HelpId) => HelpText | null };
export const ToolHelpContext = createContext<ToolHelp>({ instant: false, describe: () => null });

type Side = 'right' | 'below';
type Open = { id: HelpId; el: HTMLElement; anchor: Element; side: Side };

/** The card's trigger props for the items of one surface (a toolbox, a flyout, the control bar's reference point) and the card itself.
 *  The items keep their `title` (the checks and the tooltip when the setting is off read it); while the card shows, the titles of the item
 *  and of its parents up to the surface are held back so the system tooltip does not show over the card. */
export function useHelpCard(side: Side) {
  const help = useContext(ToolHelpContext);
  const [open, setOpen] = useState<Open | null>(null);
  const cardId = useId();
  const held = useRef<{ el: Element; title: string }[]>([]);
  const release = () => {
    // a title React set again while it was held is the newer one
    for (const h of held.current) if (!h.el.hasAttribute('title')) h.el.setAttribute('title', h.title);
    held.current = [];
  };
  const show = (id: HelpId, el: HTMLElement, anchorSelector: string, at: Side) => {
    release();
    const anchor = el.closest(anchorSelector) ?? el;
    for (let n: Element | null = el; n; n = n.parentElement) {
      const title = n.getAttribute('title');
      if (title !== null) { held.current.push({ el: n, title }); n.removeAttribute('title'); }
      if (n === anchor) break;
    }
    setOpen({ id, el, anchor, side: at });
  };
  const leave = () => { release(); setOpen(null); };
  useEffect(() => {
    if (!open) return;
    const win = open.el.ownerDocument.defaultView ?? window;
    // Esc closes the card (the key goes on: it may also cancel what the person was doing)
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(null); };
    win.addEventListener('keydown', key, true);
    return () => win.removeEventListener('keydown', key, true);
  }, [open]);
  // the setting turned off while a card shows, or the surface goes away: nothing is left held
  useEffect(() => { if (!help.instant && open) leave(); }, [help.instant]);  // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => () => release(), []);
  const trigger = (id: HelpId, anchorSelector = '.toolbox', at: Side = side) => ({
    'data-help': id,
    'aria-describedby': open?.id === id ? cardId : undefined,
    onPointerEnter: (e: PointerEvent<HTMLElement>) => { if (help.instant && e.pointerType !== 'touch') show(id, e.currentTarget, anchorSelector, at); },
    onPointerLeave: () => { if (held.current.length || open) leave(); },
    // a press hides the card (the flyout opens where it was); the titles stay held until the pointer leaves
    onPointerDown: () => setOpen(null),
    onFocus: (e: FocusEvent<HTMLElement>) => { if (help.instant && e.target instanceof Element && e.target.matches(':focus-visible')) show(id, e.currentTarget, anchorSelector, at); },
    onBlur: () => leave(),
  });
  const text = open && help.instant ? help.describe(open.id) : null;
  const card = open && text ? createPortal(<HelpCard id={cardId} text={text} open={open} />, open.el.ownerDocument.body) : null;
  return { trigger, card };
}

/** The card: name and keys, one line, and (when there is one) the tool's note (ui/tools.tsx `toolNote`). Beside the surface, never under the pointer;
 *  it turns to the other side at the window's edge. It shows at once and does not move (DESIGN.md § 17). */
function HelpCard({ id, text, open }: { id: string; text: HelpText; open: Open }) {
  const ref = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const card = ref.current, win = open.el.ownerDocument.defaultView;
    if (!card || !win) return;
    const a = open.anchor.getBoundingClientRect(), t = open.el.getBoundingClientRect();
    const w = card.offsetWidth, h = card.offsetHeight, vw = win.innerWidth, vh = win.innerHeight, gap = 8, edge = 4;
    const clamp = (v: number, lo: number, hi: number) => Math.max(lo, Math.min(v, hi));
    let x: number, y: number;
    if (open.side === 'right') {
      x = a.right + gap;
      if (x + w > vw - edge) x = a.left - gap - w;
      if (x < edge) x = clamp(t.right + gap, edge, vw - w - edge);
      y = clamp(t.top, edge, vh - h - edge);
    } else {
      y = Math.max(a.bottom, t.bottom) + gap;
      if (y + h > vh - edge) y = Math.min(a.top, t.top) - gap - h;
      x = clamp(t.left, edge, vw - w - edge);
    }
    card.style.left = `${Math.round(x)}px`;
    card.style.top = `${Math.round(Math.max(edge, y))}px`;
    card.style.visibility = 'visible';
  });
  return (
    <div className="tool-card" id={id} role="tooltip" ref={ref} style={{ visibility: 'hidden' }}>
      <div className="tc-head">
        <span className="tc-name">{text.name}</span>
        {text.keys.length > 0 && <span className="tc-keys">{text.keys.map((k) => <kbd key={k}>{keyLabel(k)}</kbd>)}</span>}
      </div>
      <p className="tc-summary">{text.summary}</p>
      {text.note && <p className="tc-note">{text.note}</p>}
    </div>
  );
}

/** The words need about this much height under the tools (a summary and four lines) before the section opens by itself. */
const ROOM_FOR_WORDS = 112;

/** The section under the tools (the Folio tools view): the chosen tool's name, keys and what it does. The head folds it; folded, it still
 *  names the tool. It changes with the tool at once. `onRoom` says whether the view has room for the words under the tools and the fill
 *  and stroke (a person who never chose opens or folds by that: a short tools view keeps its buttons in sight). */
export function ToolHelpSection({ id, icon, open, onToggle, onRoom }: { id: HelpId; icon?: string; open: boolean; onToggle: () => void; onRoom?: (room: boolean) => void }) {
  const help = useContext(ToolHelpContext);
  const bodyId = useId();
  const ref = useRef<HTMLElement>(null);
  const text = help.describe(id);
  useEffect(() => {
    const view = ref.current?.parentElement, tools = view?.querySelector<HTMLElement>('.sb-tools-body');
    if (!view || !tools || !onRoom) return;
    // the tools' own height (scrollHeight: what they need, whatever the section takes) against the view's
    const measure = () => onRoom(view.clientHeight - tools.scrollHeight - (ref.current?.querySelector<HTMLElement>('.th-head')?.offsetHeight ?? 28) >= ROOM_FOR_WORDS);
    const watch = new ResizeObserver(measure);
    watch.observe(view);
    for (const child of tools.children) watch.observe(child);
    measure();
    return () => watch.disconnect();
  }, [onRoom, !!text]);  // eslint-disable-line react-hooks/exhaustive-deps
  if (!text) return null;
  return (
    <section className="tool-help" ref={ref} data-open={open ? 'true' : 'false'} data-help={id} aria-label={tr('고른 도구 설명')}>
      <button className="th-head" aria-expanded={open} aria-controls={bodyId} onClick={onToggle} title={open ? tr('도구 설명 접기') : tr('도구 설명 펼치기')}>
        <Icon name={open ? 'chevronDown' : 'chevronRight'} size={12} className="th-chev" />
        {icon && <Icon name={icon} size={16} className="th-icon" />}
        <span className="th-name">{text.name}</span>
        {text.keys.length > 0 && <span className="th-keys">{text.keys.map((k) => <kbd key={k}>{keyLabel(k)}</kbd>)}</span>}
      </button>
      {open && (
        <div className="th-body" id={bodyId}>
          <p className="th-summary">{text.summary}</p>
          <p className="th-detail">{text.detail}</p>
          {text.note && <p className="th-note">{text.note}</p>}
        </div>
      )}
    </section>
  );
}
