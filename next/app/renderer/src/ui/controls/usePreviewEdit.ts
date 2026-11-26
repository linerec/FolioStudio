// `usePreviewEdit`: wires a number field's onPreview / onCommit / onCancel to the engine's preview session
// (previewsession.ts). The panel says only what a value means (`build`: the edit commands for a value); this hook does
// the rest: the preview shows them provisionally, a commit makes them one undo step, a cancel gives back the exact
// document. Without a provider (a dialog, a test) there is no preview and a commit is an ordinary edit.
import { createContext, useCallback, useContext, useRef } from 'react';
import type { EditCommand } from '../../engine';
import type { PreviewSession } from './previewsession';

/**
 * What the hook needs of a preview session: the window's own (`PreviewEditContext`), or a port a part of the window makes of it when
 * committing and cancelling need more than the engine's reply (the text editor keeps its cursor history: textedit.tsx `preview`).
 */
export type PreviewPort = Pick<PreviewSession, 'open' | 'apply' | 'commit' | 'cancel' | 'discard'>;

export const PreviewEditContext = createContext<PreviewSession | null>(null);

/**
 * Runs `fn` so that the edits it makes (through the window's ordinary edit function) go to the preview session as provisional
 * changes instead of being made: how a field whose value is turned into commands by the window's own handlers (the control
 * bar's move, resize, rotate) previews them. Provided by App.tsx; none outside it.
 */
export const PreviewScopeContext = createContext<((fn: () => void) => void) | null>(null);

export function usePreviewEdit(
  label: string,
  build: (value: number, base: number | null) => EditCommand[],
  edit: (label: string, commands: EditCommand[]) => unknown,
  /** The session to use: none given, the window's; `null`, no preview (a commit is an ordinary edit). */
  port?: PreviewPort | null,
): { onPreview: (v: number, base: number | null) => void; onCommit: (v: number) => unknown; onCancel: () => void } {
  const windowSession = useContext(PreviewEditContext);
  const session: PreviewPort | null = port === undefined ? windowSession : port;
  // the latest closures: a field keeps the callbacks it was given when a drag began
  const latest = useRef({ label, build, edit });
  latest.current = { label, build, edit };
  const shown = useRef<number | null>(null);
  const onPreview = useCallback((v: number, base: number | null) => {
    if (!session) return;
    shown.current = v;
    session.apply(latest.current.label, latest.current.build(v, base) as unknown as Record<string, unknown>[]);
  }, [session]);
  const onCommit = useCallback((v: number) => {
    const { label: l, build: b, edit: e } = latest.current;
    const same = shown.current !== null && Math.abs(shown.current - v) < 1e-9;
    shown.current = null;
    if (session?.open && same) return session.commit();
    // not what the session shows (typed after a preview): an ordinary edit, which the engine takes the preview back before
    session?.discard();
    return e(l, b(v, null));
  }, [session]);
  const onCancel = useCallback(() => {
    shown.current = null;
    void session?.cancel();
  }, [session]);
  return { onPreview, onCommit, onCancel };
}
