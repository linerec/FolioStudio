// Layout ▸ Go Back / Go Forward: the pages a person has looked at, as InDesign keeps them. A page
// joins the history once the view rests on it for a moment, so scrolling past pages does not fill
// it; going back or forward moves along the history without adding to it.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

const SETTLE_MS = 400;
const LIMIT = 100;
/** How long a Go Back / Go Forward scroll may take before the history takes the page it reached. */
const JUMP_MS = 1500;

type History = { back: number[]; forward: number[]; at: number | null };
const EMPTY: History = { back: [], forward: [], at: null };

export type PageHistory = { canBack: boolean; canForward: boolean; go: (d: -1 | 1) => void };

/** `docKey` names the open document (the history starts over with another one). */
export function usePageHistory(docKey: string | null, current: number, pages: number, scrollToPage: (i: number) => void): PageHistory {
  const [h, setH] = useState<History>(EMPTY);
  const jump = useRef<{ page: number; until: number } | null>(null);
  useEffect(() => { setH(EMPTY); jump.current = null; }, [docKey]);
  useEffect(() => {
    if (docKey === null) return;
    const t = setTimeout(() => {
      const j = jump.current;
      if (j) {
        // a jump in progress: wait until it arrives (or gives up), then stand on the page it reached
        if (current !== j.page && performance.now() < j.until) return;
        jump.current = null;
        setH((o) => (o.at === current ? o : { ...o, at: current }));
        return;
      }
      setH((o) => (o.at === current ? o : { back: o.at === null ? o.back : [...o.back, o.at].slice(-LIMIT), forward: [], at: current }));
    }, SETTLE_MS);
    return () => clearTimeout(t);
  }, [docKey, current]);
  // the page shown now counts even before it has settled
  const now = (): History => (h.at === current || jump.current || h.at === null ? h : { back: [...h.back, h.at].slice(-LIMIT), forward: [], at: current });
  const go = useCallback((d: -1 | 1) => {
    const o = now();
    const from = d < 0 ? o.back : o.forward;
    if (!from.length || !pages) return;
    const page = Math.min(from[from.length - 1], pages - 1);
    const rest = from.slice(0, -1);
    const other = o.at === null ? (d < 0 ? o.forward : o.back) : [...(d < 0 ? o.forward : o.back), o.at];
    setH(d < 0 ? { back: rest, forward: other, at: page } : { back: other, forward: rest, at: page });
    jump.current = { page, until: performance.now() + JUMP_MS };
    scrollToPage(page);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [h, current, pages, scrollToPage]);
  const o = now();
  const canBack = o.back.length > 0, canForward = o.forward.length > 0;
  return useMemo(() => ({ canBack, canForward, go }), [canBack, canForward, go]);
}
