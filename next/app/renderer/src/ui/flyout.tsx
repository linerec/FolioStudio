// The panel menu button (☰) of a window that is not in the dock: the Book and Library windows carry InDesign's panel menus (menutree.ts PANEL_MENUS
// book / library) as a menu opened by this button. `getCommands` is read while this renders, so the entries are the live commands.
import { useState } from 'react';
import { tr } from '../i18n';
import type { Commands } from './commands';
import { Icon } from './icons';
import type { MenuEntry } from './menudef';
import { PopupMenu } from './menu';

export function Flyout({ items, getCommands, label }: { items: MenuEntry[]; getCommands: () => Commands; label?: string }) {
  const [at, setAt] = useState<DOMRect | null>(null);
  const title = label ?? tr('패널 메뉴');
  return (
    <>
      <button type="button" className="dpanel-menu" aria-label={title} title={title} onClick={(e) => setAt(e.currentTarget.getBoundingClientRect())}><Icon name="menu" size={14} /></button>
      {at && <PopupMenu items={items} commands={getCommands()} anchor={at} onClose={() => setAt(null)} />}
    </>
  );
}
