// Object ▸ Convert Shape, Object ▸ Paths (Open, Close, Reverse, Join), Pathfinder and Convert Point on the selected objects. The
// engine gives each a new path that keeps the box it showed, stroke included, as InDesign does
// (edit/src/ops/convert.rs).
import { t as tr } from '../i18n';
import type { EditCommand } from '../engine';
import type { Command } from './commands';

type Shape = 'rectangle' | 'roundedRectangle' | 'beveledRectangle' | 'inverseRoundedRectangle' | 'ellipse' | 'triangle'
  | 'polygon' | 'line' | 'straightLine' | 'openPath' | 'closedPath';

const SHAPES: [string, Shape, () => string][] = [
  ['id.object.convertShape.rectangle', 'rectangle', () => tr('사각형으로 변환')],
  ['id.object.convertShape.roundedRectangle', 'roundedRectangle', () => tr('둥근 사각형으로 변환')],
  ['id.object.convertShape.beveledRectangle', 'beveledRectangle', () => tr('경사진 사각형으로 변환')],
  ['id.object.convertShape.inverseRoundedRectangle', 'inverseRoundedRectangle', () => tr('역둥근 사각형으로 변환')],
  ['id.object.convertShape.ellipse', 'ellipse', () => tr('타원으로 변환')],
  ['id.object.convertShape.triangle', 'triangle', () => tr('삼각형으로 변환')],
  ['id.object.convertShape.polygon', 'polygon', () => tr('다각형으로 변환')],
  ['id.object.convertShape.line', 'line', () => tr('선으로 변환')],
  ['id.object.convertShape.orthogonalLine', 'straightLine', () => tr('직교선으로 변환')],
  ['id.object.paths.openPath', 'openPath', () => tr('패스 열기')],
  ['id.object.paths.closePath', 'closedPath', () => tr('패스 닫기')],
];

/** The Convert Shape and Paths commands for App's command table. */
export function shapeCommands({ editable, ids, compound = [], paths = () => null, points = () => null, edit }: {
  editable: boolean; ids: string[]; /** The selected objects that are compound paths. */ compound?: string[];
  /** An object's paths: whether each is open and how many points it has. */
  paths?: (id: string) => { open: boolean; count: number }[] | null;
  /** The anchor points selected with the Direct Selection tool. */
  points?: () => { id: string; path: number; indices: number[] } | null;
  edit: (label: string, commands: EditCommand[]) => unknown;
}): Record<string, Partial<Command>> {
  const ok = editable && ids.length > 0;
  const out: Record<string, Partial<Command>> = {};
  for (const [cmd, to, label] of SHAPES) {
    out[cmd] = { run: ok ? () => edit(label(), [{ op: 'convertShape', ids, to }]) : undefined };
  }
  // Make Compound Path (Ctrl+8): two or more shapes, the rearmost taking the others' paths; Release: each path its own object
  out['object.compoundPath'] = { run: ok && ids.length > 1 ? () => edit(tr('컴파운드 패스 만들기'), [{ op: 'makeCompoundPath', ids }]) : undefined };
  out['id.object.paths.releaseCompoundPath'] = { run: editable && compound.length ? () => edit(tr('컴파운드 패스 풀기'), compound.map((id) => ({ op: 'releaseCompoundPath' as const, id }))) : undefined };
  out['id.object.paths.reversePath'] = { run: ok ? () => edit(tr('패스 반전'), [{ op: 'reversePath', ids }]) : undefined };
  // Object ▸ Paths ▸ Join: the first selected open path takes the other open paths (or closes itself when alone)
  const open = ids.filter((id) => { const p = paths(id); return p?.length === 1 && p[0].open; });
  out['id.object.paths.join'] = { run: editable && open.length > 0 && open.length === ids.length ? () => edit(tr('패스 연결'), [{ op: 'joinPaths', ids: open }]) : undefined };
  // Object ▸ Pathfinder: two closed shapes of straight segments become one (the engine refuses curves and other kinds)
  const pathfinder = (cmd: string, kind: 'add' | 'subtract' | 'intersect' | 'excludeOverlap' | 'minusBack', label: () => string) => {
    out[cmd] = { run: ok && ids.length === 2 ? () => edit(label(), [{ op: 'pathfinder', ids, kind }]) : undefined };
  };
  pathfinder('id.object.pathfinder.add', 'add', () => tr('패스파인더: 추가'));
  pathfinder('id.object.pathfinder.subtract', 'subtract', () => tr('패스파인더: 빼기'));
  pathfinder('id.object.pathfinder.intersect', 'intersect', () => tr('패스파인더: 교차'));
  pathfinder('id.object.pathfinder.excludeOverlap', 'excludeOverlap', () => tr('패스파인더: 겹침 제외'));
  pathfinder('id.object.pathfinder.minusBack', 'minusBack', () => tr('패스파인더: 아래 개체 빼기'));
  // Object ▸ Convert Point: the selected anchor points only (InDesign greys it with a whole object selected: measured with the
  // menu actions, oracle/indesign/probe-convert-object.jsx)
  const picked = points();
  const convert = (cmd: string, kind: 'plain' | 'corner' | 'smooth' | 'symmetrical', label: () => string) => {
    out[cmd] = {
      run: editable && picked && picked.indices.length ? () => edit(label(), [{ op: 'convertPathPoints', id: picked.id, path: picked.path, indices: picked.indices, kind }]) : undefined,
    };
  };
  convert('id.object.convertPoint.plain', 'plain', () => tr('기준점 변환: 일반'));
  convert('id.object.convertPoint.corner', 'corner', () => tr('기준점 변환: 모퉁이'));
  convert('id.object.convertPoint.smooth', 'smooth', () => tr('기준점 변환: 매끄럽게'));
  convert('id.object.convertPoint.symmetrical', 'symmetrical', () => tr('기준점 변환: 대칭'));
  return out;
}
