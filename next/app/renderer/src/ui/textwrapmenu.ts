// Text Wrap panel menu: Apply to Parent Page Only (IDML `ApplyToMasterPageOnly`): a parent page's object
// wraps text on the parent page but not on the pages that use it. InDesign offers it while a parent
// page is being edited, on an object that wraps text (nothing to apply to otherwise).
import { tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { PageItems } from '../inspect';
import type { Command } from './commands';
import { findItem } from './overlay';

type Props = {
  selection: { page: number; id: string; content: boolean }[];
  editable: boolean;
  /** A parent page is open (Pages panel: double-click a parent). */
  onParent: boolean;
  peek: (page: number) => PageItems | null | undefined;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};

export function textWrapMenu({ selection, editable, onParent, peek, edit }: Props): Record<string, Partial<Command>> {
  const one = selection.length === 1 && !selection[0].content ? selection[0] : null;
  const item = one ? findItem(peek(one.page), one.id)?.item : null;
  const on = !!item?.text_wrap?.master_only;
  return {
    'id.panel.textWrap.applyToParentPageOnly': {
      run: editable && onParent && item && item.text_wrap && !item.locked ? () => void edit(tr('텍스트 감싸기'), [{ op: 'textWrap', id: item.id, masterOnly: !on }]) : undefined,
      checked: on,
    },
  };
}
