// Cell Styles / Table Styles panels: the list as a tree of style groups, and the small dialogs of the panel menu (a name for a new group, a
// rename or a copy; the group to move a style into). The engine writes the groups as InDesign does (edit/src/ops/tablestylegroups.rs, oracle
// tablestyles cs-group-*, cs-duplicate).
import { t as tr } from '../i18n';
import { type ReactNode, useState } from 'react';
import { createPortal } from 'react-dom';
import type { StyleGroupInfo, TableStyleInfo } from '../inspect';

export const groupName = (g: StyleGroupInfo) => g.path[g.path.length - 1];
const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);
const startsWith = (a: string[], b: string[]) => b.length <= a.length && b.every((x, i) => x === a[i]);

/** The groups with no style or group in them (the only ones the engine deletes). */
export function emptyGroup(group: StyleGroupInfo, groups: StyleGroupInfo[], styles: TableStyleInfo[]): boolean {
  return !styles.some((s) => startsWith(s.group ?? [], group.path)) && !groups.some((g) => g.id !== group.id && startsWith(g.path, group.path));
}

/** The rows of a panel: the styles of a group, then its groups (open ones with their rows under them). `closed` holds the group ids shut. */
export function styleRows(styles: TableStyleInfo[], groups: StyleGroupInfo[], closed: Set<string>, row: (s: TableStyleInfo, depth: number) => ReactNode,
  head: (g: StyleGroupInfo, depth: number, open: boolean) => ReactNode, parent: string[] = [], depth = 0): ReactNode[] {
  const out: ReactNode[] = styles.filter((s) => same(s.group ?? [], parent)).map((s) => row(s, depth));
  for (const g of groups.filter((x) => x.path.length === parent.length + 1 && startsWith(x.path, parent))) {
    const open = !closed.has(g.id);
    out.push(head(g, depth, open));
    if (open) out.push(...styleRows(styles, groups, closed, row, head, g.path, depth + 1));
  }
  return out;
}

/** A name to type: a new group, a group's new name, a copy's name. */
export function NameDialog({ title, label, initial, taken, onOk, close }: {
  title: string; label: string; initial: string; taken: (name: string) => boolean; onOk: (name: string) => void; close: () => void;
}) {
  const [name, setName] = useState(initial);
  const bad = name.trim() === '' || /[/:]/.test(name);
  const dup = !bad && taken(name.trim());
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={title} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); if (!bad && !dup) { onOk(name.trim()); close(); } }}>
        <h2>{title}</h2>
        <label>{label}<input aria-label={label} autoFocus value={name} onChange={(e) => setName(e.target.value)} /></label>
        {dup && <p className="field-error" role="alert">{tr('같은 이름이 이미 있습니다.')}</p>}
        {/[/:]/.test(name) && <p className="field-error" role="alert">{tr('이름에는 / 와 : 를 쓸 수 없습니다.')}</p>}
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={bad || dup}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}

/** The group to move a style into (the top level, or one of the groups). */
export function MoveDialog({ groups, current, onOk, close }: { groups: StyleGroupInfo[]; current: string[]; onOk: (group: string) => void; close: () => void }) {
  const [group, setGroup] = useState('');
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('스타일 그룹으로 옮기기')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); onOk(group); close(); }}>
        <h2>{tr('스타일 그룹으로 옮기기')}</h2>
        <label>{tr('옮길 그룹:')}
          <select aria-label={tr('옮길 그룹')} value={group} onChange={(e) => setGroup(e.target.value)}>
            <option value="">{tr('(최상위)')}</option>
            {groups.filter((g) => !same(g.path, current)).map((g) => <option key={g.id} value={g.id}>{g.path.join(' / ')}</option>)}
          </select></label>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit">{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
