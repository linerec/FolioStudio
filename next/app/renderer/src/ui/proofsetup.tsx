// View ▸ Proof Colors and View ▸ Proof Setup (Document CMYK, Working CMYK, Custom…). The state belongs to the engine (the
// `proofSetup` query, engine crate server/proof_setup.rs), so a person and an LLM set it the same way; the screen shows it,
// asks the engine to change it and draws the pages again. The device and options are a per-computer user setting
// (never in a document); whether the proof is on belongs to the running window.
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { tr } from '../i18n';
import { useColorProfiles } from '../features/preferences/color-options';
import type { Command } from './commands';
import { PickField, PrefsDialog } from './prefsfields';

export type ProofSource = 'document' | 'working' | 'custom';
export type ProofSetup = {
  /** View ▸ Proof Colors. */
  enabled: boolean;
  source: ProofSource;
  /** Color Settings' working CMYK, passed on by the screen. */
  workingCmyk: string;
  customProfile: string;
  /** Custom…: Preserve CMYK Numbers. */
  preserveNumbers: boolean;
  /** Custom…: Simulate Black Ink. */
  simulateBlackInk: boolean;
  /** Custom…: Simulate Paper Color (implies Simulate Black Ink). */
  simulatePaperColor: boolean;
  /** Custom…: rendering intent of the conversion to the device (numbers not preserved). */
  intent: 'Perceptual' | 'Saturation' | 'RelativeColorimetric' | 'AbsoluteColorimetric';
};
export type ProofReport = { setup: ProofSetup; profile: { name: string; file: string | null; space?: string | null; status: string; error: string | null } };

export const PROOF_OFF: ProofSetup = {
  enabled: false, source: 'document', workingCmyk: 'U.S. Web Coated (SWOP) v2', customProfile: '',
  preserveNumbers: true, simulateBlackInk: false, simulatePaperColor: false, intent: 'RelativeColorimetric',
};

/** Reads the engine's proof setup, or changes the given fields of it (`{ cmd: 'proofSetup', set }`). */
export async function proofSetup(set?: Partial<ProofSetup>): Promise<ProofReport> {
  const { header } = await window.folio.request({ cmd: 'proofSetup', ...(set ? { set } : {}) });
  if (!header.ok) throw new Error(String(header.error ?? 'proofSetup failed'));
  return { setup: { ...PROOF_OFF, ...(header.setup as Partial<ProofSetup>) }, profile: header.profile as ProofReport['profile'] };
}

/** The proof state of the window and the menu commands that change it. `redraw` draws the pages again. */
export function useProofSetup({ hasDoc, workingCmyk, redraw }: { hasDoc: boolean; workingCmyk: string; redraw: () => void }) {
  const [setup, setSetup] = useState<ProofSetup>(PROOF_OFF);
  const [report, setReport] = useState<ProofReport | null>(null);
  const [dialog, setDialog] = useState(false);
  const wasOn = useRef(false);
  const last = useRef('');
  const apply = useCallback(async (set: Partial<ProofSetup>): Promise<ProofReport | null> => {
    try {
      const next = await proofSetup(set);
      const key = JSON.stringify(next.setup);
      const changed = key !== last.current;
      last.current = key;
      setSetup(next.setup);
      setReport(next);
      // the pages are drawn again when the look may have changed (the proof is on, or was)
      if (changed && (wasOn.current || next.setup.enabled)) redraw();
      wasOn.current = next.setup.enabled;
      return next;
    } catch (e) { console.error('proof setup', String(e)); return null; }
  }, [redraw]);
  // the engine learns Color Settings' working CMYK (and tells what it keeps)
  useEffect(() => { void apply({ workingCmyk }); }, [workingCmyk, apply]);
  // the engine's state may be changed from outside the menus (an LLM through the same query): read it again when the window is used
  useEffect(() => {
    const again = () => void apply({});
    window.addEventListener('focus', again);
    return () => window.removeEventListener('focus', again);
  }, [apply]);
  const on = (source: ProofSource) => () => void apply({ source, enabled: true });
  const handlers = useMemo((): Record<string, Partial<Command>> => ({
    'view.proofColors': { run: hasDoc ? () => void apply({ enabled: !setup.enabled }) : undefined, checked: setup.enabled },
    'id.view.proofSetup.documentCmyk': { run: hasDoc ? on('document') : undefined, checked: setup.source === 'document' },
    'id.view.proofSetup.workingCmyk': { run: hasDoc ? on('working') : undefined, checked: setup.source === 'working' },
    'id.view.proofSetup.custom': { run: hasDoc ? () => setDialog(true) : undefined, checked: setup.source === 'custom' },
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }), [hasDoc, setup, apply]);
  return { setup, report, handlers, dialog, closeDialog: () => setDialog(false), apply };
}

/** View ▸ Proof Setup ▸ Custom… (InDesign: Customize Proof Condition). */
export function ProofSetupDialog({ setup, documentCmyk, apply, onClose }: {
  setup: ProofSetup; documentCmyk: string | null; apply: (set: Partial<ProofSetup>) => Promise<ProofReport | null>; onClose: () => void;
}) {
  const installed = useColorProfiles();
  const first = setup.customProfile || documentCmyk || setup.workingCmyk;
  const [device, setDevice] = useState(first);
  const [preserve, setPreserve] = useState(setup.preserveNumbers);
  const [black, setBlack] = useState(setup.simulateBlackInk);
  const [paper, setPaper] = useState(setup.simulatePaperColor);
  // InDesign 2025 has no rendering intent box here (a warning sign stands next to the unchecked Preserve box): the conversion keeps the setup's intent
  const intent = setup.intent;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  // the device may be a CMYK, RGB or gray profile; its colour space names the "Preserve … Numbers" box (InDesign: CMYK / RGB / Color)
  const space = installed.profiles.find((p) => p.name === device)?.space ?? 'CMYK';
  const deviceOptions = useMemo((): [string, string][] => [...new Set([device, ...installed.profiles.map((p) => p.name)])].filter(Boolean).sort().map((n) => [n, n]), [device, installed.profiles]);
  // Preserve CMYK Numbers means something only for a device other than the document's own profile
  const sameAsDocument = space === 'CMYK' && device === documentCmyk;
  const submit = async () => {
    if (busy) return;
    setBusy(true); setError('');
    try {
      const r = await apply({ source: 'custom', customProfile: device, preserveNumbers: sameAsDocument ? true : preserve, simulateBlackInk: black || paper, simulatePaperColor: paper, intent, enabled: true });
      if (r?.profile.error) { setError(r.profile.error); setBusy(false); } else onClose();
    } catch (e) { setError(String(e)); setBusy(false); }
  };
  return <PrefsDialog title={tr('교정 조건 사용자 정의')} className="proof-setup-dialog" busy={busy} error={error || installed.error} onClose={onClose} onSubmit={() => void submit()}>
    <PickField name="proofDevice" label={tr('시뮬레이션할 장치')} value={device} set={setDevice} options={deviceOptions} />
    <label className="page-check"><input type="checkbox" data-field="proofPreserveNumbers" checked={sameAsDocument || preserve} disabled={sameAsDocument} onChange={(e) => setPreserve(e.target.checked)} />{space === 'RGB' ? tr('RGB 값 유지') : space === 'GRAY' ? tr('색상 값 유지') : tr('CMYK 값 유지')}</label>
    {space === 'CMYK' && !(sameAsDocument || preserve) && <p className="proof-warning" data-field="proofConvertWarning">{'⚠ '}{tr('값 유지를 끄면 문서의 CMYK 수치가 장치의 수치로 변환됩니다(상대 색도계).')}</p>}
    <fieldset disabled={busy}>
      <legend>{tr('화면 표시 옵션')}</legend>
      <label className="page-check"><input type="checkbox" data-field="proofSimulatePaper" checked={paper} onChange={(e) => setPaper(e.target.checked)} />{tr('용지 색상 시뮬레이션')}</label>
      <label className="page-check"><input type="checkbox" data-field="proofSimulateBlack" checked={black || paper} disabled={paper} onChange={(e) => setBlack(e.target.checked)} />{tr('검정 잉크 시뮬레이션')}</label>
    </fieldset>
    <p>{tr('이 설정은 이 컴퓨터의 사용자 설정이며 문서에는 저장되지 않습니다. 장치는 CMYK·RGB·회색 프로필을 고를 수 있습니다.')}</p>
  </PrefsDialog>;
}
