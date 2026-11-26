// Typing rules of the engine (Edit ▸ Spelling ▸ Autocorrect, and the document's typographer's quotes). The text editor asks this hook what typing a
// text at a place does; which characters finish a word, which language's table applies, how a word is replaced and which quote is curly are the
// engine's (query spellTyped, edit command typeText): the screen asks, hands back how many characters of the word before the caret were typed
// (`run`: a word that a script or a paste brought is not corrected), and when something changes it sends the typeText command the hook gives back
// (and puts the caret where the changed text ends). Words put in by a script, a paste or insertText are never corrected.
import type { RegisteredEditCommand } from '../../generated/edit-commands';

export type TypedFix = { command: RegisteredEditCommand; caretAt: number;
  /** a corrected word and the typed character after it, as two undo steps (InDesign: the first undo takes the typed character, the second the correction) */
  split?: { start: number; end: number; word: string; typed: string; typedFirst: boolean } };
export type TypedAutoCorrect = (story: string, at: number, typed: string, run: number) => Promise<{ run: number; fix: TypedFix | null }>;
export const typedAutoCorrect: { current: TypedAutoCorrect | null } = { current: null };
