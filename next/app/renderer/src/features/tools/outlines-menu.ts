// Type ▸ Create Outlines. The engine does the work (edit/src/ops/outlines.rs: `createOutlines` on whole text frames,
// `createOutlinesRange` on a selected text range; the host measures the glyph shapes, so both commands carry
// `shapes: ''`); this decides when InDesign offers it: unlocked text frames with text (a threaded one takes its whole story), selected as objects,
// or a selected range of characters (not inside a table cell or a note).
import { tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { ItemInfo, PageItems } from '../../inspect';
import type { Command } from '../../ui/commands';
import { findItem } from '../../ui/overlay';

type Props = {
  selection: { page: number; id: string; content: boolean }[];
  editable: boolean;
  /** A text caret or range is active: Create Outlines would act on that range. */
  inText: boolean;
  /** The selected text range (story characters [start, end)), when there is one. */
  range?: { story: string; start: number; end: number; nested?: boolean } | null;
  peek: (page: number) => PageItems | null | undefined;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};

export function useOutlinesMenu({ selection, editable, inText, range, peek, edit }: Props): Record<string, Partial<Command>> {
  const objects = selection.filter((s) => !s.content);
  const ids = objects.map((s) => s.id);
  const items = objects.map((s) => findItem(peek(s.page), s.id)?.item).filter((x): x is ItemInfo => !!x);
  const ok = editable && !inText && items.length > 0 && items.length === objects.length
    && items.every((it) => it.kind === 'textFrame' && !it.locked && !!it.text && !it.text.empty);
  const text = editable && !!range && range.end > range.start && !range.nested;
  return {
    'type.createOutlines': {
      run: text && range ? () => void edit(tr('윤곽선 만들기'), [{ op: 'createOutlinesRange', story: range.story, start: range.start, end: range.end, shapes: '' }])
        : ok ? () => void edit(tr('윤곽선 만들기'), [{ op: 'createOutlines', ids, shapes: '' }]) : undefined,
    },
    // Create Outlines without Deleting Text (Ctrl+Alt+Shift+O, tools-c1/c2): the engine's createOutlinesKeepText puts the outlines in front of
    // the frames and keeps the text frames under them; one edit, so one Undo (the host measures the glyphs as for Create Outlines)
    'type.key.createOutlinesKeepText': {
      run: ok ? () => void edit(tr('윤곽선 만들기(글 남김)'), [{ op: 'createOutlinesKeepText', ids, shapes: '' }]) : undefined,
    },
  };
}
