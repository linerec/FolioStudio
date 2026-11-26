import { useState } from 'react';
import type { Command } from './commands';
import type { TextSelectionState } from './textedit';
import type { EditCommand, FoundText } from '../engine';
import { fontGlyphs } from '../features/glyphs/query';

/** One side of Find/Change ▸ Glyph: the face and the glyph id (null: not chosen yet). Memory only, like InDesign's findGlyphPreferences. */
export type GlyphSpec = { family: string; style: string; glyphId: number | null };
export type GlyphPrefs = { find: GlyphSpec; change: GlyphSpec };
export type FindDialogTab = 'text' | 'glyph';
export type GlyphAt = { family: string; style: string; glyphId: number | null; text: string; defaultFamily: string; defaultStyle: string };
export type GlyphScope = { scope: 'story' | 'document' | 'selection'; story?: string; start?: number; end?: number };

export const emptyGlyph = (): GlyphSpec => ({ family: '', style: '', glyphId: null });

/** The character at a story index, as Load Selected Glyph reads it (engine query `glyphAt`). */
export async function glyphAt(story: string, at: number): Promise<GlyphAt> {
  const { header } = await window.folio.request({ cmd: 'glyphAt', story, at });
  if (!header.ok) throw Error(header.error);
  const g = header.glyph as { family: string; style: string; glyphId: number | null; text: string; defaultFamily: string; defaultStyle: string };
  return g;
}

/** Find ▸ Glyph (engine query `glyphFind`): the characters that show glyph `glyphId` of the face. */
export async function findGlyph(spec: GlyphSpec & { glyphId: number }, where: GlyphScope): Promise<FoundText[]> {
  const { header } = await window.folio.request({ cmd: 'glyphFind', query: { family: spec.family, style: spec.style, glyphId: spec.glyphId, ...where } });
  if (!header.ok) throw Error(header.error);
  return header.matches as FoundText[];
}

/** Change ▸ Glyph (engine command `glyphReplace`). */
export function glyphReplaceCommand(find: GlyphSpec & { glyphId: number }, change: GlyphSpec & { glyphId: number }, where: GlyphScope): EditCommand {
  return { op: 'glyphReplace', find_family: find.family, find_style: find.style, find_glyph: find.glyphId, scope: where.scope, story: where.story ?? '',
    start: where.start ?? 0, end: where.end ?? 0, change_family: change.family, change_style: change.style, change_glyph: change.glyphId } as EditCommand;
}

/** The glyph a Unicode value reaches in a face (the glyph panel's own query), or null when the face does not map it. */
export async function glyphOfUnicode(family: string, style: string, codepoint: number): Promise<number | null> {
  const page = await fontGlyphs(family, style, 0, `U+${codepoint.toString(16).toUpperCase().padStart(4, '0')}`, 'mapped');
  return page.entries.find(e => e.unicode.includes(codepoint))?.id ?? null;
}

/** App-level state, so the context menu can load a glyph while the dialog is closed. */
export function useGlyphFind() {
  const [prefs, setPrefs] = useState<GlyphPrefs>({ find: emptyGlyph(), change: emptyGlyph() });
  const [tab, setTab] = useState<FindDialogTab>('text');
  return { prefs, setPrefs, tab, setTab };
}
export type GlyphFindState = ReturnType<typeof useGlyphFind>;

/** Text context menu ▸ Load Selected Glyph in Find / Change (measured: glyphfind-menu.json). One character must be selected.
 *  Find takes the face and glyph id and, when the Change face is not set, gives it the document's default face;
 *  Change takes the face and glyph id and leaves Find alone. */
export function glyphFindCommands(state: GlyphFindState, text: TextSelectionState | null, openDialog: () => void,
  report: (message: string) => void): Record<string, Partial<Command>> {
  const single = !!text && !text.nested && text.end - text.start === 1;
  const load = (which: 'find' | 'change') => async () => {
    if (!text) return;
    try {
      const g = await glyphAt(text.story, Math.min(text.start, text.end));
      const spec: GlyphSpec = { family: g.family, style: g.style, glyphId: g.glyphId };
      state.setPrefs(p => which === 'find'
        ? { find: spec, change: p.change.family ? p.change : { family: g.defaultFamily, style: g.defaultStyle, glyphId: null } }
        : { find: p.find, change: spec });
      state.setTab('glyph');
      openDialog();
    } catch (e) { report(String(e)); }
  };
  return {
    'id.ctx.text.loadSelectedGlyphInFind': { run: single ? load('find') : undefined },
    'id.ctx.text.loadSelectedGlyphInChange': { run: single ? load('change') : undefined },
  };
}

/** The Glyphs panel's right-click menu ▸ Load Glyph in Find / Change: the glyph picked in the panel goes into the Find (or Change) side of Find/Change ▸ Glyph
 *  and the dialog opens. Find gives the Change side the same face when it has none yet; Change leaves Find alone (the same rule as Load Selected Glyph). */
export function loadGlyphIntoFind(state: GlyphFindState, which: 'find' | 'change', g: { family: string; style: string; glyphId: number }, openDialog: () => void): void {
  const spec: GlyphSpec = { family: g.family, style: g.style, glyphId: g.glyphId };
  state.setPrefs(p => which === 'find'
    ? { find: spec, change: p.change.family ? p.change : { family: g.family, style: g.style, glyphId: null } }
    : { find: p.find, change: spec });
  state.setTab('glyph');
  openDialog();
}
