import { useEffect, useState } from 'react';
import { UI_COLORS } from '../../ui/storyeditor';
import type { SpellOptions, UserDictionaries } from './spelling';

/** One word to underline: the baseline from its first to its last character, on a page (page points). */
type Mark = { page: number; from: [number, number]; to: [number, number]; story: string; start: number; end: number };

/** InDesign names the colour `RED`, `LightBlue` or `LIGHT_BLUE` depending on where it is read; the screen colours are the same table the story editor uses. */
export function misspelledColor(value: unknown): string {
  const key = String(value ?? 'RED').replace(/[^A-Za-z]/g, '').toUpperCase();
  const hit = Object.entries(UI_COLORS).find(([name]) => name.replace(/_/g, '') === key);
  return hit ? hit[1] : UI_COLORS.RED;
}

/** The squiggle InDesign 2026 draws under a word (artifacts/spell-c2-20261023/STATUS.ko.md, window captures of the dynamic spelling
 * switch on and off): a red triangle wave from the start to the end of the word, sized in points whatever the type size is, so it
 * follows the zoom, but only up to a limit on screen (at 1500% it stays 50 px long per period, 42 px high, 17 px thick, device pixels).
 * Its top is 0.25 pt below the baseline. `k` is the device pixels one point has. The line is drawn along the baseline, so a
 * rotated frame gets a rotated squiggle. Screen only: it is not in the document, a print or a PDF. */
export function squigglePoints(from: [number, number], to: [number, number], k: number): { points: string; width: number } {
  const period = Math.min(2.45, 50 / k), vertical = Math.min(1.0, 17 / k), amplitude = Math.min(1.2, 24 / k);
  const half = period / 2, depth = 0.25 + (amplitude + vertical) / 2;
  // the wave is as thick across the stroke as `vertical` is straight down, so divide out the slope
  const width = vertical / Math.hypot(1, amplitude / half);
  const dx = to[0] - from[0], dy = to[1] - from[1], length = Math.hypot(dx, dy);
  if (length < 1e-6) return { points: '', width };
  const u = [dx / length, dy / length], n = [-u[1], u[0]];
  const out: string[] = [];
  for (let t = 0, i = 0; ; t += half, i++) {
    const at = Math.min(t, length), wave = depth + (i % 2 ? amplitude / 2 : -amplitude / 2);
    out.push(`${(from[0] + u[0] * at + n[0] * wave).toFixed(3)},${(from[1] + u[1] * at + n[1] * wave).toFixed(3)}`);
    if (t >= length) break;
  }
  return { points: out.join(' '), width };
}

/** The dynamic-spelling underline layer: asks the engine (query `spellMarks`) which words of the document are not in the dictionary
 * and draws a squiggle under each. The switch is the user setting `spellPreferences.dynamicSpellCheck`. */
export function useDynamicSpellingMarks(args: { enabled: boolean; document: string; revision: Map<number, number>; options: SpellOptions; userDictionaries: UserDictionaries; color: unknown }) {
  const { enabled, document, revision, options, userDictionaries, color } = args;
  const [marks, setMarks] = useState<Mark[]>([]);
  const settings = JSON.stringify([options, userDictionaries]);
  useEffect(() => {
    let active = true;
    setMarks([]);
    if (!enabled) return () => { active = false; };
    const timer = setTimeout(() => {
      void window.folio.request({ cmd: 'spellMarks', options, userDictionaries }).then(({ header }) => { if (active && header.ok) setMarks(header.marks as Mark[]); }).catch(() => {});
    }, 150);
    return () => { active = false; clearTimeout(timer); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [enabled, document, revision, settings]);
  const stroke = misspelledColor(color);
  return (page: number, width: number, height: number, scale: number) => {
    if (!enabled) return null;
    const k = scale * (window.devicePixelRatio || 1);
    return <svg data-dynamic-spelling="true" viewBox={`0 0 ${width} ${height}`} style={{ position: 'absolute', inset: 0, width: '100%', height: '100%', pointerEvents: 'none', overflow: 'visible' }}>
      {marks.filter(m => m.page === page).map((m, i) => {
        const { points, width: w } = squigglePoints(m.from, m.to, k);
        return points ? <polyline key={i} data-spell-mark={m.story + ':' + m.start} points={points} fill="none" stroke={stroke} strokeWidth={w} strokeLinejoin="miter" strokeLinecap="butt" /> : null;
      })}
    </svg>;
  };
}
