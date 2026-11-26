// Object ▸ Effects ▸ Clear Effects / Clear All Transparency, Object ▸ Transform ▸ Clear
// Transformations, Object ▸ Content ▸ Graphic / Unassigned. The rules are in the engine
// (edit/src/ops/clear.rs, oracle basic-object-menu2); this decides when InDesign offers them.
import { useEffect, useState } from 'react';
import { tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { ItemInfo, PageItems } from '../inspect';
import type { Command } from './commands';
import { findItem } from './overlay';

type Sel = { page: number; id: string; content: boolean };
type Effects = { levels: Record<string, { opacity: number; mode: string; effects: Record<string, { on: boolean }> }> };

type Props = {
  selection: Sel[];
  editable: boolean;
  /** Changes after every edit, so the effects are read again. */
  revision: unknown;
  peek: (page: number) => PageItems | null | undefined;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};

const frames = new Set(['rectangle', 'oval', 'polygon']);
const identity = (m: ItemInfo['transform']) => Math.abs(m[0] - 1) < 1e-9 && Math.abs(m[1]) < 1e-9 && Math.abs(m[2]) < 1e-9 && Math.abs(m[3] - 1) < 1e-9;

export function useObjectClear({ selection, editable, revision, peek, edit }: Props): Record<string, Partial<Command>> {
  const objects = selection.filter((s) => !s.content);
  const ids = objects.map((s) => s.id);
  const key = ids.join(' ');
  // what the selection has: object-level effects (Clear Effects), any transparency (Clear All)
  const [has, setHas] = useState({ effects: false, transparency: false });
  useEffect(() => {
    let live = true;
    if (!editable || !ids.length) { setHas({ effects: false, transparency: false }); return; }
    void Promise.all(ids.map((id) => window.folio.request({ cmd: 'effects', item: id }).then(({ header }) => (header.ok ? header.effects as Effects : null))))
      .then((all) => {
        if (!live) return;
        const on = (l: Effects['levels'][string] | undefined) => Object.values(l?.effects ?? {}).some((e) => e.on);
        setHas({
          effects: all.some((v) => on(v?.levels.object)),
          transparency: all.some((v) => Object.values(v?.levels ?? {}).some((l) => l.opacity !== 100 || l.mode !== 'Normal' || on(l))),
        });
      }, () => { if (live) setHas({ effects: false, transparency: false }); });
    return () => { live = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, editable, revision]);
  const items = objects.map((s) => findItem(peek(s.page), s.id)?.item).filter((x): x is ItemInfo => !!x);
  const all = editable && items.length > 0 && items.length === objects.length;
  // InDesign: only an empty frame's content type changes (not one with text or a graphic): an
  // empty shape, or a text frame without a character (not threaded)
  const shape = (it: ItemInfo) => frames.has(it.kind) && !it.content && !it.text && !it.children.length;
  const emptyText = (it: ItemInfo) => it.kind === 'textFrame' && !!it.text?.empty;
  const empty = all && items.every((it) => shape(it) || emptyText(it));
  const toText = all && items.every(shape);
  return {
    'id.object.effects.clearEffects': { run: all && has.effects ? () => void edit(tr('효과 지우기'), [{ op: 'clearEffects', ids }]) : undefined },
    'id.object.effects.clearAllTransparency': { run: all && has.transparency ? () => void edit(tr('모든 투명도 지우기'), [{ op: 'clearAllTransparency', ids }]) : undefined },
    'id.object.transform.clearTransformations': { run: all && items.some((it) => !identity(it.transform)) ? () => void edit(tr('변형 지우기'), [{ op: 'clearTransformations', ids }]) : undefined },
    'id.object.content.graphic': { run: empty ? () => void edit(tr('내용: 그래픽'), [{ op: 'setContentType', ids, kind: 'graphic' }]) : undefined },
    'id.object.content.text': { run: toText ? () => void edit(tr('내용: 텍스트'), [{ op: 'setContentType', ids, kind: 'text' }]) : undefined },
    'id.object.content.unassigned': { run: empty ? () => void edit(tr('내용: 지정 안 함'), [{ op: 'setContentType', ids, kind: 'unassigned' }]) : undefined },
  };
}
