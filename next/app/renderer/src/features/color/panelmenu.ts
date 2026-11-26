// Color panel menu: the slider mode (CMYK, RGB, HSB, Lab) and Add to Swatches. The mode follows
// the colour shown (its own space) until one is chosen from the menu; the choice holds while the
// same object (and fill or stroke) is shown, so editing in HSB stays in HSB. This is panel state,
// not a setting kept in documents or user settings.
import { useCallback, useEffect, useState } from 'react';
import { t as tr } from '../../i18n';
import type { EditCommand, EditResult } from '../../engine';
import type { Command } from '../../ui/commands';
import type { ColorMode } from './convert';

export function useColorPanelMenu({ editable, edit }: { editable: boolean; edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null> }) {
  const [choice, setChoice] = useState<ColorMode | null>(null);
  // what the panel shows now: the colour's own mode, and the unnamed colour Add to Swatches would name
  const [seen, setSeen] = useState<{ native: ColorMode; add: string | null; key: string }>({ native: 'cmyk', add: null, key: '' });
  const publish = useCallback((native: ColorMode, add: string | null, key: string) => setSeen((s) => (s.native === native && s.add === add && s.key === key ? s : { native, add, key })), []);
  // another object, or its other paint: the panel follows the colour shown again
  useEffect(() => setChoice(null), [seen.key]);
  const mode = choice ?? seen.native;
  const handlers: Record<string, Partial<Command>> = {};
  for (const m of ['hsb', 'lab', 'cmyk', 'rgb'] as const) handlers[`id.panel.color.${m}`] = { run: () => setChoice(m), checked: mode === m };
  const add = seen.add;
  handlers['id.panel.color.addToSwatches'] = { run: editable && add ? () => void edit(tr('색상 견본에 추가'), [{ op: 'addUnnamedColors', only: [add] }]) : undefined };
  return { mode, publish, handlers };
}
export type ColorPanelMenu = ReturnType<typeof useColorPanelMenu>;
