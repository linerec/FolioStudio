// The colour the Smart Guides are drawn in: Preferences ▸ Guides & Pasteboard ▸ Color ▸ Smart Guides (InDesign: app.smartGuidePreferences.guideColor,
// one of its interface colours, GRID_GREEN by default; measured in oracle/measured/smart-guide-prefs-2025.json, rules in oracle/rules/smartguides-c3.ko.md).
// Every interface colour is drawn in the pixel value InDesign 2025 puts on the screen for it (all 37 measured with a real drag and a screenshot while
// the button was held: the engine's query `uiColors`, field `indesign`). The page-middle line keeps its own colour whatever is chosen.
import { useEffect, useState } from 'react';
import { SMART_COLOR } from './smartguides';

type RGB = [number, number, number];
export const SMART_GUIDE_COLOR = 'GRID_GREEN';

let table: Record<string, RGB> | null = null;
let loading: Promise<void> | null = null;

/** The colours InDesign draws the interface colours in, by name as the preference spells it (GRID_GREEN); asked of the engine once. */
function load(): Promise<void> {
  loading ??= (async () => {
    if (!window.folio?.request) return;
    const { header } = await window.folio.request({ cmd: 'uiColors' });
    const rows = (header as unknown as { colors?: { name: string; indesign: RGB | null }[] }).colors;
    if (!rows) return;
    const out: Record<string, RGB> = {};
    for (const { name, indesign: rgb } of rows) if (rgb) out[name.replace(/([a-z])([A-Z])/g, '$1_$2').toUpperCase()] = rgb;
    table = out;
  })().catch(() => { loading = null; });
  return loading;
}

export function useUiColorTable(): Record<string, RGB> | null {
  const [t, setT] = useState(table);
  useEffect(() => {
    if (table) { setT(table); return; }
    let alive = true;
    void load().then(() => { if (alive && table) setT(table); });
    return () => { alive = false; };
  }, []);
  return t;
}

/** The stroke colour of the object lines, arrows and brackets for the chosen interface colour (a name like `RED`). */
export function smartLineColor(name: string | undefined, colors: Record<string, RGB> | null): string {
  if (!name || name === SMART_GUIDE_COLOR) return SMART_COLOR;
  const rgb = colors?.[name];
  return rgb ? `rgb(${rgb[0]},${rgb[1]},${rgb[2]})` : SMART_COLOR;
}
