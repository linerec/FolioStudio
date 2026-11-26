// Panel dock, laid out like InDesign's "Essentials Classic" workspace: a column of panel icons
// (groups [Pages, Layers, Links], [Stroke, Color], [Swatches]) whose panels fly out to the
// left, an expanded area at the far right (Folio puts the project there, where InDesign has
// its cloud libraries), and floating panels for the rest (opened from the Window menu).
// Since uix-d1 (2026-11-20) there are three dock areas (left, right, bottom), each a split tree of tab groups, and a drag shows where
// a panel will land (join the tabs, split a group on one side, a new column or row at an area edge, an icon, or floating).
import { Fragment, useEffect, useMemo, useRef, useState } from 'react';
import type { Commands } from './commands';
import { Icon } from './icons';
import { PopupMenu } from './menu';
import type { MenuEntry } from './menudef';
import { tr } from '../i18n';
import { AREA_SIZE, MIN_SHARE, type Area, type DropTarget, type Edge, type ViewNode, type Zone, areaSize, areaTree, zoneGroups } from './docklayout';
import { dropKey, isAreaZone, placeGhost, view } from './dock-drop-target';
import { PANEL_META, type PanelId } from './dock-panels';
import { PopoutWindow } from './dock-popout';
import { canPopOut, type Dock, type HiddenPanels } from './dock-state';

export { PANEL_META, VIEWS, type PanelId, type ViewId } from './dock-panels';
export { CLASSIC, canPopOut, useDock, withViews, type Dock, type DockReply, type DockState, type HiddenPanels, type SidebarSeed } from './dock-state';

const POP_OUT_WHY = () => tr('이 기기에서는 창을 하나만 쓸 수 있어 패널을 다른 창으로 떼어 낼 수 없습니다.');

export type PanelHost = {
  render: (id: PanelId) => React.ReactNode;
  menu: (id: PanelId) => MenuEntry[];
  commands: Commands;
  /** Small number on a tab (missing links, preflight errors). */
  badge?: (id: PanelId) => number | null;
};

/** A place a drag can end: the element carries the DropTarget as data; it lights up while the drag is over it. */
const drop = (dock: Dock, to: DropTarget) => {
  const key = dropKey(to);
  return { 'data-drop': key, 'data-hot': dock.drag?.key === key ? 'true' : undefined } as const;
};

/** The thin place between two groups (shown while a drag is on): letting go there makes a group of its own. */
function Slot({ dock, zone, before }: { dock: Dock; zone: 'strip' | Area; before: number }) {
  if (!dock.drag) return null;
  return <div className="dock-slot" {...drop(dock, { zone, before })} />;
}

/** Panels a workspace shows elsewhere (the Folio workspace: the project is the sidebar's explorer). They keep their place in the layout,
 *  so going back to InDesign Classic shows them where they were; the screen only leaves them out (drops keep the real indices). */
/** In the Folio workspace the project is the explorer: the dock leaves the project panel out (it keeps its place for InDesign Classic). */
export const FOLIO_HIDES: HiddenPanels = new Set<PanelId>(['project']);
/** InDesign Classic has the project panel and its tools down the left edge: the Folio views keep their place in the layout, unseen. */
export const CLASSIC_HIDES: HiddenPanels = new Set<PanelId>(['explorer', 'toolbox']);
const shownIn = (panels: PanelId[], hide?: HiddenPanels) => (hide ? panels.filter((id) => !hide.has(id)) : panels);

/** The dock's own menu items for a group (the panel menu's "Docking" submenu, and the context menu of the tab strip): where to send
 *  it, float it or dock it back, maximize, fold, close. The same as dragging it, so the keyboard can do it too. */
function groupMenu(dock: Dock, zone: Zone, group: number): { items: MenuEntry[]; commands: Commands } {
  const s = dock.state;
  const g = zoneGroups(s, zone)[group] ?? [];
  const commands: Commands = {};
  const items: MenuEntry[] = [];
  const add = (id: string, label: string, run: (() => unknown) | undefined, checked?: boolean, why?: string) => {
    commands[id] = { label, run: run ? () => { run(); } : undefined, checked, why: run ? undefined : why };
    items.push({ cmd: id });
  };
  const send = (to: DropTarget) => () => dock.apply({ op: 'moveGroup', zone, group, to });
  add('dock.toLeft', tr('왼쪽 도크로 보내기'), zone === 'left' ? undefined : send({ zone: 'left', edge: 'bottom' }));
  add('dock.toRight', tr('오른쪽 도크로 보내기'), zone === 'column' ? undefined : send({ zone: 'column', before: s.column.length }));
  add('dock.toBottom', tr('아래 도크로 보내기'), zone === 'bottom' ? undefined : send({ zone: 'bottom', edge: 'right' }));
  add('dock.toStrip', tr('아이콘으로 접기'), zone === 'strip' ? undefined : send({ zone: 'strip', before: s.strip.length }));
  items.push({ sep: true });
  if (zone === 'floating') add('dock.back', tr('원래 자리로 도킹'), () => dock.apply({ op: 'dockFloating', group }));
  else add('dock.float', tr('띄우기'), () => dock.apply({ op: 'floatGroup', zone, group }));
  // another monitor: an OS window of its own (uix-d2); a phone has one window, so there it is grey with the reason
  const popped = zone === 'floating' && !!s.floating[group]?.window;
  if (popped) add('dock.popIn', tr('주 창 안으로 되돌리기'), () => dock.apply({ op: 'popIn', group }));
  else add('dock.popOut', tr('새 창으로 떼어 내기 (다른 모니터로)'), canPopOut() ? () => dock.act({ op: 'popOut', zone, group }) : undefined, undefined, POP_OUT_WHY());
  if (isAreaZone(zone)) {
    const max = !!s.maximized && g.includes(s.maximized);
    add('dock.maximize', max ? tr('최대화 되돌리기') : tr('최대화'), () => dock.apply({ op: 'maximizeGroup', zone, group, on: !max }), max);
    const folded = s.folded?.[g[0]] === true;
    add('dock.fold', folded ? tr('패널 그룹 펼치기') : tr('패널 그룹 접기'), () => dock.apply({ op: 'foldGroup', zone, group, folded: !folded }));
  }
  items.push({ sep: true });
  add('dock.closeGroup', tr('탭 그룹 닫기'), () => dock.apply({ op: 'closeGroup', zone, group }));
  // every tab of the group (a group of many tabs shows only some of them in its strip)
  if (g.length > 1) {
    const shown = zone === 'floating' ? s.floating[group]?.active : (s.active[g[0]] ?? g[0]);
    const tabs: MenuEntry[] = g.map((id) => { const cmd = `dock.tab.${id}`; commands[cmd] = { label: PANEL_META[id as PanelId]?.title ?? id, run: () => { dock.apply({ op: 'setActive', panel: id }); }, checked: id === shown }; return { cmd }; });
    items.push({ sep: true }, { label: tr('탭'), sub: tabs });
  }
  return { items, commands };
}

function PanelFrame({ panels, active: chosen, onTab, onClose, host, onDragStart, collapse, dock, zone, group, folded, onFold, hide, maximized }: {
  panels: PanelId[];
  active: PanelId;
  onTab: (id: PanelId) => void;
  onClose?: () => void;
  host: PanelHost;
  onDragStart?: (e: React.PointerEvent) => void;
  collapse?: () => void;
  dock: Dock;
  zone: Zone;
  group: number;
  /** Folded to its title strip (area groups). */
  folded?: boolean;
  onFold?: () => void;
  hide?: HiddenPanels;
  /** This group has its area to itself for now (area groups); the title strip shows a button that gives the others back. */
  maximized?: boolean;
}) {
  const [menu, setMenu] = useState<{ at: DOMRect; dockOnly: boolean } | null>(null);
  const active = hide?.has(chosen) ? shownIn(panels, hide)[0] ?? chosen : chosen;
  const head = useRef<HTMLElement | null>(null);
  // the tab that shows is scrolled into view in a strip of many tabs, clear of the buttons stuck to the strip's right end (instantly:
  // DESIGN.md § 12). Measured by hand: scrollIntoView did nothing in a window that is not shown (uix-d1 saw the tab stay out of sight).
  useEffect(() => {
    const h = head.current;
    const t = h?.querySelector<HTMLElement>('.dpanel-tab.on');
    if (!h || !t || h.scrollWidth <= h.clientWidth) return;
    const covered = [...h.querySelectorAll<HTMLElement>('.dpanel-menu, .dpanel-x, .dpanel-max')].reduce((n, b) => n + b.offsetWidth, 0);
    const box = h.getBoundingClientRect(), tab = t.getBoundingClientRect();
    const room = box.width - covered;
    if (tab.left < box.left) h.scrollLeft -= box.left - tab.left + 4;
    else if (tab.right > box.left + room) h.scrollLeft += tab.right - (box.left + room) + 4;
  }, [active, panels.length]);
  const own = host.menu(active);
  const docking = menu ? groupMenu(dock, zone, group) : null;
  // the panel menu (≡) stays InDesign's; the dock's own menu is the title strip's context menu (right button, or the menu key /
  // Shift+F10 on a focused tab: the keyboard way to send, float, maximize or close a group)
  const items: MenuEntry[] = !menu || !docking ? [] : menu.dockOnly ? docking.items : (own.length ? own : [{ cmd: 'panel.none' }]);
  return (
    <section className={`dpanel${folded ? ' folded' : ''}${maximized ? ' maximized' : ''}`} data-zone={zone} data-group={group}>
      <header className="dpanel-tabs" ref={head} onPointerDown={onDragStart} onDoubleClick={collapse} {...drop(dock, { zone, group })}
        // the wheel scrolls a strip of many tabs sideways (as VS Code's and the browsers' tab strips do)
        onWheel={(e) => { const h = head.current; if (h && h.scrollWidth > h.clientWidth && Math.abs(e.deltaY) > Math.abs(e.deltaX)) h.scrollLeft += e.deltaY; }}
        onContextMenu={(e) => {
          if ((e.target as HTMLElement).closest('.dpanel-menu')) return;
          e.preventDefault();
          // from the keyboard the event has no pointer place: the menu opens under the focused tab
          const r = (e.target as HTMLElement).getBoundingClientRect();
          setMenu({ at: e.clientX || e.clientY ? new DOMRect(e.clientX, e.clientY, 0, 0) : new DOMRect(r.left, r.bottom - 2, 0, 0), dockOnly: true });
        }}>
        {onFold && <button className="dpanel-fold" onPointerDown={(e) => e.stopPropagation()} onClick={onFold} title={folded ? tr('패널 그룹 펼치기') : tr('패널 그룹 접기')} aria-label={folded ? tr('패널 그룹 펼치기') : tr('패널 그룹 접기')} aria-expanded={!folded}><Icon name={folded ? 'chevronRight' : 'chevronDown'} size={12} /></button>}
        {panels.map((id, i) => {
          if (hide?.has(id)) return null;
          const b = host.badge?.(id);
          return (
            <button
              key={id}
              data-panel={id}
              className={`dpanel-tab${id === active ? ' on' : ''}`}
              {...drop(dock, { zone, group, index: i })}
              onPointerDown={(e) => { e.stopPropagation(); dock.beginDrag(e, { kind: 'panel', id }); }}
              onClick={() => { if (!dock.dragged()) onTab(id); }}
              title={PANEL_META[id].title}
            >
              <span className="dpanel-tab-name">{PANEL_META[id].title}</span>
              {b ? <em>{b}</em> : null}
            </button>
          );
        })}
        <span className="dpanel-fill" onPointerDown={zone === 'floating' ? undefined : (e) => { e.preventDefault(); dock.beginDrag(e, { kind: 'group', zone, group }); }} />
        {maximized && <button className="dpanel-max" onPointerDown={(e) => e.stopPropagation()} onClick={() => isAreaZone(zone) && dock.apply({ op: 'maximizeGroup', zone, group, on: false })} title={tr('최대화 되돌리기')} aria-label={tr('최대화 되돌리기')}><Icon name="doubleLeft" size={12} /></button>}
        {onClose && <button className="dpanel-x" onPointerDown={(e) => e.stopPropagation()} onClick={onClose} title={tr("닫기")}><Icon name="close" size={12} /></button>}
        <button className="dpanel-menu" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => setMenu({ at: e.currentTarget.getBoundingClientRect(), dockOnly: false })} title={tr("패널 메뉴")}><Icon name="menu" size={14} /></button>
      </header>
      {!folded && <div className="dpanel-body">{host.render(active)}</div>}
      {menu && docking && <PopupMenu items={items} commands={{ ...host.commands, ...docking.commands }} anchor={menu.at} onClose={() => setMenu(null)} />}
    </section>
  );
}

export function DockStrip({ dock, host, hide }: { dock: Dock; host: PanelHost; hide?: HiddenPanels }) {
  const { state, flyout, setFlyout } = dock;
  const groups = useRef<(HTMLDivElement | null)[]>([]);
  const [rect, setRect] = useState<DOMRect | null>(null);

  useEffect(() => {
    if (!flyout) return;
    setRect(groups.current[flyout.group]?.getBoundingClientRect() ?? null);
    const onDown = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (!(t instanceof Element) || (!t.closest('.flyout') && !t.closest('.dock-strip') && !t.closest('.dropdown') && !t.closest('.overlay'))) setFlyout(null);
    };
    window.addEventListener('mousedown', onDown, true);
    return () => window.removeEventListener('mousedown', onDown, true);
  }, [flyout, setFlyout]);

  if (dock.hidden) return null;
  const group = flyout ? state.strip[flyout.group] : null;
  const meta = flyout ? PANEL_META[flyout.active] : null;
  // with every icon moved out the strip keeps only a thin edge (it took its 118 px with nothing in it, uix-d2's break-ui); the edge
  // still takes a drop
  const empty = !state.strip.some((g) => shownIn(g, hide).length > 0);
  return (
    <aside className={`dock-strip${empty ? ' empty' : ''}`} {...drop(dock, { zone: 'strip', before: state.strip.length })} title={empty ? tr('아이콘으로 접기') : undefined}>
      <div className="dock-grip" />
      {state.strip.map((g, gi) => (
        <Fragment key={g[0]}>
          <Slot dock={dock} zone="strip" before={gi} />
          {shownIn(g, hide).length > 0 && <div className="dock-group" ref={(el) => { groups.current[gi] = el; }} {...drop(dock, { zone: 'strip', group: gi })}>
            {g.map((id, i) => {
              if (hide?.has(id)) return null;
              const b = host.badge?.(id);
              const on = flyout?.group === gi && flyout.active === id;
              return (
                <button
                  key={id}
                  data-panel={id}
                  className={`dock-icon${on ? ' on' : ''}`}
                  onPointerDown={(e) => dock.beginDrag(e, { kind: 'panel', id })}
                  onClick={() => { if (!dock.dragged()) setFlyout(on ? null : { group: gi, active: id }); }}
                  title={PANEL_META[id].title}
                  {...drop(dock, { zone: 'strip', group: gi, index: i })}
                >
                  <Icon name={PANEL_META[id].icon} size={18} />
                  <span>{PANEL_META[id].title}</span>
                  {b ? <em>{b}</em> : null}
                </button>
              );
            })}
          </div>}
        </Fragment>
      ))}
      <Slot dock={dock} zone="strip" before={state.strip.length} />
      {group && flyout && rect && meta && (
        <div
          className="flyout"
          style={{
            right: window.innerWidth - rect.left + 1,
            top: Math.max(60, Math.min(rect.top, window.innerHeight - meta.height - 30)),
            width: meta.width,
            height: meta.height,
          }}
        >
          <PanelFrame panels={group} active={flyout.active} onTab={(id) => setFlyout({ group: flyout.group, active: id })} host={host} collapse={() => setFlyout(null)} dock={dock} zone="strip" group={flyout.group} hide={hide} />
        </div>
      )}
    </aside>
  );
}

/**
 * The line between two parts of a split (or between an area and the document): dragged, it moves (the DOM follows the pointer, the
 * command runs on letting go); a double click evens the parts (or gives the area its default size); with the focus, the arrow keys
 * move it. `onDone` gets the new shares (or the new size in pixels).
 */
function Sash({ dir, label, measure, onDone, onReset, live }: {
  /** `row`: the line stands between parts side by side (it moves left and right); `col`: between parts one above another. */
  dir: 'row' | 'col';
  label: string;
  /** The value at the start of a drag and its bounds, and how the pointer's movement changes it. */
  measure: () => { value: number; min: number; max: number; per: number };
  onDone: (value: number) => void;
  onReset: () => void;
  /** Shows a value while the drag goes on (DOM only). */
  live: (value: number) => void;
}) {
  const down = (e: React.PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    const el = e.currentTarget;
    el.setPointerCapture(e.pointerId);
    const m = measure();
    const start = dir === 'row' ? e.clientX : e.clientY;
    let value = m.value;
    el.classList.add('dragging');
    const move = (ev: PointerEvent) => { value = Math.min(m.max, Math.max(m.min, m.value + ((dir === 'row' ? ev.clientX : ev.clientY) - start) * m.per)); live(value); };
    const up = () => { el.removeEventListener('pointermove', move); el.removeEventListener('pointerup', up); el.removeEventListener('lostpointercapture', up); el.classList.remove('dragging'); if (value !== m.value) onDone(value); };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('lostpointercapture', up);
  };
  const key = (e: React.KeyboardEvent) => {
    const back = dir === 'row' ? 'ArrowLeft' : 'ArrowUp', on = dir === 'row' ? 'ArrowRight' : 'ArrowDown';
    if (e.key !== back && e.key !== on) return;
    e.preventDefault();
    e.stopPropagation();
    const m = measure();
    const step = (e.shiftKey ? 48 : 12) * Math.abs(m.per);
    onDone(Math.min(m.max, Math.max(m.min, m.value + (e.key === on ? 1 : -1) * step * Math.sign(m.per || 1))));
  };
  return <div className={`dock-sash ${dir}`} role="separator" aria-orientation={dir === 'row' ? 'vertical' : 'horizontal'} aria-label={label} title={label} tabIndex={0}
    onPointerDown={down} onDoubleClick={onReset} onKeyDown={key} />;
}

/** Shares (summing to 1) become flex grows this many times larger: grows that sum below 1 take only that part of the free room, so a
 *  group folded to its title strip left its share empty instead of giving it to the others (uix-d2 saw it in the sidebar). */
const GROW = 1000;

const AREA_LABEL = () => ({ left: tr('왼쪽 도크'), column: tr('오른쪽 도크'), bottom: tr('아래 도크') });

/**
 * One dock area: its split tree of tab groups, the lines between the parts, the line between the area and the document (its size),
 * and, while a drag is on, the places at its outer edges (a new column or row across the area) and between groups.
 * The right area keeps the classes the older screens and checks know (`.dock-column`, `.dock-colhead`, `.column-collapsed`).
 */
export function DockArea({ dock, host, hide, area, sidebar }: {
  dock: Dock; host: PanelHost; hide?: HiddenPanels; area: Area;
  /** The Folio workspace's left area, its sidebar (uix-d2): no head of its own (the activity bar folds it and shows its icons), and
   *  folded it is only the activity bar. The class `folio-sidebar` stays the sidebar's (uix-c1's checks find the explorer by it). */
  sidebar?: boolean;
}) {
  const { state } = dock;
  const box = useRef<HTMLElement | null>(null);
  // Shift+Tab hides the panels but the tools: the Folio sidebar holds the tools, so it stays
  if (dock.hidden && !sidebar) return null;
  const groups = zoneGroups(state, area) as PanelId[][];
  const visible = groups.some((g) => shownIn(g, hide).length);
  const cls = area === 'column' ? 'dock-column' : `dock-area ${area}${sidebar ? ' folio-sidebar' : ''}`;
  const size = areaSize(state, area);
  const sizeStyle = area === 'bottom' ? { height: size } : { width: size };
  // an empty area shows only while a drag is on: the right one as a column to drop into (as before), the left and bottom ones as a
  // thin band along the document's edge that takes no room (the document does not move when a drag begins: DESIGN.md § 12)
  if (!visible) {
    if (!dock.drag) return null;
    if (area === 'column') {
      // a band along the document's right edge too (the Folio workspace's right area is empty to begin with: a column that pushed the
      // document aside at every drag made the place under the pointer jump, uix-d2). `dock-column empty` stays its name (dock-ui-check).
      const col = document.querySelector<HTMLElement>('.doc-col');
      const at = col ? { left: col.offsetLeft + col.offsetWidth - 28, top: col.offsetTop, height: col.offsetHeight } : { right: 0, top: 0, bottom: 0 };
      return <div className="dock-column empty dock-band right" data-area="column" style={at} {...drop(dock, { zone: 'column', before: 0 })} title={AREA_LABEL().column}><span>{AREA_LABEL().column}</span></div>;
    }
    // (a group of its own in front of the first: with panels this workspace leaves out in the area, the newcomer joins their column,
    // so the area does not widen for groups it does not show)
    return <div className={`dock-band ${area}`} data-area={area} {...drop(dock, { zone: area, before: 0 })} title={AREA_LABEL()[area]}><span>{AREA_LABEL()[area]}</span></div>;
  }
  const collapsed = area === 'column' ? state.columnCollapsed : state.collapsed?.[area] === true;
  const collapse = (v: boolean) => dock.apply(area === 'column' ? { op: 'collapseColumn', collapsed: v } : { op: 'collapseArea', zone: area, collapsed: v });
  if (collapsed) {
    if (sidebar) return null;
    return (
      <aside className={`dock-strip column-collapsed${area === 'column' ? '' : ` area-collapsed ${area}`}`} data-area={area} {...drop(dock, { zone: area, before: groups.length })}>
        <button className="dock-grip-button" onClick={() => collapse(false)} title={tr("패널 펼치기")}><Icon name={area === 'left' ? 'doubleRight' : 'doubleLeft'} size={12} /></button>
        {groups.map((g) => shownIn(g, hide).map((id) => (
          <button key={id} data-panel={id} className="dock-icon" onClick={() => dock.apply({ op: 'openPanel', panel: id })} title={PANEL_META[id].title}>
            <Icon name={PANEL_META[id].icon} size={18} />
            <span>{PANEL_META[id].title}</span>
          </button>
        )))}
      </aside>
    );
  }
  const maxGroup = state.maximized ? groups.findIndex((g) => g.includes(state.maximized as PanelId)) : -1;
  const frame = (gi: number) => {
    const g = groups[gi];
    const active = state.active[g[0]] ?? g[0];
    const folded = state.folded?.[g[0]] === true;
    return (
      <PanelFrame
        key={g[0]}
        hide={hide}
        panels={g}
        active={active}
        onTab={(id) => dock.apply({ op: 'setActive', panel: id })}
        host={host}
        collapse={() => collapse(true)}
        dock={dock}
        zone={area}
        group={gi}
        folded={folded}
        maximized={gi === maxGroup}
        onFold={() => dock.apply({ op: 'foldGroup', zone: area, group: gi, folded: !folded })}
      />
    );
  };
  const seen = (n: ViewNode): boolean => (n.kind === 'group' ? shownIn(groups[n.group], hide).length > 0 : n.parts.some(seen));
  // the shares of a split as the DOM shows them while a line between two parts is dragged
  const node = (n: ViewNode, top: boolean): React.ReactNode => {
    if (n.kind === 'group') {
      const g = groups[n.group];
      return shownIn(g, hide).length ? frame(n.group) : null;
    }
    const out: React.ReactNode[] = [];
    // a part with nothing to show (the panels this workspace leaves out: the Folio views in InDesign Classic, the project panel in
    // Folio) keeps its place in the tree but is not drawn, and the line before the next part sizes it against the last part shown
    let prev = -1;
    n.parts.forEach((p, i) => {
      const folded = p.kind === 'group' && state.folded?.[groups[p.group][0]] === true;
      const shows = seen(p);
      const j = prev;
      if (shows && j >= 0) {
        out.push(<Sash key={`s${i}`} dir={n.dir} label={tr('패널 크기 조절')}
          measure={() => {
            const el = box.current?.querySelector<HTMLElement>(`[data-split="${n.path.join('.')}"]`);
            const total = el ? (n.dir === 'row' ? el.clientWidth : el.clientHeight) : 600;
            const pair = n.sizes[j] + n.sizes[i];
            return { value: n.sizes[j], min: MIN_SHARE, max: pair - MIN_SHARE, per: 1 / Math.max(1, total) };
          }}
          live={(v) => {
            const el = box.current?.querySelector<HTMLElement>(`[data-split="${n.path.join('.')}"]`);
            const kids = el ? [...el.children].filter((c) => c.classList.contains('dock-part')) as HTMLElement[] : [];
            const pair = n.sizes[j] + n.sizes[i];
            if (kids[j]) kids[j].style.flexGrow = String(v * GROW);
            if (kids[i]) kids[i].style.flexGrow = String((pair - v) * GROW);
          }}
          onDone={(v) => { const sizes = [...n.sizes]; const pair = sizes[j] + sizes[i]; sizes[j] = v; sizes[i] = pair - v; dock.apply({ op: 'resizeSplit', zone: area, path: n.path, sizes }); }}
          onReset={() => dock.apply({ op: 'equalizeSplit', zone: area, path: n.path })} />);
      }
      // the place before a group of a split that runs the area's own way (between two groups one above another)
      if (shows && p.kind === 'group' && n.dir === (area === 'bottom' ? 'row' : 'col') && area !== 'bottom') out.push(<Slot key={`b${p.group}`} dock={dock} zone={area} before={p.group} />);
      out.push(<div key={`p${i}`} className={`dock-part${folded ? ' folded' : ''}${shows ? '' : ' unseen'}`} style={{ flexGrow: folded ? 0 : n.sizes[i] * GROW }}>{shows ? node(p, false) : null}</div>);
      if (shows) prev = i;
    });
    if (top && n.dir === 'col' && area !== 'bottom') out.push(<Slot key="end" dock={dock} zone={area} before={groups.length} />);
    return <div className={`dock-split ${n.dir}`} data-split={n.path.join('.')}>{out}</div>;
  };
  const tree = maxGroup >= 0 ? ({ kind: 'group', group: maxGroup } as ViewNode) : areaTree(state, area);
  const edges: Edge[] = ['left', 'right', 'top', 'bottom'];
  const b = AREA_SIZE[area];
  return (
    <aside className={cls} data-area={area} style={sizeStyle} ref={(el) => { box.current = el; }} {...drop(dock, { zone: area, before: groups.length })}>
      {area === 'column' && (
        <div className="dock-colhead">
          <button className="dock-grip-button" onClick={() => collapse(true)} title={tr("아이콘으로 축소")}><Icon name="doubleRight" size={12} /></button>
        </div>
      )}
      {area !== 'column' && !sidebar && (
        <div className="dock-colhead">
          <span className="dock-areaname">{AREA_LABEL()[area]}</span>
          <button className="dock-grip-button" onClick={() => collapse(true)} title={tr("아이콘으로 축소")}><Icon name={area === 'left' ? 'doubleLeft' : 'chevronDown'} size={12} /></button>
        </div>
      )}
      <div className="dock-tree">
        {!tree ? null : tree.kind === 'group' ? <>{area !== 'bottom' && <Slot dock={dock} zone={area} before={tree.group} />}{node(tree, true)}{area !== 'bottom' && <Slot dock={dock} zone={area} before={groups.length} />}</> : node(tree, true)}
      </div>
      <Sash dir={area === 'bottom' ? 'col' : 'row'} label={tr('도크 크기 조절')}
        measure={() => ({ value: size, min: b.min, max: b.max, per: area === 'column' ? -1 : area === 'left' ? 1 : -1 })}
        live={(v) => { if (box.current) { if (area === 'bottom') box.current.style.height = `${v}px`; else box.current.style.width = `${v}px`; } }}
        onDone={(v) => dock.apply({ op: 'setAreaSize', zone: area, size: v, view: view() })}
        onReset={() => dock.apply({ op: 'setAreaSize', zone: area, size: b.def })} />
      {dock.drag && edges.map((edge) => <div key={edge} className={`dock-edge ${edge}`} {...drop(dock, { zone: area, edge })} title={AREA_LABEL()[area]} />)}
    </aside>
  );
}

export function DockColumn({ dock, host, hide }: { dock: Dock; host: PanelHost; hide?: HiddenPanels }) {
  return <DockArea dock={dock} host={host} hide={hide} area="column" />;
}

/** Whether the Folio sidebar (the left area) shows: it has a panel to show and is not folded to the activity bar. */
export function sidebarOpen(dock: Dock, hide?: HiddenPanels) {
  const s = dock.state;
  return !s.collapsed?.left && zoneGroups(s, 'left').some((g) => shownIn(g as PanelId[], hide).length > 0);
}

/** Window ▸ Sidebar (Ctrl+Alt+5) and the activity bar's last button: the left area folds to the bar or opens; with nothing in it,
 *  the explorer comes back to it. */
export function toggleSidebar(dock: Dock, hide?: HiddenPanels) {
  const s = dock.state;
  if (!zoneGroups(s, 'left').some((g) => shownIn(g as PanelId[], hide).length > 0)) { dock.showView('explorer'); return; }
  dock.setHidden(false);
  dock.apply({ op: 'collapseArea', zone: 'left', collapsed: !s.collapsed?.left });
}

/**
 * The Folio workspace's activity bar (uix-d2): a button for each panel docked in the left area, in its order (the explorer and the
 * tools to begin with, and whatever a person docks there), and the sidebar's open / close at the bottom. A button shows its panel (the
 * area unfolds, its tab comes to the front, its group unfolds); pressed while it shows, it folds the panel's group, or the whole area
 * when the other groups are folded already (as the sidebar's tiers did, uix-c1). Its buttons drag like dock icons, and a panel dropped
 * on the bar joins the left area. Nothing moves (DESIGN.md § 6).
 */
export function ActivityBar({ dock, hide }: { dock: Dock; hide?: HiddenPanels }) {
  const s = dock.state;
  const groups = zoneGroups(s, 'left') as PanelId[][];
  const ids = groups.flatMap((g) => shownIn(g, hide));
  const open = sidebarOpen(dock, hide);
  const press = (id: PanelId) => {
    if (dock.dragged()) return;
    if (dock.showing(id)) dock.hideView(id); else { dock.setHidden(false); dock.apply({ op: 'openPanel', panel: id, view: view() }); }
  };
  return (
    <nav className="activity" aria-label={tr('활동 막대')} {...drop(dock, { zone: 'left', before: groups.length })}>
      {ids.map((id) => {
        const on = dock.showing(id);
        return (
          <button key={id} data-panel={id} className={`activity-btn${on ? ' on' : ''}`} aria-pressed={on}
            onPointerDown={(e) => dock.beginDrag(e, { kind: 'panel', id })} onClick={() => press(id)}
            title={id === 'explorer' ? tr('탐색기 (창 ▸ 프로젝트)') : PANEL_META[id].title} aria-label={PANEL_META[id].title}>
            <Icon name={PANEL_META[id].icon} size={20} />
          </button>
        );
      })}
      <span className="activity-fill" />
      <button className="activity-btn" onClick={() => toggleSidebar(dock, hide)} title={open ? tr('사이드바 닫기 (Ctrl+Alt+5)') : tr('사이드바 열기 (Ctrl+Alt+5)')}
        aria-label={open ? tr('사이드바 닫기 (Ctrl+Alt+5)') : tr('사이드바 열기 (Ctrl+Alt+5)')} aria-pressed={open}><Icon name="sidebar" size={18} /></button>
    </nav>
  );
}

const RESIZE = ['n', 's', 'e', 'w', 'ne', 'nw', 'se', 'sw'] as const;

export function FloatingPanels({ dock, host, hide }: { dock: Dock; host: PanelHost; hide?: HiddenPanels }) {
  const { state } = dock;
  const [top, setTop] = useState<number | null>(null);
  const order = useMemo(() => state.floating.map((_, i) => i).sort((a, b) => (a === top ? 1 : b === top ? -1 : a - b)), [state.floating, top]);
  // A panel window keeps its component, and so its OS window, while its group keeps one of its panels (uix-d3): reordered tabs, the
  // first tab dragged away or another window's tabs joining do not close and reopen it. Each panel remembers its window's key.
  const winKeys = useRef<{ of: Map<string, string>; next: number }>({ of: new Map(), next: 1 });
  const keyOf: (string | null)[] = [];
  {
    const prev = winKeys.current.of, of = new Map<string, string>(), used = new Set<string>();
    for (const f of state.floating) {
      if (!f.window) { keyOf.push(null); continue; }
      const k = f.panels.map((id) => prev.get(id)).find((x): x is string => !!x && !used.has(x)) ?? `w${f.panels[0]}#${winKeys.current.next++}`;
      used.add(k);
      for (const id of f.panels) of.set(id, k);
      keyOf.push(k);
    }
    winKeys.current.of = of;
  }
  if (dock.hidden) return null;
  const preview = dock.drag?.preview;
  return (
    <>
      {order.map((i) => {
        const f = state.floating[i];
        if (!shownIn(f.panels, hide).length) return null;
        // a group in an OS window of its own: drawn there (it moves and sizes with the window, its title is the window's)
        if (f.window && canPopOut()) {
          return (
            <PopoutWindow key={keyOf[i] ?? `w${f.panels[0]}`} dock={dock} group={i} panels={f.panels} rect={f.window}>
              <div className="floating popped" data-float={i}>
                <PanelFrame panels={f.panels} active={f.active} onTab={(id) => dock.apply({ op: 'setActive', panel: id })}
                  onClose={() => dock.apply({ op: 'closeGroup', zone: 'floating', group: i })} host={host} dock={dock} zone="floating" group={i} hide={hide}
                  // the group dragged by its title's empty part: onto the app window's dock, another panel window, or elsewhere on the desktop (uix-d3)
                  onDragStart={(e) => { if (!(e.target as HTMLElement).closest('button')) { e.preventDefault(); dock.beginDrag(e, { kind: 'group', zone: 'floating', group: i }); } }} />
              </div>
            </PopoutWindow>
          );
        }
        return (
          <div
            key={`${f.panels.join(',')}`}
            className="floating"
            data-float={i}
            style={{ left: f.x, top: f.y, width: f.w, height: f.h }}
            onPointerDown={() => setTop(i)}
            onDoubleClick={(e) => {
              // a double click on the title (not on a tab or a button) puts the panel back where it was docked
              const t = e.target as HTMLElement;
              if (t.closest('.dpanel-tabs') && !t.closest('button')) dock.apply({ op: 'dockFloating', group: i });
            }}
          >
            <PanelFrame
              panels={f.panels}
              active={f.active}
              onTab={(id) => dock.apply({ op: 'setActive', panel: id })}
              onClose={() => dock.apply({ op: 'closeGroup', zone: 'floating', group: i })}
              host={host}
              dock={dock}
              zone="floating"
              group={i}
              hide={hide}
              onDragStart={(e) => dock.beginFloat(e, i)}
            />
            {RESIZE.map((r) => <div key={r} className={`fl-rz ${r}`} onPointerDown={(e) => { e.stopPropagation(); setTop(i); dock.beginFloat(e, i, r); }} />)}
          </div>
        );
      })}
      {/* (over a panel window, the preview and the ghost show in that window: PopoutWindow) */}
      {preview && !dock.drag?.win && <div className="dock-preview" style={{ left: preview.x, top: preview.y, width: preview.w, height: preview.h }} />}
      {dock.drag && dock.drag.ghost && !dock.drag.win && <div className="dock-ghost" style={placeGhost({}, dock.drag.at.x, dock.drag.at.y)}>{dock.drag.label}</div>}
    </>
  );
}
