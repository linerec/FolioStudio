// The window's side of the engine's preview session (engine crates/edit/src/preview.rs). A field that shows a value
// provisionally (a list item under the pointer, a value being dragged) sends `previewApply` with the commands the value
// would make; the engine shows them and takes them back exactly. This class only keeps the requests in order and
// coalesces them: while one is being answered, the ones that follow replace each other and only the last is sent.
// Pure TypeScript (no React, no engine import), so node can test it (next/app/scripts/test-numberfield.cjs).

export type PreviewReply = unknown;

/** Sends one edit request (the commands of one `previewApply`, `previewCommit` or `previewCancel`) and hands the reply on. */
export type PreviewSend = (label: string, commands: Record<string, unknown>[]) => Promise<PreviewReply>;

export class PreviewSession {
  /** A session is open in the engine (or a request that opens it is on its way). */
  open = false;
  private pending: { label: string; commands: Record<string, unknown>[]; onReply?: (reply: PreviewReply) => void } | null = null;
  private running: Promise<void> | null = null;

  constructor(private readonly send: PreviewSend) {}

  /**
   * Shows `commands` (replacing what the session showed). Many calls in a row cost one request each time the engine is free.
   * `onReply` hears the engine's reply to this request (not to one that was replaced before it was sent).
   */
  apply(label: string, commands: Record<string, unknown>[], onReply?: (reply: PreviewReply) => void): void {
    this.open = true;
    this.pending = { label, commands, onReply };
    this.pump();
  }

  private pump(): void {
    if (this.running || !this.pending) return;
    const { label, commands, onReply } = this.pending;
    this.pending = null;
    this.running = this.send(label, [{ op: 'previewApply', commands }]).then((reply) => { try { onReply?.(reply); } catch { /* the listener's trouble is not the session's */ } }, () => undefined).then(() => {
      this.running = null;
      this.pump();
    });
  }

  /** Resolves when nothing is being sent and nothing waits. */
  async idle(): Promise<void> {
    while (this.running || this.pending) await (this.running ?? Promise.resolve());
  }

  /** Makes the shown change permanent (one undo step). Resolves with the engine's reply; null when no session was open. */
  async commit(): Promise<PreviewReply | null> {
    if (!this.open) return null;
    await this.idle();
    this.open = false;
    return this.send('', [{ op: 'previewCommit' }]);
  }

  /** Takes the shown change back. A value that was waiting to be sent is never shown. */
  async cancel(): Promise<PreviewReply | null> {
    if (!this.open) return null;
    this.pending = null;
    await this.idle();
    this.open = false;
    return this.send('', [{ op: 'previewCancel' }]);
  }

  /** Forgets the session without a request: another edit follows at once and the engine takes the preview back before it. */
  discard(): void {
    this.pending = null;
    this.open = false;
  }
}
