import type {GlyphRequest,GlyphPage,GlyphFilter} from '../../generated/glyphs';
/** A glyph in a glyph set: its face, glyph id, the Unicode value it goes in with (none: by id), and whether it goes in in its own face. */
export type SetGlyph={family:string;style:string;id:number;cp:number|null;rememberFont:boolean};
/** A user glyph set (kept in the user settings by the engine: `glyphSets`). `order`: where an added glyph goes. */
export type GlyphSet={name:string;order:'front'|'end'|'unicode';glyphs:SetGlyph[]};
export type GlyphSetsReply={sets:GlyphSet[];created?:string;written?:{path:string;sets:number};imported?:string[]};
/** Glyphs panel ▸ glyph sets: list, create, edit, delete, add, remove, export, import (engine server/glyph_sets.rs). */
export async function glyphSets(request:Record<string,unknown>):Promise<GlyphSetsReply> {
  const {header}=await window.folio.request({cmd:'glyphSets',...request});
  if(!header.ok)throw Error(String(header.error));
  return header as unknown as GlyphSetsReply;
}
export type {GlyphEntry,GlyphPage,GlyphFilter} from '../../generated/glyphs';
export async function fontGlyphs(family:string,style:string,offset:number,search:string,filter:GlyphFilter,text='',order='unicode',show=''):Promise<GlyphPage> {
  const args:GlyphRequest={family,style,offset,limit:64,search,filter,text,order,show};
  const {header}=await window.folio.request({cmd:'fontGlyphs',...args});
  if(!header.ok)throw Error(header.error);
  return header.page as GlyphPage;
}
