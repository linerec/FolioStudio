import type { Command } from '../../ui/commands';
import { AUTOCORRECT_KEY, DYNAMIC_KEY } from './spelling';

/** Edit ▸ Spelling. The two switches live in the computer's user settings (like InDesign's
 * `app.spellPreferences.dynamicSpellCheck` and `app.autoCorrectPreferences.autoCorrect`): they
 * flip at once, need no document, and are shared by every document. */
export function spellingCommands({ hasDocument, prefs, setPref, openCheck, openUserDictionary }: {
  hasDocument: boolean; prefs: Record<string, unknown> | undefined; setPref: (key: string, value: boolean) => void;
  openCheck: () => void; openUserDictionary: () => void;
}): Record<string, Partial<Command>> {
  const dynamic = prefs?.[DYNAMIC_KEY] === true, autocorrect = prefs?.[AUTOCORRECT_KEY] === true;
  return {
    // Fields, labels and the order of steps were compared with InDesign 2026 and 2025 (artifacts/spell-c2-20261023, inputtype-c1-20261023, spell-alldocs-c1-20261023).
    // Search: All Documents walks the open documents in the order they were opened (electron/spell-docs.cjs hands the check from window to window).
    'edit.spelling': { run: hasDocument ? openCheck : undefined },
    // The squiggle under the words that are not in the dictionary is drawn by the document view (dynamic-underline.tsx), screen only.
    // The squiggle under the words that are not in the dictionary is drawn by the document view (dynamic-underline.tsx), screen only.
    'id.edit.spelling.dynamicSpelling': { run: () => setPref(DYNAMIC_KEY, !dynamic), checked: dynamic },
    'id.edit.spelling.autocorrect': { run: () => setPref(AUTOCORRECT_KEY, !autocorrect), checked: autocorrect },
    'id.edit.spelling.userDictionary': { run: openUserDictionary },
  };
}
