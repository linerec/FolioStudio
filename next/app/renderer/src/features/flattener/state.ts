import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { DocumentSummary, InkView, Settings } from '../../engine';
import { cleanFlatteners } from '../preferences/flattener-presets';

// Window ▸ Output ▸ Flattener Preview. The panel's state belongs to the engine (query `flattenerSettings`, application state: it stays when the
// panel is hidden or the document is closed); the engine draws the page (render with a `flattener` view) and says which controls are on.
// What stays here is when the preview is up to date (measured in InDesign 2025, rules/windowmenu-c2.ko.md): choosing a Highlight, pressing
// Refresh or turning Auto Refresh on draws it; an edit, or another Preset or Ignore Spread Overrides, leaves the live page half way to white
// until Refresh (with Auto Refresh on they redraw at once).

export const HIGHLIGHTS = ['none', 'complexRegions', 'transparent', 'affected', 'affectedGraphics', 'outlinedStrokes', 'outlinedText', 'rasterFill', 'rasterized'] as const;
export type Highlight = (typeof HIGHLIGHTS)[number];
export type FlattenerSettings = { highlight: Highlight; autoRefresh: boolean; preset: string; ignoreSpreadOverrides: boolean };
export type FlattenerEnabled = { highlight: boolean; autoRefresh: boolean; refresh: boolean; preset: boolean; ignoreSpreadOverrides: boolean; applySettingsToPrint: boolean };
/** What Apply Settings to Print wrote (the print dialog starts from it). */
export type PrintApply = { flattenerPresetName: string; ignoreSpreadOverrides: boolean } | null;

const START: FlattenerSettings = { highlight: 'none', autoRefresh: false, preset: '[High Resolution]', ignoreSpreadOverrides: false };
const OFF: FlattenerEnabled = { highlight: true, autoRefresh: false, refresh: false, preset: false, ignoreSpreadOverrides: false, applySettingsToPrint: false };

type Reply = { state: FlattenerSettings; enabled: FlattenerEnabled; printApply: PrintApply };

async function ask(body: Record<string, unknown>): Promise<Reply> {
  const { header } = await window.folio.request({ cmd: 'flattenerSettings', ...body });
  if (!header.ok) throw new Error(header.error ?? 'flattenerSettings failed');
  return header as unknown as Reply;
}

export function useFlattenerPreview(doc: DocumentSummary | null, info: unknown, settings: Settings) {
  const [state, setState] = useState<FlattenerSettings>(START);
  const [enabled, setEnabled] = useState<FlattenerEnabled>(OFF);
  const [printApply, setPrintApply] = useState<PrintApply>(null);
  const [valid, setValid] = useState(false);
  const [revision, setRevision] = useState(0);
  const take = useCallback((r: Reply) => { setState(r.state); setEnabled(r.enabled); setPrintApply(r.printApply ?? null); return r; }, []);
  useEffect(() => { ask({}).then(take, () => {}); }, [take]);

  const redraw = useCallback(() => { setValid(true); setRevision((n) => n + 1); }, []);
  /** Changes the engine's state; `how`: the preview is drawn again (`draw`), or only goes out of date unless Auto Refresh is on (`stale`). */
  const change = useCallback(async (set: Partial<FlattenerSettings>, how: 'draw' | 'stale' | 'keep') => {
    const r = take(await ask({ set }));
    if (how === 'draw' || (how === 'stale' && r.state.autoRefresh)) redraw();
    else if (how === 'stale') setValid(false);
  }, [take, redraw]);

  // an edit: the highlight follows it with Auto Refresh on, else the preview goes out of date
  const last = useRef<unknown>(info);
  useEffect(() => {
    if (last.current === info) return;
    last.current = info;
    if (state.highlight === 'none') return;
    if (state.autoRefresh) setRevision((n) => n + 1);
    else setValid(false);
  }, [info, state.highlight, state.autoRefresh]);
  useEffect(() => { setValid(false); }, [doc?.path]);

  const custom = useMemo(() => cleanFlatteners(settings.flattenerPresets), [settings.flattenerPresets]);
  const view = useMemo<InkView | null>(() => {
    if (!doc || state.highlight === 'none') return null;
    const own = custom.find((p) => p.name === state.preset);
    const preset = own ? { name: own.name, balance: own.rasterVectorBalance, strokes: own.convertAllStrokesToOutlines, text: own.convertAllTextToOutlines, clip: own.clipComplexRegions } : state.preset;
    return { mode: 'flattener', highlight: state.highlight, preset, ignoreSpreadOverrides: state.ignoreSpreadOverrides, revision, ...(valid ? {} : { stale: true }) };
  }, [doc, state, custom, revision, valid]);

  return {
    state, enabled, view, valid, printApply, customPresets: custom.map((p) => p.name),
    setHighlight: (highlight: Highlight) => change({ highlight }, 'draw'),
    setAutoRefresh: (autoRefresh: boolean) => change({ autoRefresh }, autoRefresh ? 'draw' : 'keep'),
    setPreset: (preset: string) => change({ preset }, 'stale'),
    setIgnoreSpreadOverrides: (ignoreSpreadOverrides: boolean) => change({ ignoreSpreadOverrides }, 'stale'),
    refresh: redraw,
    applyToPrint: async () => { take(await ask({ apply: true })); },
  };
}
export type FlattenerPreviewState = ReturnType<typeof useFlattenerPreview>;
