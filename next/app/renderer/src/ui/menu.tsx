// Menu bar (in the window's title row, like InDesign on Windows) and drop-down menus, also
// used for panel menus. Keyboard: Alt+letter opens a menu, arrows move, Enter runs, Esc closes.
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { type Commands, isEnabled, keyLabel, isMac } from './commands';
import { type MenuDef, type MenuEntry, SPECS } from './menudef';

type Props = { menus: MenuDef[]; commands: Commands; right?: React.ReactNode };

export function MenuBar({ menus, commands, right }: Props) {
  const [open, setOpen] = useState<number | null>(null);
  const bar = useRef<HTMLDivElement>(null);
  const titles = useRef<(HTMLButtonElement | null)[]>([]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!isMac && e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && /^Key[A-Z]$/.test(e.code)) {
        const i = menus.findIndex((m) => `Key${m.mnemonic}` === e.code);
        if (i >= 0) { e.preventDefault(); setOpen(i); }
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [menus]);

  const rect = open !== null ? titles.current[open]?.getBoundingClientRect() : null;
  return (
    <div className="menubar" ref={bar}>
      <div className="app-mark" title="Folio Studio">
        {/* a folio page with its corner turned, an F set in it (drawn, the same in every OS font) */}
        <svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">
          <path d="M4.5 2.5h8l3 3v12h-11z" style={{ fill: 'var(--accent)' }} />
          <path d="M12.5 2.5v3h3" fill="none" style={{ stroke: 'var(--accent-text)' }} strokeWidth="1.2" strokeLinejoin="round" opacity="0.7" />
          <path d="M8 14.5v-7h4.2M8 11h3.4" fill="none" style={{ stroke: 'var(--accent-text)' }} strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>
      <nav className="menu-titles">
        {menus.map((m, i) => (
          <button
            key={m.label}
            ref={(el) => { titles.current[i] = el; }}
            className={open === i ? 'menu-title on' : 'menu-title'}
            onMouseDown={(e) => { e.preventDefault(); setOpen(open === i ? null : i); }}
            onMouseEnter={() => { if (open !== null) setOpen(i); }}
          >
            <MenuTitle label={m.label} mnemonic={m.mnemonic} />
          </button>
        ))}
      </nav>
      <div className="menubar-drag" />
      {right}
      {open !== null && rect && (
        <Dropdown
          items={menus[open].items}
          commands={commands}
          x={rect.left}
          y={rect.bottom}
          onClose={() => setOpen(null)}
          onStep={(d) => setOpen((o) => (o === null ? o : (o + d + menus.length) % menus.length))}
        />
      )}
    </div>
  );
}

type DropdownProps = {
  items: MenuEntry[];
  commands: Commands;
  x: number;
  y: number;
  onClose: () => void;
  /** Left/right arrow on a top-level menu: the neighbouring menu. */
  onStep?: (d: -1 | 1) => void;
  nested?: boolean;
  /** Closes the whole menu chain (a nested menu's onClose closes only itself). */
  onCloseAll?: () => void;
};

export function Dropdown({ items: entries, commands, x, y, onClose, onStep, nested, onCloseAll }: DropdownProps) {
  const items = useMemo(() => entries.flatMap<MenuEntry>(it => 'cmd' in it ? commands[it.cmd]?.items ?? [it] : [it]), [entries, commands]);
  const closeAll = onCloseAll ?? onClose;
  const ref = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState(-1);
  const [subOpen, setSubOpen] = useState<number | null>(null);
  const [pos, setPos] = useState({ x, y });
  const rows = useRef<(HTMLDivElement | null)[]>([]);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    setPos({
      x: Math.max(0, Math.min(x, window.innerWidth - r.width - 4)),
      y: Math.max(0, Math.min(y, window.innerHeight - r.height - 4)),
    });
  }, [x, y]);

  const runnable = useCallback(
    (i: number) => {
      const it = items[i];
      if (!it || 'sep' in it) return false;
      if ('sub' in it) return true;
      return isEnabled(commands[it.cmd]);
    },
    [items, commands],
  );

  const choose = useCallback(
    (i: number) => {
      const it = items[i];
      if (!it || 'sep' in it) return;
      if ('sub' in it) { setSubOpen(i); return; }
      const cmd = commands[it.cmd];
      if (!isEnabled(cmd)) return;
      closeAll();
      // let the menu close before a dialog opens
      window.setTimeout(() => cmd.run?.(), 0);
    },
    [items, commands, closeAll],
  );

  useEffect(() => {
    if (nested) return;
    const onDown = (e: MouseEvent) => {
      const t = e.target as HTMLElement;
      if (!(t instanceof Element) || (!t.closest('.dropdown') && !t.closest('.menu-title'))) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); onClose(); }
    };
    window.addEventListener('mousedown', onDown, true);
    window.addEventListener('keydown', onKey, true);
    window.addEventListener('blur', onClose);
    return () => {
      window.removeEventListener('mousedown', onDown, true);
      window.removeEventListener('keydown', onKey, true);
      window.removeEventListener('blur', onClose);
    };
  }, [onClose, nested]);

  // arrows and Enter while this (innermost) menu is open
  useEffect(() => {
    if (subOpen !== null) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        e.stopPropagation();
        const d = e.key === 'ArrowDown' ? 1 : -1;
        let i = active;
        for (let n = 0; n < items.length; n++) {
          i = (i + d + items.length) % items.length;
          if (runnable(i)) break;
        }
        setActive(i);
        rows.current[i]?.scrollIntoView({ block: 'nearest' });
      } else if (e.key === 'Enter' && active >= 0) {
        e.preventDefault();
        e.stopPropagation();
        choose(active);
      } else if (e.key === 'ArrowRight') {
        e.preventDefault();
        e.stopPropagation();
        const it = items[active];
        if (it && 'sub' in it) setSubOpen(active);
        else onStep?.(1);
      } else if (e.key === 'ArrowLeft') {
        e.preventDefault();
        e.stopPropagation();
        if (nested) onClose();
        else onStep?.(-1);
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [active, items, runnable, choose, subOpen, onStep, nested, onClose]);

  const subRect = subOpen !== null ? rows.current[subOpen]?.getBoundingClientRect() : null;
  return (
    <div className="dropdown" ref={ref} style={{ left: pos.x, top: pos.y }} onMouseDown={(e) => e.preventDefault()}>
      {items.map((it, i) => {
        if ('sep' in it) return <div key={`s${i}`} className="dd-sep" />;
        if ('sub' in it) {
          return (
            <div
              key={it.label}
              ref={(el) => { rows.current[i] = el; }}
              className={`dd-item${active === i || subOpen === i ? ' on' : ''}`}
              onMouseEnter={() => { setActive(i); setSubOpen(i); }}
              onClick={() => setSubOpen(i)}
            >
              <span className="dd-check" />
              <span className="dd-label">{it.label}</span>
              <span className="dd-key" />
              <span className="dd-arrow">›</span>
            </div>
          );
        }
        const cmd = commands[it.cmd];
        const label = cmd?.label ?? SPECS[it.cmd]?.label ?? it.cmd;
        const keys = cmd?.keys?.[0] ?? SPECS[it.cmd]?.keys?.[0];
        const on = isEnabled(cmd);
        return (
          <div
            key={it.cmd}
            ref={(el) => { rows.current[i] = el; }}
            className={`dd-item${on ? '' : ' off'}${active === i ? ' on' : ''}`}
            aria-disabled={!on} title={!on ? cmd?.why : undefined}
            onMouseEnter={() => { setActive(i); setSubOpen(null); }}
            onClick={() => choose(i)}
          >
            <span className="dd-check">{cmd?.checked ? '✓' : ''}</span>
            <span className="dd-label">{label}</span>
            <span className="dd-key">{keyLabel(keys)}</span>
            <span className="dd-arrow" />
          </div>
        );
      })}
      {subOpen !== null && subRect && 'sub' in items[subOpen] && (
        <Dropdown
          items={(items[subOpen] as { sub: MenuEntry[] }).sub}
          commands={commands}
          x={subRect.right - 2}
          y={subRect.top - 4}
          onClose={() => { setSubOpen(null); }}
          onCloseAll={closeAll}
          nested
        />
      )}
    </div>
  );
}

/** A small menu opened by a button (panel menus, flyouts); closes on outside click. */
export function PopupMenu({ items, commands, anchor, onClose }: { items: MenuEntry[]; commands: Commands; anchor: DOMRect; onClose: () => void }) {
  return <Dropdown items={items} commands={commands} x={anchor.left} y={anchor.bottom + 2} onClose={onClose} />;
}

/** Korean titles show the access key after the name, as InDesign does (파일(F)); Latin titles
 * underline it in the name, as English InDesign does (File). */
function MenuTitle({ label, mnemonic }: { label: string; mnemonic: string }) {
  const at = label.toLowerCase().indexOf(mnemonic.toLowerCase());
  if (at < 0 || /[ᄀ-ᇿ㄰-㆏가-힯]/.test(label)) return <>{label}<span className="mn">(<u>{mnemonic}</u>)</span></>;
  return <>{label.slice(0, at)}<u>{label[at]}</u>{label.slice(at + 1)}</>;
}
