// The conveyor bar: a strip over the bottom of the document window that lists what the Content Collector took (conveyor.ts).
// It shows when View ▸ Extras ▸ Show Conveyor is on and the conveyor has something or the Collector / Placer is the tool.
import { useEffect } from 'react';
import { tr } from '../i18n';
import { conveyor, conveyorActions, conveyorLink, conveyorMapStyles, isConveyorTool, removeAfterPlace } from './conveyor';
import { useLive } from './store';
import type { ToolId } from './tools';

/** An IDML element name as a person calls the object. */
const KINDS: Record<string, () => string> = {
  Rectangle: () => tr('사각형'), Oval: () => tr('타원'), Polygon: () => tr('다각형'), GraphicLine: () => tr('선'),
  TextFrame: () => tr('텍스트 프레임'), Group: () => tr('그룹'), Image: () => tr('그림'), PDF: () => tr('그림'),
};
const kindName = (kind: string) => KINDS[kind]?.() ?? tr('개체');

export function ConveyorBar({ tool, shown }: { tool: ToolId; shown: boolean }) {
  const c = useLive(conveyor);
  const removeAfter = useLive(removeAfterPlace);
  const link = useLive(conveyorLink);
  const mapStyles = useLive(conveyorMapStyles);
  // the conveyor is the engine's: what another caller changed (a second window, a scripted request) shows within a moment
  useEffect(() => {
    if (!shown) return undefined;
    void conveyorActions.refresh().catch(() => undefined);
    const timer = window.setInterval(() => { void conveyorActions.refresh().catch(() => undefined); }, 1500);
    return () => window.clearInterval(timer);
  }, [shown, tool]);
  if (!shown || (!c.items.length && !isConveyorTool(tool))) return null;
  const note = (e: unknown) => console.error('conveyor', e);
  return (
    <div className="conveyor-bar" role="region" aria-label={tr('컨베이어')}>
      <span className="conveyor-title">{tr('컨베이어')}</span>
      <span className="conveyor-count" data-testid="conveyor-count">{tr('{0}개', { 0: c.items.length })}</span>
      <div className="conveyor-items">
        {c.items.length === 0 && <span className="conveyor-empty">{tr('내용 수집 도구(B)로 개체를 누르면 여기에 담깁니다')}</span>}
        {c.items.map((item, i) => {
          const first = item.objects[0];
          const label = item.objects.length > 1 ? tr('{0} 외 {1}개', { 0: kindName(first.kind), 1: item.objects.length - 1 }) : (first?.name || kindName(first?.kind ?? ''));
          return (
            <span key={item.id} className={`conveyor-item${i === c.current ? ' on' : ''}`} aria-current={i === c.current ? 'true' : undefined} data-item={item.id}>
              <button className="conveyor-pick" title={`${label} · ${item.document}`} onClick={() => void conveyorActions.select(i).catch(note)}>
                <span className="conveyor-kind">{kindName(first?.kind ?? '')}</span>
                <span className="conveyor-name">{label}</span>
              </button>
              <button className="conveyor-x" title={tr('컨베이어에서 빼기')} aria-label={tr('컨베이어에서 빼기')} onClick={() => void conveyorActions.remove(item.id).catch(note)}>×</button>
            </span>
          );
        })}
      </div>
      <label className="conveyor-opt" title={tr('놓은 개체는 컨베이어에서 뺍니다')}>
        <input type="checkbox" checked={removeAfter} onChange={(e) => removeAfterPlace.set(e.target.checked)} />
        {tr('놓은 뒤 빼기')}
      </label>
      <label className="conveyor-opt" title={tr('놓은 개체를 원본과 콘텐츠 링크로 잇습니다 (같은 문서 안)')}>
        <input type="checkbox" checked={link} onChange={(e) => conveyorLink.set(e.target.checked)} />
        {tr('링크 만들기')}
      </label>
      <label className="conveyor-opt" title={tr('링크를 만들 때 원본의 스타일을 놓은 곳의 스타일에 맞춥니다')}>
        <input type="checkbox" checked={mapStyles} onChange={(e) => conveyorMapStyles.set(e.target.checked)} />
        {tr('스타일 맞추기')}
      </label>
      <button className="conveyor-clear" disabled={!c.items.length} onClick={() => void conveyorActions.clear().catch(note)}>{tr('모두 빼기')}</button>
    </div>
  );
}
