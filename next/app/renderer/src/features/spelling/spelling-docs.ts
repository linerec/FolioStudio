// Check Spelling > Search: All Documents. Every app window holds one document with an engine of its own, so the walk from one document to the
// next goes through the main process (electron/spell-docs.cjs): it lists the windows, asks another window's engine the read-only query
// `spellCheck`, and hands the check over to that window. What InDesign 2025 does (artifacts/spell-alldocs-c1-20261023/STATUS.ko.md): the
// documents are walked in the order they were opened, round once from the one the check started in; a document without a flagged word is
// passed over; the next document's window comes to the front and its first word is selected; after the last one the dialog says the check is
// complete and stays on that document.
import type { SpellIssue, SpellOptions, UserDictionaries } from './spelling';

/** `window.folio.spellWindows()`: the app windows in the order their documents were opened (`title` null: the window holds no document). */
export type SpellWindow = { window: number; title: string | null; self: boolean };

/** What the check carries to the window it goes on in (the words a person told the check to skip, the Case Sensitive box, the settings in force). */
export type SpellHandoff = { rest: number[]; direction: 'forward' | 'backward'; ignored: string[]; caseSensitive: boolean; options: SpellOptions; userDictionaries: UserDictionaries };

/** The windows that follow this one, in the order their documents were opened, and then the ones before it: one round, this window's own document last.
 * Going backwards the walk takes the previous document first (the same round the other way; InDesign 2025, Direction: Backward). */
export function documentRing(windows: SpellWindow[], direction: 'forward' | 'backward' = 'forward'): number[] {
  const open = windows.filter(w => w.self || w.title !== null);
  const me = open.findIndex(w => w.self);
  const forward = me < 0 ? open.map(w => w.window) : [...open.slice(me + 1), ...open.slice(0, me)].map(w => w.window);
  return direction === 'backward' ? forward.reverse() : forward;
}

/** The next window in `rest` whose document has a flagged word (the ones without any are passed over), and what is left after it; null when none has. */
export async function nextDocumentWithIssues(rest: number[], args: Record<string, unknown>,
  ask: (window: number, message: { cmd: string; [arg: string]: unknown }) => Promise<{ ok: boolean; header?: Record<string, unknown>; error?: string }>): Promise<{ window: number; after: number[] } | null> {
  for (let i = 0; i < rest.length; i++) {
    const r = await ask(rest[i], { cmd: 'spellCheck', scope: 'document', ...args });
    const issues = r.ok && r.header && r.header.ok !== false ? (r.header.issues as SpellIssue[] | undefined) ?? [] : [];
    if (issues.length) return { window: rest[i], after: rest.slice(i + 1) };
  }
  return null;
}
