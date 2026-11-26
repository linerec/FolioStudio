// Generated: cargo run -p folio-text --example export_glyph_types
// Do not edit; regenerate from the Rust registry. Keep LF line endings.
export type GlyphFilter = 'all' | 'mapped' | 'unmapped' | 'alternates';
export type GlyphRequest = { family: string; style: string; offset: number; limit: number; search: string; filter: GlyphFilter; text: string; order: string; show: string };
export type GlyphShowItem = { id: string; name: string; group: number };
export type GlyphEntry = { id: number; unicode: Array<number>; name: string | null; insertable: Array<number>; path: string; bounds: [number, number, number, number] | null; advance: number; features: Array<string> };
export type GlyphPage = { family: string; style: string; postscript: string; glyphCount: number; total: number; offset: number; limit: number; unitsPerEm: number; entries: Array<GlyphEntry>; showItems: Array<GlyphShowItem> };
