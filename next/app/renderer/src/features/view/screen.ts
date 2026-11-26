import type { DocumentSummary, PageInfo } from '../../engine';
import type { PageSide } from '../../inspect';
import type { ScreenMode } from '../../ui/tools';
import { physicalMargins } from '../../inspect';

export const SCREEN_MODES = ['normal', 'preview', 'bleed', 'slug'] as const;

/** The outside of a spread grows; the join between its pages stays at the trim edge. */
export function screenRegion(mode: ScreenMode, page: PageInfo, doc: Pick<DocumentSummary, 'bleed' | 'slug'>, side: PageSide | undefined, first: boolean, last: boolean): number[] | null {
  if (mode !== 'bleed' && mode !== 'slug') return null;
  const extent = (key: 'top' | 'bottom' | 'inside' | 'outside') => Math.max(0, doc.bleed?.[key] ?? 0, mode === 'slug' ? doc.slug?.[key] ?? 0 : 0);
  const [top, left, bottom, right] = physicalMargins([extent('top'), extent('inside'), extent('bottom'), extent('outside')], side);
  return [first ? -left : 0, -top, page.width + (last ? right : 0), page.height + bottom];
}
