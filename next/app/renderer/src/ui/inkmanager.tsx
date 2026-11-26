// Ink Manager… (Separations Preview and Swatches panel menus): the document's inks with their type, "spot to process" and Ink Alias, and
// All Spots to Process. The dialog edits a draft and sends the differences as one edit when OK is pressed (one undo step). The engine
// writes the settings on the `Ink` elements of Resources/Graphic.xml (edit/src/ops/inks.rs; the separations preview and the PDF read them
// back, render/src/inks.rs). Not built: neutral density, halftone angle and frequency, trapping sequence and Print Ink.
import { t as tr } from '../i18n';
import { useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';

type Ink = DocumentInfo['inks'][number];
type Edit = (label: string, commands: EditCommand[]) => unknown;
type Draft = { inkType: string; convertToProcess: boolean; alias: string };

const TYPES = () => [['Normal', tr('일반')], ['Transparent', tr('투명')], ['Opaque', tr('불투명')], ['OpaqueIgnore', tr('불투명 무시')]] as const;
const PROCESS_LABEL = () => ({ 'Process Cyan': tr('사이안'), 'Process Magenta': tr('마젠타'), 'Process Yellow': tr('노랑'), 'Process Black': tr('검정') }) as Record<string, string>;

const draftOf = (k: Ink): Draft => ({ inkType: k.inkType || 'Normal', convertToProcess: k.convertToProcess, alias: k.alias ?? '' });

/** The changes between the document's inks and the draft, as engine commands. */
export function inkCommands(inks: Ink[], draft: Record<string, Draft>): EditCommand[] {
  const out: EditCommand[] = [];
  for (const k of inks) {
    const was = draftOf(k), now = draft[k.name];
    if (!now || k.process) continue;
    const change: { inkType?: string; convertToProcess?: boolean; alias?: string } = {};
    if (now.inkType !== was.inkType) change.inkType = now.inkType;
    if (now.convertToProcess !== was.convertToProcess) change.convertToProcess = now.convertToProcess;
    if (now.alias !== was.alias) change.alias = now.alias;
    if (Object.keys(change).length) out.push({ op: 'setInk', name: k.name, ...change });
  }
  return out;
}

/** Whether `target` prints on `name`'s plate through the draft's aliases (so `name` may not alias to `target`). */
function leadsTo(draft: Record<string, Draft>, from: string, name: string): boolean {
  for (let at: string | undefined = from, n = 0; at && n < 64; n += 1) {
    if (at === name) return true;
    at = draft[at]?.alias || undefined;
  }
  return false;
}

export function InkManagerDialog({ info, edit, close }: { info: DocumentInfo | null; edit: Edit; close: () => void }) {
  const inks = useMemo(() => info?.inks ?? [], [info]);
  const [draft, setDraft] = useState<Record<string, Draft>>(() => Object.fromEntries(inks.map((k) => [k.name, draftOf(k)])));
  const spots = inks.filter((k) => !k.process);
  const label = (name: string) => PROCESS_LABEL()[name] ?? name;
  const set = (name: string, part: Partial<Draft>) => setDraft((d) => ({ ...d, [name]: { ...d[name], ...part } }));
  const allSpots = spots.length > 0 && spots.every((k) => draft[k.name]?.convertToProcess);
  const setAll = (on: boolean) => setDraft((d) => Object.fromEntries(Object.entries(d).map(([n, v]) => [n, inks.find((k) => k.name === n)?.process ? v : { ...v, convertToProcess: on }])));
  const commands = inkCommands(inks, draft);
  const ok = () => {
    if (commands.length) void edit(tr('잉크 관리자'), commands);
    close();
  };
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('잉크 관리자')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <div className="card dialog style-options ink-manager">
        <h2>{tr('잉크 관리자')}</h2>
        <fieldset>
          <legend>{tr('잉크')}</legend>
          <table className="ink-table">
            <thead>
              <tr><th>{tr('잉크')}</th><th>{tr('종류')}</th><th>{tr('별색을 원색으로')}</th><th>{tr('잉크 별칭')}</th></tr>
            </thead>
            <tbody>
              {inks.map((k) => {
                const d = draft[k.name] ?? draftOf(k);
                return (
                  <tr key={k.name} className={k.process ? 'process' : 'spot'}>
                    <td>{label(k.name)}{!k.process && <em> {tr('별색')}</em>}</td>
                    <td>
                      <select aria-label={tr('{0} 종류', { 0: label(k.name) })} value={d.inkType} disabled={k.process} onChange={(e) => set(k.name, { inkType: e.target.value })}>
                        {TYPES().map(([v, l]) => <option key={v} value={v}>{l}</option>)}
                      </select>
                    </td>
                    <td>
                      <input type="checkbox" aria-label={tr('{0}을(를) 원색으로', { 0: label(k.name) })} checked={d.convertToProcess} disabled={k.process} onChange={(e) => set(k.name, { convertToProcess: e.target.checked })} />
                    </td>
                    <td>
                      <select aria-label={tr('{0} 별칭', { 0: label(k.name) })} value={d.alias} disabled={k.process} onChange={(e) => set(k.name, { alias: e.target.value })}>
                        <option value="">{tr('별칭 없음')}</option>
                        {inks.filter((o) => o.name !== k.name && !leadsTo(draft, o.name, k.name)).map((o) => <option key={o.name} value={o.name}>{label(o.name)}</option>)}
                      </select>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </fieldset>
        <label className="check">
          <input type="checkbox" checked={allSpots} disabled={spots.length === 0} onChange={(e) => setAll(e.target.checked)} /> {tr('모든 별색을 일반 잉크로')}
        </label>
        <p className={`muted`} title={tr('중성 밀도·망점 각도·선수·트래핑 순서·잉크 인쇄')}>
          {tr('중성 밀도, 망점 각도·선수, 트래핑 순서, 잉크 인쇄 여부는 아직 고칠 수 없습니다.')}
        </p>
        <div className="dialog-actions">
          <button type="button" onClick={close}>{tr('취소')}</button>
          <button type="button" onClick={ok}>{tr('확인')}</button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
