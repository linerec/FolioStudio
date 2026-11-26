import stepRepeat from '../../../../shared/dialogs/stepRepeat.json';
import { userSettings } from '../user-settings';
// Edit ▸ Duplicate and Edit ▸ Step and Repeat…, as InDesign does them: Duplicate copies the
// selection by the offset Step and Repeat last used, and Step and Repeat makes a row of copies,
// each moved from the one before, or a grid of them. Measured in
// next/oracle/scenarios/basic-duplicate2.json.

export type StepRepeat = { grid: boolean; count: number; rows: number; columns: number; h: number; v: number };

const KEY = 'folio.stepRepeat';
/** The values before Step and Repeat is first used (points). InDesign's own first values are not
 * measured yet: the InDesign that answered basic-duplicate2 remembered 50.75 / 0.016 pt from earlier
 * use, and a scripted duplicate does not change them (edit-duplicate-after-copy). */
export const STEP_DEFAULT: StepRepeat = Object.fromEntries(stepRepeat.fields.map(f=>[f.name,f.default])) as StepRepeat;

/** The values Step and Repeat last used on this computer (Edit ▸ Duplicate's offset). */
export function lastStepRepeat(): StepRepeat {
  try {
    const v = JSON.parse(userSettings.getItem(KEY) ?? 'null');
    if (v && typeof v === 'object') return { ...STEP_DEFAULT, ...v };
  } catch { /* per computer only */ }
  return STEP_DEFAULT;
}
