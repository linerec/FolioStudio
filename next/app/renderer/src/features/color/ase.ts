// Adobe Swatch Exchange (.ase), the public swatch file format: names, a colour model and the
// values only (no profiles, no gradients, no tints). Numbers are big endian.
//   "ASEF", version 1.0 (u16 u16), block count (u32), then blocks:
//   block = type (u16), length (u32), data. Types: 0xC001 group start (name), 0xC002 group
//   end (no data), 0x0001 colour (name, model, values, colour type).
//   name = length in UTF-16 units including the end 0 (u16), the units (UTF-16BE).
//   model = "CMYK" (4 floats 0..1), "RGB " (3 floats 0..1), "LAB " (L 0..1 = L/100, a, b), "Gray" (1 float).
//   colour type (u16): 0 global (a process swatch), 1 spot, 2 normal.
// Values here are the document's: CMYK 0..100, RGB 0..255, LAB L 0..100 with a and b as they are.
export type AseColor = { name: string; space: 'CMYK' | 'RGB' | 'LAB'; values: number[]; spot: boolean };
/** `name` null: colours outside any group. */
export type AseGroup = { name: string | null; colors: AseColor[] };

const MODEL = { CMYK: 'CMYK', RGB: 'RGB ', LAB: 'LAB ' } as const;
const SCALE = { CMYK: [0.01, 0.01, 0.01, 0.01], RGB: [1 / 255, 1 / 255, 1 / 255], LAB: [0.01, 1, 1] } as const;

function nameBytes(name: string): number[] {
  const out = [(name.length + 1) >> 8, (name.length + 1) & 255];
  for (let i = 0; i < name.length; i++) out.push(name.charCodeAt(i) >> 8, name.charCodeAt(i) & 255);
  out.push(0, 0);
  return out;
}

function block(type: number, data: number[]): number[] {
  const n = data.length;
  return [type >> 8, type & 255, (n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255, ...data];
}

function colorBlock(c: AseColor): number[] {
  const data = nameBytes(c.name);
  for (const ch of MODEL[c.space]) data.push(ch.charCodeAt(0));
  const f = new DataView(new ArrayBuffer(4));
  c.values.forEach((v, i) => { f.setFloat32(0, v * SCALE[c.space][i]); for (let k = 0; k < 4; k++) data.push(f.getUint8(k)); });
  data.push(0, c.spot ? 1 : 0);
  return block(0x0001, data);
}

export function writeAse(groups: AseGroup[]): Uint8Array {
  const blocks: number[][] = [];
  for (const g of groups) {
    if (!g.colors.length) continue;
    if (g.name !== null) blocks.push(block(0xc001, nameBytes(g.name)));
    for (const c of g.colors) blocks.push(colorBlock(c));
    if (g.name !== null) blocks.push(block(0xc002, []));
  }
  const n = blocks.length;
  return Uint8Array.from([0x41, 0x53, 0x45, 0x46, 0, 1, 0, 0, (n >>> 24) & 255, (n >>> 16) & 255, (n >>> 8) & 255, n & 255, ...blocks.flat()]);
}

const round3 = (v: number) => Math.round(v * 1000) / 1000;

export function readAse(bytes: Uint8Array): AseGroup[] {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  if (bytes.length < 12 || String.fromCharCode(...bytes.slice(0, 4)) !== 'ASEF') throw new Error('not an Adobe Swatch Exchange file');
  const count = v.getUint32(8);
  const groups: AseGroup[] = [];
  let current: AseGroup | null = null;
  const into = (): AseGroup => {
    if (!current) { current = { name: null, colors: [] }; groups.push(current); }
    return current;
  };
  const name = (at: number): [string, number] => {
    const n = v.getUint16(at);
    let s = '';
    for (let i = 0; i < n; i++) s += String.fromCharCode(v.getUint16(at + 2 + i * 2));
    return [s.replace(/\0+$/, ''), at + 2 + n * 2];
  };
  let at = 12;
  for (let b = 0; b < count; b++) {
    if (at + 6 > bytes.length) throw new Error('truncated swatch file');
    const type = v.getUint16(at), length = v.getUint32(at + 2), end = at + 6 + length;
    if (end > bytes.length) throw new Error('truncated swatch file');
    if (type === 0xc001) {
      current = { name: name(at + 6)[0], colors: [] };
      groups.push(current);
    } else if (type === 0xc002) {
      current = null;
    } else if (type === 0x0001) {
      const [label, p] = name(at + 6);
      const model = String.fromCharCode(...bytes.slice(p, p + 4));
      const f = (i: number) => v.getFloat32(p + 4 + i * 4);
      const spot = p + 4 + (model === 'CMYK' ? 16 : model === 'Gray' ? 4 : 12) + 2 <= end && v.getUint16(p + 4 + (model === 'CMYK' ? 16 : model === 'Gray' ? 4 : 12)) === 1;
      if (model === 'CMYK') into().colors.push({ name: label, space: 'CMYK', values: [0, 1, 2, 3].map((i) => round3(f(i) * 100)), spot });
      else if (model === 'RGB ') into().colors.push({ name: label, space: 'RGB', values: [0, 1, 2].map((i) => round3(f(i) * 255)), spot });
      else if (model === 'LAB ') into().colors.push({ name: label, space: 'LAB', values: [round3(f(0) * 100), round3(f(1)), round3(f(2))], spot });
      else if (model === 'Gray') into().colors.push({ name: label, space: 'CMYK', values: [0, 0, 0, round3((1 - f(0)) * 100)], spot });
    }
    at = end;
  }
  return groups.filter((g) => g.colors.length);
}
