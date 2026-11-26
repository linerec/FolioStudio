// Paragraph Styles and Character Styles panel menus (the ≡ menus): the open panel publishes what its menu items do (it owns the
// selected style, the shut groups and the dialogs); this registry hands them to the commands under InDesign's ids (menutree.ts).
import { useCallback, useMemo, useRef, useState } from 'react';
import type { Command } from './commands';
import type { RowAction } from './stylecontext';

export type TextStyleKind = 'paragraph' | 'character';
export type TextStyleAction = { id: string; disabled: boolean; checked?: boolean; run: () => unknown };

/** The panel's action names and the menu ids they answer. */
const REGISTRY: Record<TextStyleKind, Record<string, string>> = {
  paragraph: {
    new: 'newParagraphStyle', duplicate: 'duplicateStyle', newGroup: 'newStyleGroup', newGroupFromStyles: 'newGroupFromStyles', delete: 'deleteStyles', redefine: 'redefineStyle', options: 'styleOptions', clear: 'clearOverrides', breakLink: 'breakLinkToStyle',
    openAll: 'openAllStyleGroups', closeAll: 'closeAllStyleGroups', sortByName: 'sortByName', smallRows: 'smallPanelRows',
    copyToGroup: 'copyToGroup', load: 'loadParagraphStyles', loadAll: 'loadAllTextStyles',
    selectAllUnused: 'selectAllUnused', highlight: 'toggleStyleOverrideHighlighter',
    convertLists: 'convertStyleSBulletsAndNumberingToText',
  },
  character: {
    new: 'newCharacterStyle', duplicate: 'duplicateStyle', newGroup: 'newStyleGroup', newGroupFromStyles: 'newGroupFromStyles', delete: 'deleteStyle', redefine: 'redefineStyle', options: 'styleOptions', breakLink: 'breakLinkToStyle',
    openAll: 'openAllStyleGroups', closeAll: 'closeAllStyleGroups', sortByName: 'sortByName', smallRows: 'smallPanelRows',
    copyToGroup: 'copyToGroup', load: 'loadCharacterStyles', loadAll: 'loadAllTextStyles',
    selectAllUnused: 'selectAllUnused', highlight: 'toggleStyleOverrideHighlighter',
  },
};
const PREFIX: Record<TextStyleKind, string> = { paragraph: 'id.panel.paragraphStyles.', character: 'id.panel.characterStyles.' };
/** The commands only the row's right-click menu has (one id for both panels; whichever panel is open answers it). */
const CONTEXT: Record<string, string> = {
  apply: 'id.ctx.styleParagraph.apply', applyClear: 'id.ctx.styleParagraph.applyClearOverrides',
  groupOptions: 'id.ctx.styleParagraph.styleGroupOptions', deleteGroup: 'id.ctx.styleParagraph.deleteGroup',
  // Apply and clear the character styles / everything, then-next-style variants, Duplicate Group (menurest-c1)
  applyClearChars: 'id.ctx.styleParagraph.applyClearCharacterStyles', applyClearAll: 'id.ctx.styleParagraph.applyClearAll',
  applyNext: 'id.ctx.styleParagraph.applyThenNextStyle', applyNextClear: 'id.ctx.styleParagraph.applyThenNextStyleClearOverrides',
  applyNextClearChars: 'id.ctx.styleParagraph.applyThenNextStyleClearCharacterStyles', applyNextClearAll: 'id.ctx.styleParagraph.applyThenNextStyleClearAll',
  duplicateGroup: 'id.ctx.styleParagraph.duplicateGroup',
};
/** What the right-click menu's commands do: the panel's action names by the menu's command ids (both panels use the paragraph panel's ids there). */
const CONTEXT_MENU_IDS: Record<string, string> = {
  options: 'id.panel.paragraphStyles.styleOptions', duplicate: 'id.panel.paragraphStyles.duplicateStyle', delete: 'id.panel.paragraphStyles.deleteStyles',
  redefine: 'id.panel.paragraphStyles.redefineStyle', convertLists: 'id.panel.paragraphStyles.convertStyleSBulletsAndNumberingToText',
  new: 'id.panel.paragraphStyles.newParagraphStyle', copyToGroup: 'id.panel.paragraphStyles.copyToGroup', newGroupFromStyles: 'id.panel.paragraphStyles.newGroupFromStyles',
  ...CONTEXT,
};

/** The row's right-click menu for the open panel: its actions by the menu's command ids (stylecontext.tsx). */
export function textStyleRowActions(actions: TextStyleAction[]): Record<string, RowAction> {
  return Object.fromEntries(Object.entries(CONTEXT_MENU_IDS).flatMap(([key, id]) => {
    const a = actions.find((x) => x.id === key);
    return a ? [[id, { disabled: a.disabled, run: a.run }] as const] : [];
  }));
}

export function useTextStyleMenu() {
  const current = useRef<Record<TextStyleKind, TextStyleAction[]>>({ paragraph: [], character: [] });
  const signature = useRef<Record<TextStyleKind, string>>({ paragraph: '', character: '' });
  const [revision, setRevision] = useState(0);
  const publish = useCallback((kind: TextStyleKind, actions: TextStyleAction[]) => {
    current.current[kind] = actions;
    const next = JSON.stringify(actions.map((a) => [a.id, a.disabled, a.checked ?? null]));
    if (next !== signature.current[kind]) { signature.current[kind] = next; setRevision((r) => r + 1); }
  }, []);
  const handlers = useMemo(() => {
    const out: Record<string, Partial<Command>> = {};
    for (const kind of ['paragraph', 'character'] as const) {
      for (const [key, command] of Object.entries(REGISTRY[kind])) {
        const action = current.current[kind].find((a) => a.id === key);
        // Load All Text Styles is one menu item (under the paragraph panel's id) of both panels: whichever panel is open answers it
        const id = key === 'loadAll' ? PREFIX.paragraph + command : PREFIX[kind] + command;
        if (key === 'loadAll' && out[id]?.enabled) continue;
        out[id] = {
          enabled: !!action && !action.disabled,
          ...(action?.checked !== undefined ? { checked: action.checked } : {}),
          run: () => { const a = current.current[kind].find((x) => x.id === key); if (a && !a.disabled) a.run(); },
        };
      }
    }
    // the right-click menu's own commands: whichever panel is open answers them
    for (const kind of ['paragraph', 'character'] as const) {
      for (const [key, id] of Object.entries(CONTEXT)) {
        if (out[id]?.enabled) continue;
        const action = current.current[kind].find((a) => a.id === key);
        out[id] = {
          enabled: !!action && !action.disabled,
          run: () => { const a = current.current[kind].find((x) => x.id === key); if (a && !a.disabled) a.run(); },
        };
      }
    }
    return out;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision]);
  return { publish, handlers };
}
