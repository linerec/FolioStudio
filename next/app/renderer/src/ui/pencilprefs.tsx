// Pencil Tool Preferences (InDesign: a double click on the Pencil tool): Fidelity, Smoothness and "Edit selected paths" with its distance.
// They are application settings (the values go to the user settings, Settings.preferences, not to a document); the Pencil reads them when a
// stroke ends and sends them to the engine's `pencilStroke` as plain arguments (fidelity, smoothness, within_px, selected), so a script or an
// LLM can send other values. Defaults are the ones InDesign 2025 installs and pencil-c12 measured: Fidelity 2.5 px, Smoothness 0 %, edit within 12 px.
import { useState } from 'react';
import { tr } from '../i18n';
import type { PrefValue } from '../features/preferences/fields';
import { CheckField, NumField, PrefsDialog } from './prefsfields';
import { real } from './prefsvalues';

export type PencilPrefs = { fidelity: number; smoothness: number; editSelected: boolean; within: number };

export const PENCIL_DEFAULTS: PencilPrefs = { fidelity: 2.5, smoothness: 0, editSelected: true, within: 12 };

const KEY = { fidelity: 'pencilToolPreferences.fidelity', smoothness: 'pencilToolPreferences.smoothness', editSelected: 'pencilToolPreferences.editSelected', within: 'pencilToolPreferences.within' } as const;

const inRange = (v: unknown, lo: number, hi: number, fallback: number) => (typeof v === 'number' && Number.isFinite(v) && v >= lo && v <= hi ? v : fallback);

/** The Pencil's settings out of the user settings; a value that is missing or out of range is the installed one. */
export function pencilPrefs(preferences: Record<string, PrefValue> | undefined): PencilPrefs {
  const p = preferences ?? {};
  const d = PENCIL_DEFAULTS;
  return {
    fidelity: inRange(p[KEY.fidelity], 0.5, 20, d.fidelity),
    smoothness: inRange(p[KEY.smoothness], 0, 100, d.smoothness),
    editSelected: typeof p[KEY.editSelected] === 'boolean' ? (p[KEY.editSelected] as boolean) : d.editSelected,
    within: inRange(p[KEY.within], 1, 20, d.within),
  };
}

/** The user settings with the Pencil's values put in. */
export function withPencilPrefs(preferences: Record<string, PrefValue> | undefined, v: PencilPrefs): Record<string, PrefValue> {
  return { ...preferences, [KEY.fidelity]: v.fidelity, [KEY.smoothness]: v.smoothness, [KEY.editSelected]: v.editSelected, [KEY.within]: v.within };
}

export function PencilPrefsDialog({ value, onSave, onClose }: { value: PencilPrefs; onSave: (v: PencilPrefs) => Promise<void>; onClose: () => void }) {
  const [fidelity, setFidelity] = useState(String(value.fidelity));
  const [smoothness, setSmoothness] = useState(String(value.smoothness));
  const [editSelected, setEditSelected] = useState(value.editSelected);
  const [within, setWithin] = useState(String(value.within));
  const [busy, setBusy] = useState(false), [error, setError] = useState('');
  const submit = async () => {
    if (busy) return;
    const f = real(tr('충실도'), fidelity, 0.5, 20), s = real(tr('부드러움'), smoothness, 0, 100), w = real(tr('선택한 패스 편집 거리'), within, 1, 20);
    for (const v of [f, s, w]) if (typeof v === 'string') { setError(v); return; }
    setBusy(true);
    try { await onSave({ fidelity: f as number, smoothness: s as number, editSelected, within: w as number }); onClose(); } catch (e) { setError(String(e)); } finally { setBusy(false); }
  };
  return <PrefsDialog title={tr('연필 도구 환경 설정')} className="pencil-prefs-dialog" busy={busy} error={error} onClose={onClose} onSubmit={() => void submit()}>
    <fieldset><legend>{tr('허용 오차')}</legend>
      <NumField label={tr('충실도')} value={fidelity} set={setFidelity} name="fidelity" suffix={tr('픽셀')} spec="number" min={0.5} max={20} />
      <NumField label={tr('부드러움')} value={smoothness} set={setSmoothness} name="smoothness" suffix="%" spec="number" min={0} max={100} />
    </fieldset>
    <fieldset><legend>{tr('옵션')}</legend>
      <CheckField label={tr('선택한 패스 편집')} value={editSelected} set={setEditSelected} name="editSelected" />
      <NumField label={tr('다음 거리 안에서')} value={within} set={setWithin} name="within" suffix={tr('픽셀')} spec="number" min={1} max={20} />
    </fieldset>
  </PrefsDialog>;
}
