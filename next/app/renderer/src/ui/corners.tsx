// Corner options: Object ▸ Corner Options (each corner, or all the same) and the control bar's
// shape and size (all four corners). The engine writes them as InDesign does
// (edit/src/ops/corners.rs); the current corners come from the reader (pageItems `corners`).
import { t as tr } from '../i18n';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand } from '../engine';
import type { Corner, CornerShape, ItemInfo, PageItems } from '../inspect';
import { type SelEntry, findItem } from './overlay';
import { TextNumberField, stepOfLength } from './controls/lengthfield';
import './corners.css';

export const SHAPES: CornerShape[] = ['None', 'FancyCorner', 'BevelCorner', 'InsetCorner', 'InverseRoundedCorner', 'RoundedCorner'];
export const shapeLabel = (s: CornerShape) => ({
  None: tr('없음'), FancyCorner: tr('장식'), BevelCorner: tr('경사'), InsetCorner: tr('안쪽 사각형'),
  InverseRoundedCorner: tr('역방향 둥글게'), RoundedCorner: tr('둥글게'),
})[s];
const NAMES = () => [tr('왼쪽 위'), tr('오른쪽 위'), tr('왼쪽 아래'), tr('오른쪽 아래')];
const DEFAULT: Corner = { shape: 'None', radius: 12 };

/** The selected objects that have corners of their own (not lines, groups or placed content). */
export function cornerTargets(selection: SelEntry[], peek: (page: number) => PageItems | null | undefined): ItemInfo[] {
  return selection.filter((s) => !s.content).map((s) => findItem(peek(s.page), s.id)?.item)
    .filter((it): it is ItemInfo => !!it && ['rectangle', 'oval', 'polygon', 'textFrame'].includes(it.kind) && !it.locked);
}

export const cornersOf = (it: ItemInfo): Corner[] => (it.corners?.length === 4 ? it.corners : [DEFAULT, DEFAULT, DEFAULT, DEFAULT]);

/** One edit for every target: its corners changed by `change`. */
export function cornerCommands(targets: ItemInfo[], change: (c: Corner[]) => Corner[]): EditCommand[] {
  return targets.map((it) => ({ op: 'setCorners', ids: [it.id], corners: change(cornersOf(it).map((c) => ({ ...c }))) }) as EditCommand);
}

/** What the control bar shows: the corners' common shape and size, or null when they differ. */
export function common(targets: ItemInfo[]): { shape: CornerShape | null; radius: number | null } {
  const all = targets.flatMap(cornersOf);
  if (!all.length) return { shape: null, radius: null };
  return { shape: all.every((c) => c.shape === all[0].shape) ? all[0].shape : null, radius: all.every((c) => c.radius === all[0].radius) ? all[0].radius : null };
}

/** Object ▸ Corner Options… */
export function CornerOptionsDialog({ targets, format, parse, edit, close }: {
  targets: ItemInfo[]; format: (pt: number) => string; parse: (text: string) => number | null;
  edit: (label: string, commands: EditCommand[]) => unknown; close: () => void;
}) {
  const start = cornersOf(targets[0]);
  const [corners, setCorners] = useState<Corner[]>(start.map((c) => ({ ...c })));
  const [texts, setTexts] = useState<string[]>(start.map((c) => format(c.radius)));
  const [same, setSame] = useState(start.every((c) => c.shape === start[0].shape && c.radius === start[0].radius));
  const sizes = texts.map(parse);
  const step = stepOfLength(format, parse);
  const ok = sizes.every((v) => v !== null && v >= 0);
  const set = (k: number, c: Partial<Corner>, text?: string) => {
    const at = same ? [0, 1, 2, 3] : [k];
    setCorners((cs) => cs.map((x, j) => (at.includes(j) ? { ...x, ...c } : x)));
    if (text !== undefined) setTexts((ts) => ts.map((x, j) => (at.includes(j) ? text : x)));
  };
  const submit = () => {
    const final = corners.map((c, k) => ({ ...c, radius: sizes[k] ?? c.radius }));
    edit(tr('모퉁이 옵션'), cornerCommands(targets, () => final));
    close();
  };
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('모퉁이 옵션')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options corner-options" onSubmit={(e) => { e.preventDefault(); if (ok) submit(); }}>
        <h2>{tr('모퉁이 옵션')}</h2>
        <label className="check"><input type="checkbox" checked={same} onChange={(e) => {
          setSame(e.target.checked);
          if (e.target.checked) { setCorners((cs) => cs.map(() => ({ ...cs[0] }))); setTexts((ts) => ts.map(() => ts[0])); }
        }} /> {tr('모든 설정을 동일하게')}</label>
        <div className="corner-grid">
          {NAMES().map((name, k) => (
            <fieldset key={k} aria-label={name}>
              <legend>{name}</legend>
              <TextNumberField width={96} label={tr('{0} 크기', { 0: name })} text={texts[k]} onText={(t) => set(k, {}, t)} format={format} parse={parse} min={0} step={step} />
              <select aria-label={tr('{0} 모양', { 0: name })} value={corners[k].shape} onChange={(e) => set(k, { shape: e.target.value as CornerShape })}>
                {SHAPES.map((s) => <option key={s} value={s}>{shapeLabel(s)}</option>)}
              </select>
            </fieldset>
          ))}
        </div>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!ok}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
