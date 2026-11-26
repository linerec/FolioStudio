// The bracket handles of a selected text path, as InDesign 2025 draws them (measured on screenshots, next/oracle/rules/typepath-c14.ko.md): in the layer
// colour, a 1 px line across the path at the start and the end bracket (from `above` over the path to `below` under it), an 11 x 11 px white square with a
// 1 px edge on the line's outer side at its middle (the text port: behind the start line, ahead of the end line), and a 12 px tick above the path at the middle
// between the brackets. Pixel sizes do not change with the zoom.
import type { ItemInfo } from '../../inspect';
import { PORT, TICK, placeBracket, type BracketView } from './useTextPathBrackets';

export function BracketMarks({ it, view, s, color }: { it: ItemInfo; view: BracketView; s: number; color: string }) {
  const h = view.handles;
  const marks: React.ReactElement[] = [];
  for (const which of ['start', 'end'] as const) {
    const { p, t, n } = placeBracket(it, h[which]);
    const [x, y] = [p[0] * s, p[1] * s];
    const top: [number, number] = [x + n[0] * h.above * s, y + n[1] * h.above * s];
    const bottom: [number, number] = [x - n[0] * h.below * s, y - n[1] * h.below * s];
    marks.push(<line key={which} className="tp-bracket" stroke={color} x1={top[0]} y1={top[1]} x2={bottom[0]} y2={bottom[1]} />);
    const reach = (PORT - 1) / 2 * (which === 'start' ? -1 : 1);
    const c: [number, number] = [(top[0] + bottom[0]) / 2 + t[0] * reach, (top[1] + bottom[1]) / 2 + t[1] * reach];
    const side = PORT - 1;
    marks.push(<rect key={`${which}port`} className="tp-port" stroke={color} x={c[0] - side / 2} y={c[1] - side / 2} width={side} height={side}
      transform={`rotate(${Math.atan2(t[1], t[0]) * 180 / Math.PI} ${c[0]} ${c[1]})`} />);
  }
  const { p, n } = placeBracket(it, h.center);
  marks.push(<line key="center" className="tp-bracket" stroke={color} x1={p[0] * s} y1={p[1] * s} x2={p[0] * s + n[0] * TICK} y2={p[1] * s + n[1] * TICK} />);
  return <g className={`tp-brackets${view.dragging ? ' dragging' : ''}`} data-bracket-item={view.id}>{marks}</g>;
}
