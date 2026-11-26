import { useState } from 'react';
import { tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { DocumentInfo, Guide, ItemStore } from '../inspect';
import type { Command } from './commands';
import type { GuideSelection } from './guides';
import { ColorField, CUSTOM, NumField, PrefsDialog, colorValue } from './prefsfields';

type Edit = (label: string, commands: EditCommand[]) => Promise<EditResult | null>;

export function useGuideEditing(o: { editable: boolean; info: DocumentInfo | null; store: ItemStore; selected: GuideSelection; edit: Edit }) {
  const [dialog, setDialog] = useState<{ kind: 'options' | 'move'; guide?: Guide } | null>(null);
  const g = o.selected ? o.store.peek(o.selected.page)?.guides.find(g => g.id === o.selected?.id) : undefined;
  const set = (props: Record<string, string | number | boolean>, label: string) => {
    if (g) void o.edit(label, [{ op: 'setGuideOptions', ids: [g.id], props }]);
  };
  const handlers: Record<string, Partial<Command>> = {
    'layout.rulerGuides': { run: o.editable ? () => setDialog({ kind: 'options', guide: g }) : undefined },
    'id.ctx.guide.guideType.rulerGuide': { run: o.editable && g && !g.locked ? () => set({ guideType: 'Ruler' }, tr('눈금자 안내선')) : undefined, checked: g?.guideType === 'Ruler' },
    'id.ctx.guide.moveGuides': { run: o.editable && g && !g.locked ? () => setDialog({ kind: 'move', guide: g }) : undefined },
    'id.ctx.guide.unlock': { run: o.editable && g?.ownLocked ? () => set({ locked: false }, tr('안내선 잠금 해제')) : undefined },
  };
  if (g) handlers['object.lock'] = { run: o.editable && !g.locked ? () => set({ locked: true }, tr('안내선 잠금')) : undefined };
  return { handlers, dialog: dialog && <GuideDialog {...dialog} defaults={o.info?.view?.rulerGuides} edit={o.edit} onClose={() => setDialog(null)} /> };
}

function GuideDialog({ kind, guide, defaults, edit, onClose }: {
  kind: 'options' | 'move'; guide?: Guide; defaults?: { threshold: number; colorName: string | null }; edit: Edit; onClose: () => void;
}) {
  const [threshold, setThreshold] = useState(String(guide?.viewThreshold ?? defaults?.threshold ?? 5));
  const [color, setColor] = useState(colorValue(guide ? guide.colorName : defaults?.colorName ?? 'Cyan'));
  const [x, setX] = useState('0'), [y, setY] = useState('0');
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const title = kind === 'move' ? tr('안내선 이동') : tr('눈금자 안내선');
  const submit = async () => {
    if (busy) return;
    const numbers = kind === 'move' ? [x, y] : [threshold];
    if (numbers.some(v => !v.trim() || !Number.isFinite(Number(v)))) { setError(tr('올바른 숫자를 입력하세요.')); return; }
    let commands: EditCommand[];
    if (kind === 'move') commands = [{ op: 'moveGuidesBy', ids: guide ? [guide.id] : [], delta: [Number(x), Number(y)] }];
    else if (guide) commands = [{ op: 'setGuideOptions', ids: [guide.id], props: { viewThreshold: Number(threshold), ...(color !== CUSTOM ? { guideColor: color } : {}) } }];
    else commands = [{ op: 'setViewPref', key: 'rulerGuidesThreshold', value: Number(threshold) }, ...(color !== CUSTOM ? [{ op: 'setViewPref' as const, key: 'rulerGuidesColor', value: color }] : [])];
    setBusy(true); setError('');
    try { if (await edit(title, commands)) onClose(); else setError(tr('변경하지 못했습니다. 상태 표시줄의 오류를 확인하세요.')); }
    finally { setBusy(false); }
  };
  return <PrefsDialog title={title} className="guide-dialog" busy={busy} error={error} onClose={onClose} onSubmit={submit}>
    {kind === 'move' ? <>
      <NumField label={tr('가로 이동')} name="guide-x" value={x} set={setX} suffix="pt" spec="number" />
      <NumField label={tr('세로 이동')} name="guide-y" value={y} set={setY} suffix="pt" spec="number" />
    </> : <>
      <NumField label={tr('보기 한계값')} name="guide-threshold" value={threshold} set={setThreshold} suffix="%" spec="number" />
      <ColorField label={tr('색상')} name="guide-color" value={color} set={setColor} />
    </>}
  </PrefsDialog>;
}
