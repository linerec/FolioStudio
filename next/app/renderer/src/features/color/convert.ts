// Colour model conversions for the Color panel's four slider modes (CMYK, RGB, HSB, Lab). The
// panel shows a colour in the mode chosen from its menu; values the document keeps (CMYK, RGB
// or Lab) show as they are, the others are converted from the colour's screen RGB. Screen
// conversions only: nothing here is written to a document except the user's own slider values.
export type ColorMode = 'cmyk' | 'rgb' | 'hsb' | 'lab';

export type Channel = { key: string; max: number; min: number; unit: string };
const pct = (key: string): Channel => ({ key, min: 0, max: 100, unit: '%' });
export const CHANNELS: Record<ColorMode, Channel[]> = {
  cmyk: ['C', 'M', 'Y', 'K'].map(pct),
  rgb: ['R', 'G', 'B'].map((key) => ({ key, min: 0, max: 255, unit: '' })),
  hsb: [{ key: 'H', min: 0, max: 360, unit: '°' }, pct('S'), pct('B')],
  lab: [{ key: 'L', min: 0, max: 100, unit: '' }, { key: 'a', min: -128, max: 127, unit: '' }, { key: 'b', min: -128, max: 127, unit: '' }],
};

/** The mode a colour of this space is shown in without the user choosing one. */
export const nativeMode = (space: string | null | undefined): ColorMode =>
  space === 'RGB' ? 'rgb' : space === 'LAB' || space === 'Lab' ? 'lab' : 'cmyk';

/** The colour space a colour edited in this mode is written in (HSB is a way of entering RGB). */
export const spaceOf = (mode: ColorMode): 'CMYK' | 'RGB' | 'LAB' => (mode === 'cmyk' ? 'CMYK' : mode === 'lab' ? 'LAB' : 'RGB');

type V3 = [number, number, number];

export function rgbToHsb([r, g, b]: V3): V3 {
  const [x, y, z] = [r / 255, g / 255, b / 255];
  const max = Math.max(x, y, z), d = max - Math.min(x, y, z);
  let h = 0;
  if (d > 0) h = max === x ? ((y - z) / d + 6) % 6 : max === y ? (z - x) / d + 2 : (x - y) / d + 4;
  return [h * 60, max === 0 ? 0 : (d / max) * 100, max * 100];
}

export function hsbToRgb([h, s, v]: V3): V3 {
  const [S, V] = [s / 100, v / 100], c = V * S, hh = (((h % 360) + 360) % 360) / 60;
  const x = c * (1 - Math.abs((hh % 2) - 1)), m = V - c;
  const [r, g, b] = hh < 1 ? [c, x, 0] : hh < 2 ? [x, c, 0] : hh < 3 ? [0, c, x] : hh < 4 ? [0, x, c] : hh < 5 ? [x, 0, c] : [c, 0, x];
  return [(r + m) * 255, (g + m) * 255, (b + m) * 255];
}

// sRGB (D65) to CIE Lab with a D50 white, through the Bradford-adapted matrix
const lin = (c: number) => { const v = c / 255; return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4; };
const gam = (v: number) => 255 * (v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055);
const D50: V3 = [0.9642, 1, 0.8249];
const f = (t: number) => (t > 216 / 24389 ? Math.cbrt(t) : (24389 / 27 * t + 16) / 116);
const finv = (t: number) => (t ** 3 > 216 / 24389 ? t ** 3 : (116 * t - 16) / (24389 / 27));

export function rgbToLab([r, g, b]: V3): V3 {
  const [R, G, B] = [lin(r), lin(g), lin(b)];
  const X = 0.4360747 * R + 0.3850649 * G + 0.1430804 * B;
  const Y = 0.2225045 * R + 0.7168786 * G + 0.0606169 * B;
  const Z = 0.0139322 * R + 0.0971045 * G + 0.7141733 * B;
  const [fx, fy, fz] = [f(X / D50[0]), f(Y / D50[1]), f(Z / D50[2])];
  return [116 * fy - 16, 500 * (fx - fy), 200 * (fy - fz)];
}

export function labToRgb([l, a, b]: V3): V3 {
  const fy = (l + 16) / 116, fx = fy + a / 500, fz = fy - b / 200;
  const [X, Y, Z] = [finv(fx) * D50[0], finv(fy) * D50[1], finv(fz) * D50[2]];
  const R = 3.1338561 * X - 1.6168667 * Y - 0.4906146 * Z;
  const G = -0.9787684 * X + 1.9161415 * Y + 0.033454 * Z;
  const B = 0.0719453 * X - 0.2289914 * Y + 1.4052427 * Z;
  return [gam(R), gam(G), gam(B)].map((v) => Math.min(255, Math.max(0, v))) as V3;
}

/** Screen RGB to CMYK by the plain formula (K = 1 - max): for showing an RGB or Lab colour in CMYK mode only. */
export function rgbToCmyk([r, g, b]: V3): [number, number, number, number] {
  const [x, y, z] = [r / 255, g / 255, b / 255], k = 1 - Math.max(x, y, z);
  if (k >= 1) return [0, 0, 0, 100];
  return [(1 - x - k) / (1 - k) * 100, (1 - y - k) / (1 - k) * 100, (1 - z - k) / (1 - k) * 100, k * 100];
}

/** The Color panel's ramp: white, the six pure hues (red, yellow, green, cyan, blue, magenta) and black, evenly spaced; a click picks the colour under it,
 * in the panel's model (CMYK by the pure inks, RGB by the screen colours, HSB and Lab through RGB). */
export const RAMP_STOPS: { rgb: V3; cmyk: [number, number, number, number] }[] = [
  { rgb: [255, 255, 255], cmyk: [0, 0, 0, 0] }, { rgb: [237, 28, 36], cmyk: [0, 100, 100, 0] }, { rgb: [255, 242, 0], cmyk: [0, 0, 100, 0] },
  { rgb: [0, 166, 81], cmyk: [100, 0, 100, 0] }, { rgb: [0, 174, 239], cmyk: [100, 0, 0, 0] }, { rgb: [46, 49, 146], cmyk: [100, 100, 0, 0] },
  { rgb: [236, 0, 140], cmyk: [0, 100, 0, 0] }, { rgb: [0, 0, 0], cmyk: [0, 0, 0, 100] },
];
export function rampColor(mode: ColorMode, t: number): { space: 'CMYK' | 'RGB' | 'LAB'; value: number[] } {
  const x = Math.min(1, Math.max(0, t)) * (RAMP_STOPS.length - 1), i = Math.min(RAMP_STOPS.length - 2, Math.floor(x)), f = x - i;
  const [a, b] = [RAMP_STOPS[i], RAMP_STOPS[i + 1]];
  const mix = (p: number[], q: number[]) => p.map((v, k) => v + (q[k] - v) * f);
  const round = (v: number[]) => v.map((n) => Math.round(n * 10) / 10);
  if (mode === 'cmyk') return { space: 'CMYK', value: round(mix(a.cmyk, b.cmyk)) };
  const rgb = mix(a.rgb, b.rgb) as V3;
  if (mode === 'lab') return { space: 'LAB', value: round(rgbToLab(rgb)) };
  return { space: 'RGB', value: round(rgb) };
}

/** A colour as the panel shows it: its own values and space (when the document keeps them) and its screen RGB. */
export type Shown = { space: string | null; values: number[] | null; rgb: V3 | null };

/** The channel values of a colour in a mode (null when there is nothing to show). */
export function valuesIn(mode: ColorMode, c: Shown): number[] | null {
  const own = c.space === 'CMYK' ? 'cmyk' : c.space === 'RGB' ? 'rgb' : c.space === 'LAB' || c.space === 'Lab' ? 'lab' : null;
  if (own === mode && c.values) return c.values;
  if (!c.rgb) return null;
  switch (mode) {
    case 'rgb': return [...c.rgb];
    case 'hsb': return rgbToHsb(c.rgb);
    case 'lab': return own === 'lab' && c.values ? c.values : rgbToLab(c.rgb);
    case 'cmyk': return rgbToCmyk(c.rgb);
  }
}

/** What to write for channel `index` set to `value` (the others as shown): the space and the values in it. */
export function edited(mode: ColorMode, shown: number[], index: number, value: number): { space: 'CMYK' | 'RGB' | 'LAB'; value: number[] } {
  const v = shown.map((x, i) => (i === index ? value : x));
  return { space: spaceOf(mode), value: mode === 'hsb' ? hsbToRgb(v as V3) : v };
}
