import { userSettings } from '../user-settings';
// Transform panel menu: Redefine Scaling as 100 % (the engine moves a scale in the object's
// transform into its path, edit/src/ops/scale.rs) and the four settings InDesign keeps for the
// application, not the document, so they are on this computer only and never in the project or
// the IDML (oracle basic-transform-prefs, which records them before each scenario):
//   Dimensions Include Stroke Weight, Transformations are Totals,
//   Adjust Stroke Weight when Scaling, Adjust Effects when Scaling.
import { useState } from 'react';
import { tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { ItemInfo, PageItems } from '../inspect';
import type { Command } from './commands';
import { findItem } from './overlay';

const KEY = 'folio.transformPanel';

export type TransformMenuPrefs = { includeStroke: boolean; totals: boolean; adjustStroke: boolean; adjustEffects: boolean };

/** InDesign 2025 as measured on the reference computer: stroke and effects follow a scale. */
export const DEFAULT_PREFS: TransformMenuPrefs = { includeStroke: true, totals: false, adjustStroke: true, adjustEffects: true };

/** The settings now (read where they are used, so a dialog sees what the menu just set). */
export function transformPrefs(): TransformMenuPrefs {
  try {
    const v = JSON.parse(userSettings.getItem(KEY) ?? 'null');
    if (v && typeof v === 'object') {
      const b = (k: keyof TransformMenuPrefs) => (typeof v[k] === 'boolean' ? (v[k] as boolean) : DEFAULT_PREFS[k]);
      return { includeStroke: b('includeStroke'), totals: b('totals'), adjustStroke: b('adjustStroke'), adjustEffects: b('adjustEffects') };
    }
  } catch { /* this computer only */ }
  return { ...DEFAULT_PREFS };
}

/** What a scale through the object's transform (the Scale dialog, Transform Again) passes the engine. */
export const scaleAdjust = (p: TransformMenuPrefs = transformPrefs()) => ({ stroke: p.adjustStroke, effects: p.adjustEffects });

type Sel = { page: number; id: string; content: boolean };
type Props = {
  selection: Sel[];
  editable: boolean;
  peek: (page: number) => PageItems | null | undefined;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};

/** A scale that is not 100 %: the lengths of the matrix's rows (its columns' scale is what the panel shows). */
const scaled = (m: ItemInfo['transform']) => Math.abs(Math.hypot(m[0], m[1]) - 1) > 1e-9 || Math.abs(Math.hypot(m[2], m[3]) - 1) > 1e-9;

export function useTransformMenu({ selection, editable, peek, edit }: Props): { prefs: TransformMenuPrefs; handlers: Record<string, Partial<Command>> } {
  const [prefs, setPrefs] = useState<TransformMenuPrefs>(transformPrefs);
  const toggle = (k: keyof TransformMenuPrefs) => () => setPrefs((o) => {
    const n = { ...o, [k]: !o[k] };
    try { userSettings.setItem(KEY, JSON.stringify(n)); } catch { /* this computer only */ }
    return n;
  });
  const objects = selection.filter((s) => !s.content);
  const ids = objects.map((s) => s.id);
  const items = objects.map((s) => findItem(peek(s.page), s.id)?.item).filter((x): x is ItemInfo => !!x);
  const all = editable && items.length > 0 && items.length === objects.length;
  return {
    prefs,
    handlers: {
      'id.panel.transform.redefineScalingAs100': { run: all && items.some((it) => scaled(it.transform)) ? () => void edit(tr('비율을 100%로 재정의'), [{ op: 'redefineScaling', ids }]) : undefined },
      // the control bar's and the panel's W and H count the stroke (off: the path's size)
      'id.panel.transform.dimensionsIncludeStrokeWeight': { run: toggle('includeStroke'), checked: prefs.includeStroke },
      // an item inside a group: its rotation, scale and shear in the panel (and typed) are its page totals (on) or relative to the group (off)
      'id.panel.transform.transformationsAreTotals': { run: toggle('totals'), checked: prefs.totals },
      'id.panel.transform.adjustStrokeWeightWhenScaling': { run: toggle('adjustStroke'), checked: prefs.adjustStroke },
      'id.panel.transform.adjustEffectsWhenScaling': { run: toggle('adjustEffects'), checked: prefs.adjustEffects },
    },
  };
}
