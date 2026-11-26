import type { Command } from '../../ui/commands';
import type { EditCommand } from '../../engine';
import { tr } from '../../i18n';

// The ruler right-click menus (InDesign: Horizontal Ruler / Vertical Ruler / Zero Point context menus; menutree.ts CONTEXT_MENUS rulerH, rulerV,
// zeroPoint). Units and ruler origin are the document's ViewPreference, changed by the engine operation that Preferences ▸ Units & Increments
// uses (`setViewPref` horizontalUnits / verticalUnits / rulerOrigin). "Lock Zero Point" is an application setting (`folio.zeroPoint.locked`).

/** InDesign's unit names (menu id part, ViewPreference value). */
const UNITS: [string, string][] = [
  ['points', 'Points'], ['picas', 'Picas'], ['inches', 'Inches'], ['inchesDecimal', 'InchesDecimal'], ['millimeters', 'Millimeters'],
  ['centimeters', 'Centimeters'], ['ciceros', 'Ciceros'], ['agates', 'Agates'], ['pixels', 'Pixels'],
];
/** Ruler Per Page / Per Spread / On Spine (ViewPreference RulerOrigin). */
const ORIGINS: [string, string][] = [['rulerPerPage', 'PageOrigin'], ['rulerPerSpread', 'SpreadOrigin'], ['rulerOnSpine', 'SpineOrigin']];

export const ZERO_POINT_LOCKED_KEY = 'folio.zeroPoint.locked';

/** The app-level units override (Edit ▸ Preferences ▸ Units, "follow the document" or one unit for every document) hides the document's own units; a choice made in a ruler menu is a choice of the document's units, so it ends the override. */
export function rulerMenuCommands(p: {
  editable: boolean;
  /** The document's ViewPreference is known (a document with the engine's view info is open). */
  hasView: boolean;
  units: { horizontal: string; vertical: string };
  origin: string;
  locked: boolean;
  setLocked: (on: boolean) => void;
  followDocumentUnits: () => void;
  edit: (label: string, commands: EditCommand[]) => unknown;
}): Record<string, Partial<Command>> {
  const can = p.editable && p.hasView;
  const out: Record<string, Partial<Command>> = {};
  const set = (label: string, keys: string[], value: string) => () => {
    p.followDocumentUnits();
    void p.edit(label, keys.map((key) => ({ op: 'setViewPref', key, value }) as EditCommand));
  };
  for (const [slug, unit] of UNITS) {
    out[`id.ctx.rulerH.${slug}`] = { run: can ? set(tr('가로 눈금자 단위'), ['horizontalUnits'], unit) : undefined, checked: p.units.horizontal === unit };
    out[`id.ctx.rulerV.${slug}`] = { run: can ? set(tr('세로 눈금자 단위'), ['verticalUnits'], unit) : undefined, checked: p.units.vertical === unit };
    out[`id.ctx.zeroPoint.${slug}`] = { run: can ? set(tr('눈금자 단위'), ['horizontalUnits', 'verticalUnits'], unit) : undefined, checked: p.units.horizontal === unit && p.units.vertical === unit };
  }
  for (const [slug, origin] of ORIGINS) {
    out[`id.ctx.rulerH.${slug}`] = { run: can ? () => { void p.edit(tr('눈금자 원점'), [{ op: 'setViewPref', key: 'rulerOrigin', value: origin } as EditCommand]); } : undefined, checked: p.origin === origin };
  }
  out['id.ctx.zeroPoint.lockZeroPoint'] = { run: () => p.setLocked(!p.locked), checked: p.locked };
  // Toggle Measurement System (Ctrl+Alt+Shift+U, tools-c1): the next unit of the list, both rulers when they show the same unit, else the
  // horizontal one only (measured in InDesign: tablekeys-c1). InDesign's cycle also passes Custom, which Folio does not have: it is left out.
  const at = UNITS.findIndex(([, u]) => u === p.units.horizontal), next = UNITS[(at + 1) % UNITS.length][1];
  out['view.key.cycleUnits'] = { run: can ? set(tr('눈금자 단위'), p.units.horizontal === p.units.vertical ? ['horizontalUnits', 'verticalUnits'] : ['horizontalUnits'], next) : undefined };
  return out;
}
