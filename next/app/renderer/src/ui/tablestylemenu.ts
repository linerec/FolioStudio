// Cell Styles and Table Styles panel menus (the ≡ menus) and the rows' right-click menus: the open panel publishes what the menu items do (it owns the
// selected style, the shut groups and the dialogs); this registry hands them to the commands under the ids of menutree.ts (`id.panel.tableStyles.*`,
// `id.panel.cellStyles.*`, and `id.ctx.styleTable.*` / `id.ctx.styleCell.*` for the items only the right-click menu has).
import { useCallback, useMemo, useRef, useState } from 'react';
import type { Command } from './commands';
import type { RowAction } from './stylecontext';

export type TableStyleKind = 'cell' | 'table';
export type TableStyleAction = { id: string; disabled: boolean; checked?: boolean; run: () => unknown };

const COMMON: Record<string, string> = {
  duplicate: 'duplicateStyle', delete: 'deleteStyle', redefine: 'redefineStyle', options: 'styleOptions', clear: 'clearOverrides', breakLink: 'breakLinkToStyle',
  loadBoth: 'loadTableAndCellStyles', selectUnused: 'selectAllUnused', newGroup: 'newStyleGroup', openAll: 'openAllStyleGroups',
  closeAll: 'closeAllStyleGroups', copyToGroup: 'copyToGroup', newGroupFromStyles: 'newGroupFromStyles', sort: 'sortByName', small: 'smallPanelRows',
};
/** The menu commands of each panel by the panel's action names. */
const IDS: Record<TableStyleKind, Record<string, string>> = {
  cell: {
    ...Object.fromEntries(Object.entries(COMMON).map(([k, v]) => [k, 'id.panel.cellStyles.' + v])), new: 'id.panel.cellStyles.newCellStyle', load: 'id.panel.cellStyles.loadCellStyles', clearNotDefined: 'id.panel.cellStyles.clearAttributesNotDefinedByStyle',
    apply: 'id.ctx.styleCell.applyStyle', applyAll: 'id.ctx.styleCell.applyStyleClearAll', groupOptions: 'id.ctx.styleCell.styleGroupOptions', deleteGroup: 'id.ctx.styleCell.deleteStyleGroup', duplicateGroup: 'id.ctx.styleCell.duplicateStyleGroup',
  },
  table: {
    ...Object.fromEntries(Object.entries(COMMON).map(([k, v]) => [k, 'id.panel.tableStyles.' + v])), new: 'id.panel.tableStyles.newTableStyle', load: 'id.panel.tableStyles.loadTableStyles',
    apply: 'id.ctx.styleTable.applyStyle', applyClear: 'id.ctx.styleTable.applyStyleClearOverrides', applyClearCells: 'id.ctx.styleTable.applyStyleClearCellStyles',
    applyAll: 'id.ctx.styleTable.applyStyleClearAll', groupOptions: 'id.ctx.styleTable.styleGroupOptions', deleteGroup: 'id.ctx.styleTable.deleteStyleGroup', duplicateGroup: 'id.ctx.styleTable.duplicateStyleGroup',
  },
};

/** What the right-click menu's commands do: the open panel's actions by the command ids of the menu (stylecontext.tsx). */
export function tableStyleRowActions(kind: TableStyleKind, actions: TableStyleAction[]): Record<string, RowAction> {
  const out: Record<string, RowAction> = {};
  for (const a of actions) if (IDS[kind][a.id]) out[IDS[kind][a.id]] = { disabled: a.disabled, checked: a.checked, run: a.run };
  return out;
}

export function useTableStyleMenu() {
  const current = useRef<Record<TableStyleKind, TableStyleAction[]>>({ cell: [], table: [] });
  const signature = useRef<Record<TableStyleKind, string>>({ cell: '', table: '' });
  const [revision, setRevision] = useState(0);
  const publish = useCallback((kind: TableStyleKind, actions: TableStyleAction[]) => {
    current.current[kind] = actions;
    const next = JSON.stringify(actions.map((a) => [a.id, a.disabled, a.checked ?? null]));
    if (next !== signature.current[kind]) { signature.current[kind] = next; setRevision((r) => r + 1); }
  }, []);
  const handlers = useMemo(() => {
    const out: Record<string, Partial<Command>> = {};
    for (const kind of ['cell', 'table'] as const) {
      for (const [key, id] of Object.entries(IDS[kind])) {
        const action = current.current[kind].find((a) => a.id === key);
        out[id] = {
          enabled: !!action && !action.disabled,
          ...(action?.checked !== undefined ? { checked: action.checked } : {}),
          run: () => { const a = current.current[kind].find((x) => x.id === key); if (a && !a.disabled) a.run(); },
        };
      }
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision]);
  return { publish, handlers };
}
