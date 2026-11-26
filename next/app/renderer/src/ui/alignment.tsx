import { tr } from '../i18n';
// One set of alignment options shared by the control bar and Align panel.
// Geometry belongs to folio-edit; next/oracle/scenarios/basic-align*.json proves it.
import { useState } from 'react';
import type { AlignBounds, AlignMode, EditCommand } from '../engine';
import { type DocumentInfo, type PageItems, parseLength } from '../inspect';
import { Icon } from './icons';
import { type SelEntry, findItem } from './overlay';

export const ALIGN_BUTTONS: { how: AlignMode; icon: string; title: string }[] = [
  { how: 'left', icon: 'alignLeft', get title() { return tr("왼쪽 가장자리"); } },
  { how: 'hCenter', icon: 'alignHCenter', get title() { return tr("수평 가운데"); } },
  { how: 'right', icon: 'alignRight', get title() { return tr("오른쪽 가장자리"); } },
  { how: 'top', icon: 'alignTop', get title() { return tr("위쪽 가장자리"); } },
  { how: 'vCenter', icon: 'alignVCenter', get title() { return tr("수직 가운데"); } },
  { how: 'bottom', icon: 'alignBottom', get title() { return tr("아래쪽 가장자리"); } },
];
const references = (): [AlignBounds, string][] => [
  ['selection', tr("선택 영역")], ['margin', tr("여백")], ['page', tr("페이지")], ['spread', tr("펼침면")], ['bleed', tr("재단 여유")], ['key', tr("키 개체")],
];

export function useAlignment(
  selection: SelEntry[], info: DocumentInfo | null, store: { peek: (p: number) => PageItems | null | undefined },
  editable: boolean, units: { horizontal: string; vertical: string }, edit: (label: string, commands: EditCommand[]) => unknown,
) {
  const [bounds, setBounds] = useState<AlignBounds>('selection');
  const [chosenKey, setKey] = useState('');
  const [useGap, setUseGap] = useState(false);
  const [gap, setGap] = useState('0 pt');
  const selected = new Set(selection.filter((s) => !s.content).map((s) => s.id));
  const entries = selection.filter((s, index) => !s.content && selection.findIndex((x) => x.id === s.id) === index)
    .filter((s) => !findItem(store.peek(s.page), s.id)?.path.slice(0, -1).some((p) => selected.has(p.id)));
  const ids = entries.map((e) => e.id);
  const key = ids.includes(chosenKey) ? chosenKey : ids[ids.length - 1] ?? '';
  const spreads = new Set(entries.map((s) => info?.pages.find((p) => p.index === s.page)?.spreadIndex));
  const available = editable && spreads.size === 1 && !spreads.has(undefined) && selection.every((e) => !e.content);
  const canAlign = available && ids.length >= (bounds === 'selection' || bounds === 'key' ? 2 : 1);
  const canDistribute = available && ids.length >= 2;
  const choices = entries.map((e, index) => {
    const it = findItem(store.peek(e.page), e.id)?.item;
    const kind = { get textFrame() { return tr("텍스트 프레임"); }, get group() { return tr("그룹"); }, get rectangle() { return tr("사각형"); }, get oval() { return tr("타원"); }, get polygon() { return tr("다각형"); }, get line() { return tr("선"); }, get other() { return tr("개체"); } };
    return { id: e.id, title: `${index + 1}. ${it?.name || (it ? kind[it.kind] : tr("개체"))} (${e.id})` };
  });
  const distance = (how: AlignMode) => parseLength(gap, ['top', 'vCenter', 'bottom', 'vSpace'].includes(how) ? units.vertical : units.horizontal);
  const enabled = (op: 'align' | 'distribute', how: AlignMode) => op === 'align' ? canAlign
    : canDistribute && (!(useGap || bounds === 'key') || distance(how) !== null);
  const run = (op: 'align' | 'distribute', how: AlignMode) => {
    if (!enabled(op, how)) return;
    const reference = bounds === 'key' ? { key } : {};
    if (op === 'align') edit(tr("개체 정렬"), [{ op, ids, how, bounds, ...reference }]);
    else edit(how.endsWith('Space') ? tr("간격 분배") : tr("개체 분배"), [{
      op, ids, how, bounds, ...reference, ...((useGap || bounds === 'key') ? { gap: distance(how)! } : {}),
    }]);
  };
  return { bounds, setBounds, key, setKey, choices, useGap, setUseGap, gap, setGap, canAlign, enabled, run, units };
}

export type AlignmentControls = ReturnType<typeof useAlignment>;

export function AlignReference({ control, compact = false }: { control: AlignmentControls; compact?: boolean }) {
  return (
    <select className={compact ? 'al-reference compact' : 'al-reference'} aria-label={tr("정렬 기준")} title={tr("정렬·분배 기준")}
      value={control.bounds} onChange={(e) => control.setBounds(e.target.value as AlignBounds)}>
      {references().map(([value, label]) => <option key={value} value={value}>{label}</option>)}
    </select>
  );
}

export function AlignButtons({ control, distribute = false, spacing = false }: {
  control: AlignmentControls; distribute?: boolean; spacing?: boolean;
}) {
  const buttons = spacing
    ? [{ how: 'vSpace' as const, icon: 'alignVCenter', get title() { return tr("수직 간격"); } }, { how: 'hSpace' as const, icon: 'alignHCenter', get title() { return tr("수평 간격"); } }]
    : ALIGN_BUTTONS;
  const op = distribute || spacing ? 'distribute' : 'align';
  return (
    <span className="seg al-buttons">
      {buttons.map((b) => <button key={b.how} title={op === 'align' ? tr('{0} 정렬', {0:b.title}) : tr('{0} 분배', {0:b.title})}
        aria-label={op === 'align' ? tr('{0} 정렬', {0:b.title}) : tr('{0} 분배', {0:b.title})} data-align={`${op}-${b.how}`}
        disabled={!control.enabled(op, b.how)} onClick={() => control.run(op, b.how)}>
        <Icon name={b.icon} size={16} />
      </button>)}
    </span>
  );
}
