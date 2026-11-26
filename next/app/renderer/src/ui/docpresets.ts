// Document presets (File ▸ Document Presets): the New Document dialog's settings under a name, kept in
// the user's settings (the app's data folder, never in a project; the engine saves and deletes them: server/src/document_presets.rs). Lengths are in points. The built-in
// [Default] is US Letter in inches with 0.5 in margins, as InDesign's own default for a US user.
export type DocumentPreset = {
  name: string;
  /** The ruler units the document starts with (and the dialog shows). */
  unit: string;
  width: number;
  height: number;
  pages: number;
  start: number;
  facing: boolean;
  top: number;
  bottom: number;
  inside: number;
  outside: number;
  columns: number;
  gutter: number;
  /** Top, bottom, inside, outside. */
  bleed: number[];
  slug: number[];
};

export const DEFAULT_PRESET: DocumentPreset = {
  name: '', unit: 'Inches', width: 612, height: 792, pages: 1, start: 1, facing: true,
  top: 36, bottom: 36, inside: 36, outside: 36, columns: 1, gutter: 12, bleed: [0, 0, 0, 0], slug: [0, 0, 0, 0],
};

/** The settings file may hold anything: keep what is a preset. */
export function cleanPresets(raw: unknown): DocumentPreset[] {
  if (!Array.isArray(raw)) return [];
  const num = (v: unknown, d: number) => (typeof v === 'number' && Number.isFinite(v) ? v : d);
  const four = (v: unknown) => (Array.isArray(v) && v.length === 4 ? v.map((x) => num(x, 0)) : [0, 0, 0, 0]);
  return raw.filter((p): p is Record<string, unknown> => !!p && typeof p === 'object' && typeof (p as { name?: unknown }).name === 'string' && (p as { name: string }).name.trim() !== '')
    .map((p) => ({
      name: String(p.name).trim(), unit: typeof p.unit === 'string' ? p.unit : DEFAULT_PRESET.unit,
      width: num(p.width, 612), height: num(p.height, 792), pages: Math.round(num(p.pages, 1)), start: Math.round(num(p.start, 1)), facing: p.facing !== false,
      top: num(p.top, 36), bottom: num(p.bottom, 36), inside: num(p.inside, 36), outside: num(p.outside, 36),
      columns: Math.round(num(p.columns, 1)), gutter: num(p.gutter, 12), bleed: four(p.bleed), slug: four(p.slug),
    }));
}
