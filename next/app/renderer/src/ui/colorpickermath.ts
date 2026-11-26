// The Color Picker's screen maths: how the colour field and the slider map to a colour for each of the nine components
// (H S B, R G B, L a b), the working colour kept stable at greys, and hexadecimal. Only maths that needs no profile is here (the
// conversions of the features/color panel); CMYK and the gamut come from the engine query colorPick.
import { hsbToRgb, labToRgb, rgbToHsb, rgbToLab } from '../features/color/convert';

export type V3 = [number, number, number];
export type Mode = 'hsb' | 'rgb' | 'lab';
/** The component the slider follows (the picker's radio buttons): H S B, R G B, L a b. */
export type Axis = 'h' | 's' | 'v' | 'r' | 'g' | 'b' | 'l' | 'a' | 'bl';

export const RANGE: Record<Mode, [number, number][]> = {
  hsb: [[0, 360], [0, 100], [0, 100]],
  rgb: [[0, 255], [0, 255], [0, 255]],
  lab: [[0, 100], [-128, 127], [-128, 127]],
};
/** `k`: the component the slider carries; `x` and `y`: the two the field spreads (y grows upward). */
export const AXIS: Record<Axis, { mode: Mode; k: 0 | 1 | 2; x: 0 | 1 | 2; y: 0 | 1 | 2 }> = {
  h: { mode: 'hsb', k: 0, x: 1, y: 2 },
  s: { mode: 'hsb', k: 1, x: 0, y: 2 },
  v: { mode: 'hsb', k: 2, x: 0, y: 1 },
  r: { mode: 'rgb', k: 0, x: 2, y: 1 },
  g: { mode: 'rgb', k: 1, x: 2, y: 0 },
  b: { mode: 'rgb', k: 2, x: 0, y: 1 },
  l: { mode: 'lab', k: 0, x: 1, y: 2 },
  a: { mode: 'lab', k: 1, x: 2, y: 0 },
  bl: { mode: 'lab', k: 2, x: 1, y: 0 },
};

/** The colour being edited: the screen RGB and the same colour as HSB (kept apart so a grey keeps its hue while the field moves). */
export type Working = { rgb: V3; hsb: V3 };

export const clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v));
export const clampRgb = (c: V3): V3 => [clamp(c[0], 0, 255), clamp(c[1], 0, 255), clamp(c[2], 0, 255)];

export function comps(mode: Mode, w: Working): V3 {
  return mode === 'hsb' ? w.hsb : mode === 'rgb' ? w.rgb : rgbToLab(w.rgb);
}

/** A colour given as RGB: its HSB keeps the previous hue (and saturation) where the new colour has none (grey, black). */
export function fromRgb(rgb: V3, prev?: Working): Working {
  const c = clampRgb(rgb);
  const hsb = rgbToHsb(c);
  if (prev) {
    if (hsb[1] < 1e-6) hsb[0] = prev.hsb[0];
    if (hsb[2] < 1e-6) { hsb[0] = prev.hsb[0]; hsb[1] = prev.hsb[1]; }
  }
  return { rgb: c, hsb };
}

/** The working colour after component `index` of `mode` (or all three) took new values. */
export function withComps(mode: Mode, values: V3, prev: Working): Working {
  const v = values.map((x, i) => clamp(x, RANGE[mode][i][0], RANGE[mode][i][1])) as V3;
  if (mode === 'hsb') return { rgb: clampRgb(hsbToRgb(v)), hsb: v };
  return fromRgb(mode === 'rgb' ? v : labToRgb(v), prev);
}

/** Where the working colour sits in the field and on the slider of an axis: 0..1 each, y counted from the bottom. */
export function position(axis: Axis, w: Working): { x: number; y: number; k: number } {
  const a = AXIS[axis], c = comps(a.mode, w), r = RANGE[a.mode];
  const n = (i: number) => (c[i] - r[i][0]) / (r[i][1] - r[i][0]);
  return { x: n(a.x), y: n(a.y), k: n(a.k) };
}

/** The working colour for a position in the field (`x`, `y`) and on the slider (`k`) of an axis. */
export function atPosition(axis: Axis, pos: { x: number; y: number; k: number }, prev: Working): Working {
  const a = AXIS[axis], r = RANGE[a.mode], c: V3 = [0, 0, 0];
  const put = (i: number, n: number) => { c[i] = r[i][0] + clamp(n, 0, 1) * (r[i][1] - r[i][0]); };
  put(a.k, pos.k); put(a.x, pos.x); put(a.y, pos.y);
  return withComps(a.mode, c, prev);
}

/** Pixels of the colour field (RGBA, `size` x `size`, the slider's component fixed at `k`). */
export function fieldPixels(axis: Axis, k: number, size: number): Uint8ClampedArray {
  const px = new Uint8ClampedArray(size * size * 4);
  const a = AXIS[axis], r = RANGE[a.mode];
  const c: V3 = [0, 0, 0];
  c[a.k] = r[a.k][0] + k * (r[a.k][1] - r[a.k][0]);
  for (let j = 0; j < size; j++) {
    c[a.y] = r[a.y][0] + (1 - j / (size - 1)) * (r[a.y][1] - r[a.y][0]);
    for (let i = 0; i < size; i++) {
      c[a.x] = r[a.x][0] + (i / (size - 1)) * (r[a.x][1] - r[a.x][0]);
      const rgb = a.mode === 'hsb' ? hsbToRgb(c) : a.mode === 'rgb' ? c : labToRgb(c);
      const o = (j * size + i) * 4;
      px[o] = rgb[0]; px[o + 1] = rgb[1]; px[o + 2] = rgb[2]; px[o + 3] = 255;
    }
  }
  return px;
}

/** The slider's colours from top (k = 1) to bottom (k = 0), `size` of them. Measured in InDesign 2025's Color Picker (artifacts/panelinput-c1-20261103):
 *  the R G B and L a b sliders hold the field's two other components where the working colour has them; the H slider is the hue circle at
 *  S = B = 100 %, the S slider runs S at the colour's hue and B = 100 %, the B slider runs B at its hue and S = 100 %. */
export function sliderPixels(axis: Axis, w: Working, size: number): Uint8ClampedArray {
  const a = AXIS[axis], r = RANGE[a.mode], base = [...comps(a.mode, w)] as V3;
  if (a.mode === 'hsb') for (let i = 1; i < 3; i++) if (i !== a.k) base[i] = 100;
  const px = new Uint8ClampedArray(size * 4);
  for (let j = 0; j < size; j++) {
    const c = [...base] as V3;
    c[a.k] = r[a.k][0] + (1 - j / (size - 1)) * (r[a.k][1] - r[a.k][0]);
    const rgb = a.mode === 'hsb' ? hsbToRgb(c) : a.mode === 'rgb' ? c : labToRgb(c);
    px[j * 4] = rgb[0]; px[j * 4 + 1] = rgb[1]; px[j * 4 + 2] = rgb[2]; px[j * 4 + 3] = 255;
  }
  return px;
}

/** The picker shows hexadecimal in lower case (InDesign 2025: "c86432"). */
export const hexOf = (rgb: V3) => rgb.map((c) => Math.round(clamp(c, 0, 255)).toString(16).padStart(2, '0')).join('');

/** "#ff8800", "ff8800" or "f80" as a colour; null when it is not one. */
export function parseHex(text: string): V3 | null {
  const t = text.trim().replace(/^#/, '');
  if (!/^[0-9a-fA-F]{3}$|^[0-9a-fA-F]{6}$/.test(t)) return null;
  const full = t.length === 3 ? t.split('').map((c) => c + c).join('') : t;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16)) as V3;
}
