'use strict';
// The shell side of Window > Arrange across documents: keeps the window state of appwin.cjs (a pure model), puts the real app
// windows where it says (bounds, shown or hidden), tells every window what its tab strip shows, and answers the commands.
//
// An app window is one document with an engine of its own (main.cjs). Tabs are windows kept at one rectangle with only the active
// one shown; tiles are windows set side by side inside that rectangle; floating windows are windows of their own. A page, its engine
// session, its undo steps and its unsaved changes are never touched: a window only moves, shows or hides.
//
// Test runs (FOLIO_TEST_STEPS with FOLIO_TEST_HIDDEN=1) open their windows hidden: this shell then moves them but never shows,
// hides, maximizes or focuses one, and records "shows" as a flag of its own, so a test cannot put windows on the person's screen.
const model = require('./appwin.cjs');

const same = (a, b) => !!a && !!b && a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height;

function install({ ipcMain, screen, contexts, log }) {
  let state = model.initialState();
  const hiddenTest = !!process.env.FOLIO_TEST_STEPS && process.env.FOLIO_TEST_HIDDEN === '1';
  const live = (ctx) => !!ctx && !!ctx.win && !ctx.win.isDestroyed();
  /** The model's doc ids are the windows' webContents ids (the keys of `contexts`). */
  const ctxOf = (doc) => contexts.get(doc);
  const areaOf = (ctx) => {
    try { const a = screen.getDisplayMatching(ctx.win.getBounds()).workArea; return { x: a.x, y: a.y, width: a.width, height: a.height }; }
    catch { const a = screen.getPrimaryDisplay().workArea; return { x: a.x, y: a.y, width: a.width, height: a.height }; }
  };
  /** Window events caused by this shell's own moves are not the person's. */
  let applying = 0;

  function tell() {
    for (const doc of Object.keys(state.docs)) {
      const ctx = ctxOf(Number(doc));
      if (live(ctx) && !ctx.win.webContents.isDestroyed()) ctx.win.webContents.send('folio:event', { event: 'appwin', view: model.view(state, Number(doc)) });
    }
  }

  /** Puts the windows where the state says: bounds first, then the ones that show now, then the ones that hide. */
  function place() {
    const ps = model.placements(state);
    applying++;
    try {
      for (const p of ps) {
        const ctx = ctxOf(p.doc);
        if (!live(ctx)) continue;
        const w = ctx.win;
        ctx.expected = p.bounds;
        if (p.maximized && !hiddenTest) { if (!w.isMaximized()) w.maximize(); continue; }
        if (w.isMaximized() && !hiddenTest) w.unmaximize();
        if (!same(w.getBounds(), p.bounds)) w.setBounds(p.bounds);
      }
      if (!hiddenTest) {
        for (const p of ps) { const ctx = ctxOf(p.doc); if (live(ctx) && p.visible && !ctx.appwinShown) { if (p.active) { ctx.win.show(); ctx.win.focus(); } else ctx.win.showInactive(); } }
        for (const p of ps) { const ctx = ctxOf(p.doc); if (live(ctx) && !p.visible && ctx.appwinShown !== false) ctx.win.hide(); }
      }
    } finally { applying--; }
    for (const p of ps) { const ctx = ctxOf(p.doc); if (ctx) ctx.appwinShown = p.visible; }
  }

  /** Runs a command on the state; when it changed something the windows are moved and told. */
  function run(command) {
    const r = model.apply(state, command);
    if (!r.ok) return r;
    if (r.changed) { state = r.state; place(); tell(); log(`appwin ${command.cmd} -> ${JSON.stringify(model.placements(state).map((p) => [p.doc, p.group, p.visible]))}`); }
    return { ok: true, changed: r.changed, state: r.state };
  }

  /** The user's own moves and resizes: the frame of the group follows (tabs), or the window leaves the tiling (a tile dragged away). */
  function watch(ctx) {
    const w = ctx.win;
    const doc = w.webContents.id;
    const onGeometry = () => {
      if (applying || !live(ctx) || ctx.appwinShown === false) return;
      const g = model.groupOf(state, doc);
      if (!g || g.arrange) return;
      const maximized = w.isMaximized();
      const bounds = maximized ? w.getNormalBounds() : w.getBounds();
      if (!maximized && same(bounds, ctx.expected)) return;
      const r = model.apply(state, { cmd: 'setBounds', doc, bounds, maximized });
      if (r.ok) state = r.state;
    };
    for (const e of ['move', 'resize', 'maximize', 'unmaximize']) w.on(e, onGeometry);
    // a tile moved or sized by hand floats (it was dragged out of the tiling)
    const onMoved = () => {
      if (applying || !live(ctx)) return;
      const g = model.groupOf(state, doc);
      if (!g || !g.arrange || same(w.getBounds(), ctx.expected)) return;
      run({ cmd: 'floatDoc', doc, bounds: w.getBounds(), area: areaOf(ctx) });
    };
    for (const e of ['moved', 'resized']) w.on(e, onMoved);
  }

  /** A new app window (it floats on its own until the person gathers it). */
  function add(ctx) {
    const doc = ctx.win.webContents.id;
    ctx.appwinShown = true;
    const r = model.apply(state, { cmd: 'addDoc', doc, title: null, bounds: ctx.win.getBounds() });
    if (!r.ok) { log(`appwin add failed ${JSON.stringify(r)}`); return; }
    state = r.state;
    watch(ctx);
    tell();
  }

  /** An app window closed: its tab goes, the next one shows. */
  function remove(doc) {
    if (!Object.prototype.hasOwnProperty.call(state.docs, doc)) return;
    const r = model.apply(state, { cmd: 'removeDoc', doc });
    if (!r.ok) return;
    state = r.state;
    place();
    tell();
  }

  /** What a page says about its own window: the tab label and the unsaved-changes mark. */
  function meta(doc, m) {
    if (!Object.prototype.hasOwnProperty.call(state.docs, doc) || !m || typeof m !== 'object') return { ok: false };
    const r = model.apply(state, { cmd: 'updateDoc', doc, ...(typeof m.title === 'string' || m.title === null ? { title: m.title } : {}), ...(m.dirty !== undefined ? { dirty: !!m.dirty } : {}) });
    if (r.ok && r.changed) { state = r.state; tell(); }
    return { ok: r.ok };
  }

  /** Where the windows are now, as the real windows report it (for tests and for a tool that wants the truth). */
  function layout() {
    const ps = model.placements(state);
    return ps.map((p) => {
      const ctx = ctxOf(p.doc);
      const g = model.groupOf(state, p.doc);
      return { doc: p.doc, group: p.group, mode: g.arrange || 'tabs', title: state.docs[p.doc].title, dirty: state.docs[p.doc].dirty, active: p.active,
        visible: ctx ? ctx.appwinShown !== false : false, bounds: live(ctx) ? ctx.win.getBounds() : null };
    });
  }

  const PERSON = new Set(Object.keys(model.APPWIN_COMMANDS).filter((n) => !model.APPWIN_COMMANDS[n].internal && !model.APPWIN_COMMANDS[n].shell));

  /**
   * The command of a person (or a tool) from the window `caller`. The caller's window is the default for `doc`, the group it gathers
   * into, and the display whose work area floats and cascades use.
   */
  function command(command, caller) {
    if (!command || typeof command !== 'object' || typeof command.cmd !== 'string') return { ok: false, error: 'invalid', message: 'a command is an object with a cmd name' };
    const ctx = ctxOf(caller);
    if (command.cmd === 'layout') return { ok: true, changed: false, layout: layout() };
    if (command.cmd === 'view') return { ok: true, changed: false, view: model.view(state, caller) };
    if (command.cmd === 'closeDoc') {
      const doc = command.doc ?? caller;
      const target = ctxOf(doc);
      if (!live(target)) return { ok: false, error: 'unknownDoc', message: `no app window ${doc}` };
      // the tab shows, then the page is asked to close the way its window's close button does: with unsaved changes it asks first
      // (Save, Don't Save, Cancel) and closes only on a yes; a page that cannot answer (it crashed) is closed at once
      const shown = run({ cmd: 'activate', doc });
      if (!shown.ok) return shown;
      if (target.win.webContents.isCrashed()) target.win.destroy();
      else target.win.webContents.send('folio:event', { event: 'appwin-close' });
      return { ok: true, changed: false, closing: doc, state };
    }
    if (command.cmd === 'saveDoc') {
      // Save All (tools-c2): each window holds its own document and engine session, so the window is asked to save as its File > Save does
      const doc = command.doc ?? caller;
      const target = ctxOf(doc);
      if (!live(target) || target.win.webContents.isCrashed()) return { ok: false, error: 'unknownDoc', message: `no app window ${doc}` };
      target.win.webContents.send('folio:event', { event: 'appwin-save' });
      return { ok: true, changed: false, saving: doc };
    }
    if (!PERSON.has(command.cmd)) return { ok: false, error: 'unknownCommand', message: `unknown window command ${command.cmd}` };
    const withCaller = { ...command };
    if (command.cmd === 'activate' || command.cmd === 'floatDoc' || command.cmd === 'moveTab') withCaller.doc = command.doc ?? caller;
    if (withCaller.focus === undefined && Object.prototype.hasOwnProperty.call(state.docs, caller)) withCaller.focus = caller;
    if (withCaller.area === undefined && live(ctx)) withCaller.area = areaOf(ctx);
    return run(withCaller);
  }

  ipcMain.handle('folio:appwin', (e, cmd) => command(cmd, e.sender.id));
  ipcMain.handle('folio:appwin-meta', (e, m) => meta(e.sender.id, m));
  return { add, remove, command, layout, get state() { return state; } };
}

module.exports = { install };
