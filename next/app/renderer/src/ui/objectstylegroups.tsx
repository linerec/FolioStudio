// Object Styles panel menu: the small windows of the style groups (a name for a new group; the group to copy a style into). The engine writes
// the groups as InDesign does (edit/src/ops/objectstyledefs/groups.rs, oracle objectstyles os-group-*).
import { createPortal } from 'react-dom';
import { useState } from 'react';
import { t as tr } from '../i18n';
import type { StyleGroupInfo } from '../inspect';

/** The `Self` of the group for a path of names (the group the document lists, else the id InDesign would give it). */
export function groupIdOf(path: string[], groups: StyleGroupInfo[]): string {
  return groups.find((g) => g.path.length === path.length && g.path.every((n, i) => n === path[i]))?.id ?? `ObjectStyleGroup/$ID/${path.join('%3a')}`;
}

/** The group to copy a style into (one of the groups of the document). */
export function GroupPickDialog({ title, groups, onOk, close }: { title: string; groups: StyleGroupInfo[]; onOk: (group: string) => void; close: () => void }) {
  const [group, setGroup] = useState(groups[0]?.id ?? '');
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={title} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); if (group) { onOk(group); close(); } }}>
        <h2>{title}</h2>
        <label>{tr('그룹:')}
          <select aria-label={tr('그룹')} value={group} onChange={(e) => setGroup(e.target.value)}>
            {groups.map((g) => <option key={g.id} value={g.id}>{g.path.join(' / ')}</option>)}
          </select></label>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!group}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
