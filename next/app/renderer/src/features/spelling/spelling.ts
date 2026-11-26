// Edit ▸ Spelling. The words are judged by the engine (queries spellCheck, spellSuggest,
// spellUserDictionary, spellAutoCorrect); this file only calls them and keeps the person's
// choices in the computer's user settings (settings.json), never in the project folder.
import type { Settings } from '../../engine';

export type SpellIssueKind = 'misspelled' | 'repeated' | 'uncapitalizedWord' | 'uncapitalizedSentence';
export type SpellIssue = { story: string; kind: SpellIssueKind; start: number; end: number; word: string; language: string; suggestions: string[] };
export type UserDictionary = { addedWords: string[]; removedWords: string[] };
export type UserDictionaries = Record<string, UserDictionary>;
/** `toEnd`: InDesign's "To End of Story" (asked of the engine as a selection that has no end). `allDocuments`: the check goes on in the next open
 * document when this one is done (each document is asked as the scope `document`; see spelling-docs.ts). */
export type SpellScope = 'allDocuments' | 'document' | 'story' | 'toEnd' | 'selection';
export type SpellOptions = { checkMisspelledWords: boolean; checkRepeatedWords: boolean; checkCapitalizedWords: boolean; checkCapitalizedSentences: boolean };

export const DYNAMIC_KEY = 'spellPreferences.dynamicSpellCheck';
export const AUTOCORRECT_KEY = 'autoCorrectPreferences.autoCorrect';

/** `app.spellPreferences` check switches from the user settings (InDesign's defaults: all on). */
export function spellOptions(prefs: Settings['preferences']): SpellOptions {
  const on = (key: string) => prefs?.[`spellPreferences.${key}`] !== false;
  return { checkMisspelledWords: on('checkMisspelledWords'), checkRepeatedWords: on('checkRepeatedWords'),
    checkCapitalizedWords: on('checkCapitalizedWords'), checkCapitalizedSentences: on('checkCapitalizedSentences') };
}

export function cleanUserDictionaries(value: unknown): UserDictionaries {
  const out: UserDictionaries = {};
  if (!value || typeof value !== 'object' || Array.isArray(value)) return out;
  const words = (v: unknown) => Array.isArray(v) ? v.filter((w): w is string => typeof w === 'string') : [];
  for (const [language, d] of Object.entries(value)) {
    const row = d as Partial<UserDictionary> | null;
    out[language] = { addedWords: words(row?.addedWords), removedWords: words(row?.removedWords) };
  }
  return out;
}

async function call<T>(request: Record<string, unknown>): Promise<T> {
  const { header } = await window.folio.request(request);
  if (!header.ok) throw new Error(header.error ?? 'spelling request failed');
  return header as unknown as T;
}

export const spellCheck = (args: { scope: Exclude<SpellScope, 'toEnd' | 'allDocuments'>; story?: string; start?: number; end?: number; options: SpellOptions;
  userDictionaries: UserDictionaries; ignored: string[]; suggestions?: boolean }) =>
  call<{ issues: SpellIssue[]; stories: number; truncated: boolean }>({ cmd: 'spellCheck', ...args });
export const spellSuggest = (language: string, word: string) => call<{ suggestions: string[] }>({ cmd: 'spellSuggest', language, word });
/** Edit ▸ Spelling ▸ User Dictionary…: the engine's app dialog (next/shared/dialogs/userDictionary.json), shown by the shared dialog frame. */
export const openUserDictionary = async (note: (message: string) => void) => {
  try { const { header } = await window.folio.request({ cmd: 'openDialog', dialog: 'userDictionary' }); if (!header.ok) note(String(header.error)); }
  catch (e) { note(String(e)); }
};
export const spellLanguages = () => call<{ languages: { name: string; code: string; checks: boolean }[] }>({ cmd: 'spellLanguages' });
export const spellUserDictionary = (op: 'add' | 'remove' | 'exclude' | 'include' | 'get', language: string, words: string[], userDictionaries: UserDictionaries, caseSensitive?: boolean) =>
  call<{ userDictionaries: UserDictionaries; language: string; addedWords: string[]; removedWords: string[] }>({ cmd: 'spellUserDictionary', op, language, words, userDictionaries, ...(caseSensitive === undefined ? {} : { caseSensitive }) });
export const spellAutoCorrect = (args: Record<string, unknown>) => call<{ tables?: Record<string, [string, string][]>; replacement?: string | null }>({ cmd: 'spellAutoCorrect', ...args });
/** What typing `text` at `at` in a story would change (nothing is written): the characters `start..end` become `replacement`, which holds the typed text; `run` is the
 * number of typed characters in the word before the caret afterwards (hand it back with the next typed text: a word nobody typed is not corrected). */
export const spellTyped = (args: { story: string; at: number; text: string; tables: Record<string, [string, string][]>; autoCorrectCapitalizationErrors: boolean;
  autoCorrect: boolean; typedRun: number }) =>
  call<{ fix: { start: number; end: number; replacement: string } | null; run: number }>({ cmd: 'spellTyped', ...args });
