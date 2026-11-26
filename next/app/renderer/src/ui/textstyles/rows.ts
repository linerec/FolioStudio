// Paragraph Styles and Character Styles panels: the list as a tree of style groups. A group is named by the group names from the top
// (`Heads%3aSub`); its open or shut state is the panel's, kept by that key.
export type TextStyleEntry = { id: string; name: string; group: string[] };
export type TextStyleRow =
  | { type: 'style'; style: TextStyleEntry; depth: number }
  | { type: 'group'; key: string; name: string; open: boolean; depth: number };

const same = (a: string[], b: string[]) => a.length === b.length && a.every((x, i) => x === b[i]);
const startsWith = (a: string[], b: string[]) => b.length <= a.length && b.every((x, i) => x === a[i]);

export const groupKey = (path: string[]) => path.join('%3a');

/** Every group there is, by its path from the top: the groups of the document (empty ones too) and those the styles are in. */
export function groupPaths(list: TextStyleEntry[], groups: { path: string[] }[] = []): string[][] {
  const seen = new Set<string>();
  const out: string[][] = [];
  const add = (path: string[]) => {
    for (let n = 1; n <= path.length; n++) {
      const part = path.slice(0, n), key = groupKey(part);
      if (!seen.has(key)) { seen.add(key); out.push(part); }
    }
  };
  groups.forEach((g) => add(g.path));
  list.forEach((s) => add(s.group));
  return out;
}

/** The rows of the panel: the styles of a group, then its groups (an open one with its rows under it). `closed` holds the keys shut; `sorted`:
 *  by name, the built-in styles first (Sort by Name changes the panel's order only: the document's stays, tstyle-sort-paragraph). */
export function textStyleRows(list: TextStyleEntry[], paths: string[][], closed: Set<string>, sorted = false, parent: string[] = [], depth = 0): TextStyleRow[] {
  const styles = list.filter((s) => same(s.group, parent));
  const order = <T,>(items: T[], name: (x: T) => string, builtIn: (x: T) => boolean) =>
    sorted ? [...items].sort((a, b) => Number(builtIn(b)) - Number(builtIn(a)) || name(a).localeCompare(name(b))) : items;
  const rows: TextStyleRow[] = order(styles, (s) => s.name, (s) => s.id.includes('/$ID/')).map((style) => ({ type: 'style' as const, style, depth }));
  const children = paths.filter((p) => p.length === parent.length + 1 && startsWith(p, parent));
  for (const path of order(children, (p) => p[p.length - 1], () => false)) {
    const key = groupKey(path);
    const open = !closed.has(key);
    rows.push({ type: 'group', key, name: path[path.length - 1], open, depth });
    if (open) rows.push(...textStyleRows(list, paths, closed, sorted, path, depth + 1));
  }
  return rows;
}
