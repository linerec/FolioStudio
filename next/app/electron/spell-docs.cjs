'use strict';
// Edit > Spelling > Check Spelling with Search = All Documents: a thin relay between the app windows. Every window is one document with an
// engine session of its own (main.cjs `contexts`), so the check cannot walk into the next document by itself. What the main process does
// here is only what a window cannot do: list the windows in the order their documents were opened, ask ANOTHER window's engine one of the
// read-only spelling queries, and hand the check over to another window (which comes to the front, as InDesign brings the next document's
// window forward). The words, the order inside a document and every edit stay in the engine and in the window that owns the document.
//
// Answers are structured values; the same three calls are what an MCP tool would make.

const HIDDEN_TEST = !!process.env.FOLIO_TEST_STEPS && process.env.FOLIO_TEST_HIDDEN === '1';

/** The only engine queries another window may be asked: both read the document and change nothing. */
const RELAYED = new Set(['spellCheck', 'spellSuggest']);

/** The windows with their documents, in the order they were opened (a window's id is its webContents id, which only grows). */
function windowList(contexts, appwin, caller) {
  return [...contexts.entries()]
    .filter(([, c]) => !!c && !!c.win && !c.win.isDestroyed())
    .sort((a, b) => a[0] - b[0])
    .map(([id]) => {
      const d = appwin.state.docs[id];
      return { window: id, title: d && typeof d.title === 'string' ? d.title : null, self: id === caller };
    });
}

function install({ ipcMain, contexts, request, appwin, log }) {
  const live = (ctx) => !!ctx && !!ctx.win && !ctx.win.isDestroyed();

  /** `[{window, title (null: no document), self}]`, the document open order. */
  ipcMain.handle('folio:spell-windows', (e) => windowList(contexts, appwin, e.sender.id));

  /** One read-only spelling query asked of window `id`'s engine: `{ ok: true, header }` or `{ ok: false, error }`. */
  ipcMain.handle('folio:window-query', async (_e, id, message) => {
    const ctx = contexts.get(Number(id));
    if (!live(ctx)) return { ok: false, error: 'unknownWindow' };
    if (!message || typeof message !== 'object' || !RELAYED.has(message.cmd)) return { ok: false, error: 'notRelayed' };
    try {
      const { header } = await request(message, ctx);
      return { ok: true, header };
    } catch (error) {
      return { ok: false, error: String((error && error.message) || error) };
    }
  });

  /** The check goes on in window `id`: it comes to the front and is told (`spell-handoff` event) with what the check carries. */
  ipcMain.handle('folio:spell-handoff', (e, id, payload) => {
    const ctx = contexts.get(Number(id));
    if (!live(ctx) || ctx.win.webContents.isDestroyed()) return { ok: false, error: 'unknownWindow' };
    const shown = appwin.command({ cmd: 'activate', doc: Number(id) }, e.sender.id);
    if (!shown.ok) return shown;
    // the tab shows (above); a window of its own also comes to the front. A test run (hidden windows) never shows or focuses one.
    if (!HIDDEN_TEST) { if (ctx.win.isMinimized()) ctx.win.restore(); ctx.win.show(); ctx.win.focus(); }
    ctx.win.webContents.send('folio:event', { event: 'spell-handoff', handoff: payload ?? {}, from: e.sender.id });
    log(`spell handoff ${e.sender.id} -> ${id}`);
    return { ok: true, window: Number(id) };
  });

  return { windowList: (caller) => windowList(contexts, appwin, caller) };
}

module.exports = { install, RELAYED, windowList };
