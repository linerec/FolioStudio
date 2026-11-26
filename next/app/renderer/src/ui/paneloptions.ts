// Panel menus: Show Options / Hide Options. A panel's extra rows fold away and open again;
// the choice is this computer's setting (user settings, never the project folder).
import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';
import type { Command } from './commands';

const KEY = 'folio.panelOptions';

/** Panels whose menu has Show Options. Info also has its content-offset setting. */
export const OPTION_PANELS = ['stroke', 'color', 'gradient', 'align', 'textWrap', 'effects', 'swatches', 'character', 'paragraph'] as const;
export type OptionPanel = (typeof OPTION_PANELS)[number];
/** true: the extra rows are shown (InDesign's default). */
export type PanelOptions = Record<OptionPanel, boolean>;

function stored(value: string | null): PanelOptions {
  const all = Object.fromEntries(OPTION_PANELS.map((p) => [p, true])) as PanelOptions;
  try {
    const v = JSON.parse(value ?? 'null');
    if (v && typeof v === 'object') for (const p of OPTION_PANELS) if (v[p] === false) all[p] = false;
  } catch { /* this computer only */ }
  return all;
}

/** The command id of a panel's option toggle ("Hide Options" is the swatches menu's name for it). */
const command = (p: OptionPanel) => (p === 'swatches' ? 'id.panel.swatches.hideOptions' : `id.panel.${p}.showOptions`);

export function usePanelOptions(): { options: PanelOptions; handlers: Record<string, Partial<Command>> } {
  const options = stored(useUserPreference(KEY));
  const toggle = (p: OptionPanel) => () => {
    const current = stored(userSettings.getItem(KEY));
    userSettings.setItem(KEY, JSON.stringify({ ...current, [p]: !current[p] }));
  };
  const handlers: Record<string, Partial<Command>> = {};
  for (const p of OPTION_PANELS) handlers[command(p)] = { run: toggle(p), checked: p === 'swatches' ? !options[p] : options[p] };
  return { options, handlers };
}
