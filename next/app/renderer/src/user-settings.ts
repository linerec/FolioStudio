import type { Settings } from './engine';
import rendererPreferences from '../../../shared/renderer-preferences.json';

// Keep the old storage strings: each setting's existing reader owns its defaults and parsing.
const KEYS = new Set([
  'folio.showHiddenSpreads', 'folio.theme', 'folio.units',
  ...['mode', 'guides', 'edges', 'hiddenChars', 'showNotes', 'threads', 'rulers', 'contentGrabber', 'control', 'tools'].map(name => `folio.view.${name}`),
  'folio.infoPanel', 'folio.ungroupRemembersLayers', 'folio.pasteRemembersLayers',
  'folio.preflight.all', 'folio.stepRepeat', 'folio.objectStyles.clearOverridesWhenApplying', 'folio.transformPanel',
  'folio.linksPanel.thumbnails',
  // the control bar's place and the groups it hides (build-c2, ui/controlbar.tsx)
  'folio.controlBar.dock', 'folio.controlBar.at', 'folio.controlBar.hidden',
  // the groups hidden in the character, paragraph and page bars (rest-c1)
  'folio.controlBar.hidden.character', 'folio.controlBar.hidden.paragraph', 'folio.controlBar.hidden.page',
  ...rendererPreferences,
]);
let preferences: NonNullable<Settings['preferences']> = {};
let ready = false;
let revision = 0;
const changedAt = new Map<string, number>();
const inFlight = new Map<string, string>();
const listeners = new Set<() => void>();
let stopEvents: (() => void) | undefined;

function replacePreferences(next: typeof preferences) {
  for (const key of KEYS) {
    if (preferences[key] !== next[key]) changedAt.set(key, ++revision);
  }
  preferences = next;
  listeners.forEach(listener => listener());
}

async function request(values: Record<string, unknown>) {
  const { header } = await window.folio.request({ cmd: 'userSettings', ...values });
  if (!header.ok) throw new Error(String(header.error));
  return header.settings as Settings;
}

export async function initializeUserSettings() {
  const migrate: Record<string, string> = {};
  for (const key of KEYS) {
    try { const value = localStorage.getItem(key); if (value !== null) migrate[key] = value; } catch { /* Storage can be unavailable. */ }
  }
  try {
    const settings = await request({ migrate });
    preferences = { ...settings.preferences };
    ready = true;
    stopEvents?.();
    stopEvents = window.folio.onEvent(event => {
      if (event.event !== 'user-settings') return;
      const detail = event.detail as {settings:Settings};
      replacePreferences({ ...detail.settings.preferences, ...Object.fromEntries(inFlight) });
    });
    for (const [key, value] of Object.entries(migrate)) {
      try { if (localStorage.getItem(key) === value) localStorage.removeItem(key); } catch { /* A later startup can retry. */ }
    }
  } catch (error) { console.error('user settings migration', error); }
  listeners.forEach(listener => listener());
}

/** Dynamic panel flags also use this adapter; UI-only keys stay in localStorage. */
export const userSettings = {
  subscribe(listener: () => void) {
    listeners.add(listener);
    return () => { listeners.delete(listener); };
  },
  getItem(key: string): string | null {
    if (!KEYS.has(key) || !ready) {
      try { return localStorage.getItem(key); } catch { return null; }
    }
    const value = preferences[key];
    return value === undefined ? null : String(value);
  },
  setItem(key: string, value: string) {
    if (!KEYS.has(key) || !ready) {
      try { localStorage.setItem(key, value); } catch { /* Storage can be unavailable. */ }
      listeners.forEach(listener => listener());
      return;
    }
    if (preferences[key] === value) return;
    preferences[key] = value;
    inFlight.set(key, value);
    changedAt.set(key, ++revision);
    listeners.forEach(listener => listener());
    void request({ values: { [key]: value } }).catch(error => {
      // Retain an unsaved value if the engine write fails.
      if (preferences[key] === value) { try { localStorage.setItem(key, value); } catch { /* Storage can be unavailable. */ } }
      console.error('user settings save', error);
    }).finally(() => { if (inFlight.get(key) === value) inFlight.delete(key); });
  },
};

export async function saveUserSettings(patch: Partial<Settings>, previous: Settings): Promise<Settings> {
  const startedAt = revision;
  if (ready && patch.preferences) {
    const next = { ...patch.preferences };
    for (const key of KEYS) {
      if (next[key] === previous.preferences?.[key]) {
        if (preferences[key] === undefined) delete next[key];
        else next[key] = preferences[key];
      }
    }
    patch = { ...patch, preferences: next };
  }
  const saved = await window.folio.saveSettings(patch);
  const latest = { ...saved.preferences };
  for (const [key, changed] of changedAt) {
    if (changed > startedAt) latest[key] = preferences[key];
  }
  replacePreferences(latest);
  return saved;
}
