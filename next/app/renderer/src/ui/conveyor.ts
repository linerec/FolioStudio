// Content Collector (B) and Content Placer (Shift+B) tools and the conveyor they share. The conveyor itself is the engine's
// (server `conveyor.rs`: collect, select, remove, clear, and placing is a Paste of an item); this file is the screen's side:
// a mirror of its list for the bar (`conveyor`), the actions that call the engine, and the tools' clicks.
//
// What is measured against InDesign: nothing (the earlier teams could not measure it: rules/viewmarks2.ko.md, tools3 STATUS).
// Decisions, from the user manual's description only: a click with the Collector puts the object under it (the group, not a
// member) at the end of the conveyor; a click with the Placer places the current item with its middle at the click and leaves
// it in the conveyor (it is a copy), unless "놓은 뒤 빼기" is on; the arrow keys choose the item; Esc ends the tool (App.tsx).
// Create Link / Map Styles (measured in InDesign 2025, oracle basic/cc-*): the placed object (and a text frame's story) is linked to its
// source in the same document, `linkPlaced` after the paste (folio-edit `ops/contentlink.rs`); a link to an object of another document is
// refused by the engine for now. The two options are a session setting of the tool, like "놓은 뒤 빼기" (screen only: conveyorLink,
// conveyorMapStyles; the request itself takes them as arguments).
import { conveyorCall, type Conveyor, type ConveyorItem, type EditCommand, type EditResult } from '../engine';
import { tr } from '../i18n';
import type { Hit, SelEntry } from './overlay';
import { Live } from './store';
import type { ToolId } from './tools';

/** The engine's conveyor as the screen last read it. */
export const conveyor = new Live<Conveyor>({ items: [], current: null });
/** Edit ▸ Place and Link: the cursor loaded from the selection (the engine's `loaded` slot); the next click on a page places it once,
 *  linked, whichever tool is current (measured in InDesign 2025 with real input, conveyor-c4). Esc empties it. */
export const loadedCursor = new Live<ConveyorItem | null>(null);
/** Remove an item from the conveyor after it was placed (the bar's check box; a session setting of the tool). */
export const removeAfterPlace = new Live<boolean>(false);
/** Create Link: the placed object is linked to its source. */
export const conveyorLink = new Live<boolean>(false);
/** Map Styles: the story link applies style mappings. */
export const conveyorMapStyles = new Live<boolean>(false);

// the screen redraws only when the engine's list changed (the bar asks again and again, see conveyorbar.tsx)
const keep = (c: Conveyor) => {
  const loaded = c.loaded ?? null;
  if ((loadedCursor.get()?.id ?? null) !== (loaded?.id ?? null)) loadedCursor.set(loaded);
  const now = conveyor.get();
  if (JSON.stringify([now.items, now.current]) !== JSON.stringify([c.items, c.current])) conveyor.set({ items: c.items, current: c.current });
  return c;
};

export const conveyorActions = {
  refresh: () => conveyorCall({ action: 'state' }).then(keep),
  collect: (ids: string[], together = false) => conveyorCall({ action: 'collect', ids, ...(together ? { together } : {}) }).then(keep),
  select: (index: number) => conveyorCall({ action: 'select', index }).then(keep),
  step: (step: number) => conveyorCall({ action: 'select', step }).then(keep),
  remove: (item?: number) => conveyorCall({ action: 'remove', ...(item === undefined ? {} : { item }) }).then(keep),
  clear: () => conveyorCall({ action: 'clear' }).then(keep),
  /** Edit ▸ Place and Link: the objects load the cursor as one set. */
  load: (ids: string[]) => conveyorCall({ action: 'load', ids }).then(keep),
  unload: () => conveyorCall({ action: 'unload' }).then(keep),
};

export const isConveyorTool = (tool: ToolId) => tool === 'collector' || tool === 'placer';

/** The arrow keys with the Placer choose the item placed next: left and up the one before, right and down the one after. */
export function conveyorKey(e: KeyboardEvent, tool: ToolId): boolean {
  if (tool !== 'placer' || e.ctrlKey || e.metaKey || e.altKey || e.shiftKey) return false;
  const before = e.key === 'ArrowLeft' || e.key === 'ArrowUp', after = e.key === 'ArrowRight' || e.key === 'ArrowDown';
  if (!before && !after) return false;
  e.preventDefault();
  void conveyorActions.step(before ? -1 : 1).catch(() => undefined);
  return true;
}

type Props = {
  tool: ToolId;
  editable: boolean;
  /** The `Self` of the page with this index. */
  pageId: (page: number) => string | undefined;
  edit: (label: string, commands: EditCommand[], extra?: Record<string, unknown>) => Promise<EditResult | null>;
  setSelection: (selection: SelEntry[]) => void;
  note: (message: string) => void;
};

export function useConveyorTools(p: Props) {
  const fail = (e: unknown) => p.note(String((e as Error).message ?? e));
  const collect = (hit: Hit | null) => {
    if (!hit) { p.note(tr('담을 개체를 누르세요')); return; }
    conveyorActions.collect([hit.top.id]).then((c) => p.note(tr('컨베이어에 담았습니다 · {0}개', { 0: c.items.length }))).catch(fail);
  };
  /** The click that places the Place and Link cursor: the engine puts the top left corner of the visible bounds at the point and links. */
  const placeLoaded = async (here: { index: number; x: number; y: number }) => {
    if (!p.editable) { p.note(tr('읽기 전용 문서에는 놓을 수 없습니다')); return; }
    const page = p.pageId(here.index);
    if (!page) return;
    const date = new Date().toISOString().slice(0, 19);
    const done = await p.edit(tr('배치 및 연결'), [{ op: 'placeAndLink', page, at: [here.x, here.y], map_styles: true, date }], { conveyor: { loaded: true } });
    if (!done) return; // refused (a locked layer ...): the cursor stays loaded
    await conveyorActions.refresh().catch(() => loadedCursor.set(null));
    if (done.created?.length) p.setSelection(done.created.map((id) => ({ page: here.index, id, content: false })));
  };
  const place = async (here: { index: number; x: number; y: number }) => {
    // the engine's list, not the screen's copy: another window or a scripted request may have changed it
    const c = await conveyorActions.refresh().catch(() => conveyor.get());
    const item = c.current === null ? undefined : c.items[c.current];
    if (!item) { p.note(tr('컨베이어가 비어 있습니다. 내용 수집 도구(B)로 먼저 담으세요')); return; }
    const page = p.pageId(here.index);
    if (!page) return;
    const link: EditCommand[] = conveyorLink.get() ? [{ op: 'linkPlaced', map_styles: conveyorMapStyles.get(), date: new Date().toISOString().slice(0, 19) }] : [];
    const done = await p.edit(tr('컨베이어에서 놓기'), [{ op: 'paste', page, at: [here.x, here.y] }, ...link], { conveyor: { item: item.id } });
    if (!done) return;
    if (done.created?.length) p.setSelection(done.created.map((id) => ({ page: here.index, id, content: false })));
    if (removeAfterPlace.get()) await conveyorActions.remove(item.id).catch(fail);
  };
  return {
    placeLoaded,
    /** A press with the Collector or Placer; false: not one of the tools. */
    down: (e: React.PointerEvent, here: { index: number; x: number; y: number } | null, hit: Hit | null): boolean => {
      if (p.tool !== 'collector' && p.tool !== 'placer') return false;
      e.preventDefault();
      if (p.tool === 'collector') collect(hit);
      else if (!p.editable) p.note(tr('읽기 전용 문서에는 놓을 수 없습니다'));
      else if (here) void place(here);
      return true;
    },
  };
}
