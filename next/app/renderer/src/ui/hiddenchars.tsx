// Type ▸ Show Hidden Characters: InDesign's marks for spaces (·), non-breaking spaces (^), tabs
// (»), paragraph ends (¶), forced line breaks (¬) and the end of a story (#), drawn over a page
// in the colour of each text frame's layer. Where they go comes from the engine (`textMarks`):
// each mark is a character's advance on its baseline and the top of its line, so a mark follows
// a turned frame.
import { useEffect, useState } from 'react';
import type { PageItems } from '../inspect';
import { findItem, type LayerLook } from './overlay';

type Mark = { kind: string; frame: string; from: [number, number]; to: [number, number]; up: [number, number] };

const GLYPH: Record<string, string> = { space: '·', nonbreakingSpace: '^', otherSpace: '·', tab: '»', paragraph: '¶', lineBreak: '¬', end: '#' };
/** Marks drawn in the middle of their character's advance (the others start where it starts). */
const MIDDLE = new Set(['space', 'nonbreakingSpace', 'otherSpace']);

type Props = { page: number; width: number; height: number; rev: number; items: PageItems | null | undefined; layers: LayerLook };

export function HiddenMarks({ page, width, height, rev, items, layers }: Props) {
  const [marks, setMarks] = useState<Mark[]>([]);
  useEffect(() => {
    let live = true;
    window.folio.request({ cmd: 'textMarks', page }).then(({ header }) => {
      if (live && header.ok) setMarks((header.marks as Mark[]) ?? []);
    }, () => { /* no document */ });
    return () => { live = false; };
  }, [page, rev]);
  const colour = (frame: string) => {
    const it = findItem(items, frame)?.item;
    const c = it?.layer ? layers.get(it.layer)?.color : undefined;
    return c ? `rgb(${c[0]},${c[1]},${c[2]})` : 'rgb(79,153,255)';
  };
  return (
    <svg className="hidden-marks" viewBox={`0 0 ${width} ${height}`} aria-hidden="true"
      style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', pointerEvents: 'none', overflow: 'visible' }}>
      {marks.map((m, i) => {
        const ux = m.up[0] - m.from[0], uy = m.up[1] - m.from[1];
        const h = Math.hypot(ux, uy) || 8;
        // the baseline runs a quarter turn clockwise from "up"
        const angle = Math.atan2(ux, -uy) * 180 / Math.PI;
        const w = Math.hypot(m.to[0] - m.from[0], m.to[1] - m.from[1]);
        const mid = MIDDLE.has(m.kind);
        return (
          <text key={i} transform={`translate(${m.from[0]} ${m.from[1]}) rotate(${angle})`} x={mid ? w / 2 : 0} y={0}
            fontSize={h} fontFamily="Arial, sans-serif" textAnchor={mid ? 'middle' : 'start'} fill={colour(m.frame)}>{GLYPH[m.kind] ?? ''}</text>
        );
      })}
    </svg>
  );
}
