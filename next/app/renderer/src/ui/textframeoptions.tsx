// Object ▸ Text Frame Options (Ctrl+B) is the shared dialog `textFrameOptions` (next/shared/dialogs/textFrameOptions.json): the engine
// opens it from the first frame's options and sends each frame one `textFrameOptions` command with what changed (edit/src/ops/
// textframe.rs writes them as InDesign does and composes the story again). Here only which selected items it works on.
import type { PageItems, ItemInfo } from '../inspect';
import { type SelEntry, findItem } from './overlay';

/** The selected text frames (not locked). */
export function frameTargets(selection: SelEntry[], peek: (page: number) => PageItems | null | undefined): ItemInfo[] {
  return selection.filter((s) => !s.content).map((s) => findItem(peek(s.page), s.id)?.item)
    .filter((it): it is ItemInfo => !!it && it.kind === 'textFrame' && !it.locked && !!it.text?.options);
}
