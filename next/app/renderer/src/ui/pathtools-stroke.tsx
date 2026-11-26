//! Pointer strokes and overlays for the Pencil, Erase and Smooth tools.

//   Pencil: drag to draw a freehand line; the engine fits it as InDesign's Pencil does (pencilFit, edit/src/ops/pencilfit.rs; Fidelity and
//     Smoothness from the Pencil Tool Preferences). A drag that starts on a selected open path continues or redraws that path instead, and Alt
//     held when it ends closes the path, a new one too (pencilStroke, edit/src/ops/pencilextend.rs: the engine decides which, the screen sends
//     the stroke, the selection and the Alt key).
//   Erase: drag along a selected path to take that part out of it (the engine's erasePath, edit/src/ops/pencilerase.rs); a closed path is
//     opened there (one open path from the end of the gap round to its start), a path of a compound path keeps its pieces in the object.
//   Smooth: drag along a selected path to refit that part with fewer anchors (smoothPath, edit/src/ops/pencilsmooth.rs); a closed path
//     stays closed.

import { type Dispatch, type RefObject, type SetStateAction, useCallback } from 'react';
import type { EditCommand } from '../engine';
import { tr } from '../i18n';
import type { ItemInfo } from '../inspect';
import { findItem } from './overlay';
import { PENCIL_DEFAULTS } from './pencilprefs';
import type { Here, Options, PageId } from './pathtools-types';

/** A Pencil (or Erase) stroke being dragged: the page points the pointer went through. */
export type Pencil = { page: number; points: [number, number][]; tool: 'pencil' | 'erase' | 'smooth'; /** Alt was held when the pointer was let go */ alt?: boolean };
/** The Erase and Smooth tools' own preference (Fidelity in screen pixels) as installed; the Pencil's come from its preferences window (ui/pencilprefs.tsx). */
const PENCIL_FIDELITY = 2.5;

/** A shape the Erase and Smooth tools work on: a plain path, open or closed, a compound path too (no text frame, no frame with a graphic;
 *  the engine's erasePath / smoothPath, edit/src/ops/pencilerase.rs). */
const curvePath = (it: ItemInfo | undefined): boolean => !!it && !it.locked && !!it.paths?.length && it.kind !== 'textFrame' && !it.text && !it.content;

export function useStrokeTools(
  live: RefObject<Options>,
  pencil: Pencil | null,
  pencilRef: RefObject<Pencil | null>,
  setPencil: Dispatch<SetStateAction<Pencil | null>>,
  pageId: PageId,
) {
  /** The stroke goes to the engine, which edits the selected open path it started on or fits a new path; a created path is selected. */
  const finishPencil = useCallback(() => {
    const p = pencilRef.current;
    setPencil(null);
    pencilRef.current = null;
    if (!p || p.points.length < 2) return;
    const page = pageId(p.page);
    if (!page) return;
    const l = live.current;
    const black = l.info?.swatches.some((s) => s.id === 'Color/Black');
    const paint: EditCommand[] = black ? [{ op: 'setPaint', ids: ['@copies'], stroke: 'Color/Black', strokeWeight: 1 }] : [];
    // the selected open paths of the page: the stroke may continue one of them
    const items = l.store.peek(p.page);
    const selected = l.selection.filter((s) => s.page === p.page && !s.content).map((s) => s.id).filter((id) => {
      const item = findItem(items, id)?.item;
      return item && !item.locked && item.paths?.length === 1 && item.paths[0].open;
    });
    const prefs = l.pencil ?? PENCIL_DEFAULTS;
    const draw: EditCommand = { op: 'pencilStroke', page, points: p.points, fidelity: prefs.fidelity, smoothness: prefs.smoothness, px_per_pt: l.scale,
      within_px: prefs.within, alt: !!p.alt, selected: prefs.editSelected ? selected : [], layer: '' };
    void l.edit(tr('연필로 패스 만들기'), [draw, ...paint]).then((r) => {
      const id = r?.created?.[0];
      if (id) l.setSelection([{ page: p.page, id, content: false }]);
    });
  }, []);

  /** The erase (or smooth) stroke goes to the engine once per selected open path; the pieces that are left are selected (as InDesign leaves them). */
  const finishEraser = useCallback(() => {
    const p = pencilRef.current;
    setPencil(null);
    pencilRef.current = null;
    if (!p || p.points.length < 2) return;
    const page = pageId(p.page);
    if (!page) return;
    const l = live.current;
    const items = l.store.peek(p.page);
    const ids = l.selection.filter((s) => s.page === p.page && !s.content).map((s) => s.id).filter((id) => curvePath(findItem(items, id)?.item));
    if (!ids.length) return;
    const op = p.tool === 'smooth' ? 'smoothPath' : 'erasePath';
    const cmds: EditCommand[] = ids.map((id) => ({ op, id, page, points: p.points, fidelity: PENCIL_FIDELITY, px_per_pt: l.scale }));
    void l.edit(p.tool === 'smooth' ? tr('매끄럽게 도구로 다듬기') : tr('지우개로 지우기'), cmds).then((r) => {
      const made = r?.created ?? [];
      if (r) l.setSelection([...l.selection.filter((s) => s.page === p.page && !s.content), ...made.map((id) => ({ page: p.page, id, content: false }))]);
    });
  }, []);

  const down = (l: Options, e: React.PointerEvent, here: Here): boolean => {
    if (l.tool === 'pencil') {
      e.preventDefault();
      const layer = l.info?.layers.find((x) => x.id === l.info?.activeLayer);
      if (layer && (layer.locked || !layer.visible)) { l.note(tr('활성 레이어가 잠겼거나 숨겨져 있습니다.')); return true; }
      const next: Pencil = { page: here.index, points: [[here.x, here.y]], tool: 'pencil' };
      pencilRef.current = next; // events can come faster than the render that would set it
      setPencil(next);
      return true;
    }
    if (l.tool === 'erase' || l.tool === 'smooth') {
      e.preventDefault();
      // the Erase and Smooth tools work on the selected paths of the page (open or closed, a compound path too)
      const items = l.store.peek(here.index);
      const some = l.selection.some((s) => s.page === here.index && !s.content && curvePath(findItem(items, s.id)?.item));
      if (!some) { l.note(l.tool === 'smooth' ? tr('매끄럽게 도구는 선택한 패스에서 씁니다. 먼저 패스를 선택하세요.') : tr('지우개 도구는 선택한 패스에서 씁니다. 먼저 패스를 선택하세요.')); return true; }
      const next: Pencil = { page: here.index, points: [[here.x, here.y]], tool: l.tool };
      pencilRef.current = next;
      setPencil(next);
      return true;
    }
    return false;
  };

  const move = (l: Options, e: React.PointerEvent, here: Here | null): boolean => {
    const pc = pencilRef.current;
    if ((l.tool === 'pencil' || l.tool === 'erase' || l.tool === 'smooth') && pc) {
      // fast pointer movement arrives as one event with the positions in between: take them all, they are the stroke
      const merged = (e.nativeEvent as PointerEvent).getCoalescedEvents?.() ?? [];
      const spots = merged.length > 1 && l.pageAt ? merged.map((m) => l.pageAt!(m.clientX, m.clientY)) : [here];
      const points = [...pc.points];
      for (const h of spots) {
        if (!h || h.index !== pc.page) continue;
        const last = points[points.length - 1];
        if (h.x !== last[0] || h.y !== last[1]) points.push([h.x, h.y]);
      }
      if (points.length !== pc.points.length) {
        const next: Pencil = { ...pc, points };
        pencilRef.current = next;
        setPencil(next);
      }
      return true;
    }
    return false;
  };

  const up = (l: Options, e: React.PointerEvent, here: Here | null): boolean => {
    if ((l.tool === 'pencil' || l.tool === 'erase' || l.tool === 'smooth') && pencilRef.current) {
      const pc = pencilRef.current;
      // the pointer let go: its last place is the stroke's end, and Alt held now closes an edited path
      pencilRef.current = { ...pc, alt: e.altKey };
      if (here && here.index === pc.page) {
        const last = pc.points[pc.points.length - 1];
        if (here.x !== last[0] || here.y !== last[1]) pencilRef.current = { ...pencilRef.current, points: [...pc.points, [here.x, here.y]] };
      }
      if (pc.tool === 'pencil') finishPencil(); else finishEraser();
      return true;
    }
    return false;
  };

  const draw = (page: number, k: number, items: React.ReactNode[]) => {
    if (pencil && pencil.page === page && pencil.points.length > 1) {
      items.push(<path key="pencil" className="pen-path" d={`M${pencil.points.map((q) => `${q[0] * k},${q[1] * k}`).join('L')}`} />);
    }
  };

  return { down, move, up, draw };
}
