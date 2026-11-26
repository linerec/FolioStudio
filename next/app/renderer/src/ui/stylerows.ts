import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';
// The rows of a style panel that lists styles in groups (Object Styles: the groups come from the styles' own group paths),
// and a flag kept for this computer only (a panel menu setting).

type Listed = { id: string; name: string; group: string[] };
export type StyleRow<S extends Listed> =
  | { group: true; key: string; name: string; depth: number; open: boolean }
  | { group: false; style: S; name: string; depth: number };

/** The key of a group: its names joined (a name has no "/"). */
export const groupKey = (path: string[]) => path.join('/');

/** The styles of a group, then its groups (an open group with its rows under it); `closed` holds the keys of the shut groups. */
export function styleTree<S extends Listed>(styles: S[], closed: Set<string>, listed: string[][] = []): { rows: StyleRow<S>[]; groups: string[] } {
  const rows: StyleRow<S>[] = [], groups: string[] = [];
  // the groups the document lists (empty ones too), and the groups the styles say they are in
  const paths = [...listed, ...styles.map((s) => s.group)];
  const level = (parent: string[], shown: boolean) => {
    const depth = parent.length;
    for (const s of styles) if (shown && s.group.length === depth && s.group.every((n, i) => n === parent[i])) rows.push({ group: false, style: s, name: s.name, depth });
    const names: string[] = [];
    for (const p of paths) if (p.length > depth && parent.every((n, i) => n === p[i]) && !names.includes(p[depth])) names.push(p[depth]);
    for (const name of names) {
      const path = [...parent, name], key = groupKey(path), open = !closed.has(key);
      groups.push(key);
      if (shown) rows.push({ group: true, key, name, depth, open });
      level(path, shown && open);
    }
  };
  level([], true);
  return { rows, groups };
}

/** A true/false setting of this computer (a panel menu check), kept in the app's own storage, never in the document. */
export function useStoredFlag(key: string): [boolean, (v: boolean) => void] {
  return [useUserPreference(key) === '1', (v) => userSettings.setItem(key, v ? '1' : '0')];
}
