// Window ▸ Object & Layout ▸ Pathfinder: the five Object ▸ Pathfinder commands as buttons (engine: edit/src/ops/pathfinder.rs).
import { tr } from '../i18n';
import type { Commands } from './commands';

const BUTTONS: [string, () => string][] = [
  ['id.object.pathfinder.add', () => tr('추가')],
  ['id.object.pathfinder.subtract', () => tr('빼기')],
  ['id.object.pathfinder.intersect', () => tr('교차')],
  ['id.object.pathfinder.excludeOverlap', () => tr('겹침 제외')],
  ['id.object.pathfinder.minusBack', () => tr('아래 개체 빼기')],
];

export function PathfinderPanel({ commands }: { commands: Commands }) {
  return <div className="pathfinder-panel" style={{ display: 'flex', flexDirection: 'column', gap: 6, padding: 8 }}>
    <h3>{tr('패스파인더')}</h3>
    {BUTTONS.map(([id, label]) => (
      <button key={id} data-command={id} disabled={!commands[id]?.run} onClick={() => commands[id]?.run?.()}>{label()}</button>
    ))}
    <p className="hint">{tr('직선으로 된 닫힌 도형 두 개를 고릅니다.')}</p>
  </div>;
}
