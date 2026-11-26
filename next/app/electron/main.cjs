// Electron main process: starts the Rust engine (folio-engine) and relays requests from the
// window. Engine replies are frames: u32 LE header length, JSON header, `bodyBytes` raw bytes.
const { app, BrowserWindow, dialog, ipcMain, screen, shell } = require('electron');
const { translator } = require('./i18n.cjs');
const sample = require('./sample.cjs');
const { checkExternalUrl } = require('./externalurl.cjs');
const { spawn } = require('node:child_process');
const path = require('node:path');
const fs = require('node:fs');

// FOLIO_USER_DATA=<dir>: keep test runs out of the user's settings (recent documents).
if (process.env.FOLIO_USER_DATA) app.setPath('userData', process.env.FOLIO_USER_DATA);
const mac = process.platform === 'darwin' ? require('./mac.cjs') : null;
const macLogs = mac?.setup();

let win = null;
// FOLIO_LOG=<file>: append diagnostic lines (engine replies, UI console) to a file.
const log = (line) => {
  if (!process.env.FOLIO_LOG) return;
  const text = `${new Date().toISOString()} ${line}\n`;
  if (!mac) { fs.appendFileSync(process.env.FOLIO_LOG, text); return; }
  try { fs.appendFileSync(process.env.FOLIO_LOG, text); } catch (e) { console.error('log:', e.message); }
};
// One window per document, and one engine process per window (a document session lives in its engine): File > Open in New Window makes
// another window with an engine of its own, so documents open at the same time never share a session, an undo history or a lock.
// `mainCtx` is the first window's (the one `win` names); the others are made by createWindow(path).
const contexts = new Map(); // webContents id -> { win, engine, nextId, pending, initialPath }
const created = []; // the windows in the order they were made (test steps address them by number)
let mainCtx = null;
const ctxOf = (sender) => contexts.get(sender.id) || mainCtx;
// Window > Arrange across documents: which windows are tabs of one window, tiles of one frame, or float (appwin.cjs is the model)
const appwin = require('./appwin-shell.cjs').install({ ipcMain, screen, contexts, log });
// Check Spelling > All Documents: the list of windows, a spelling query asked of another window's engine, and the hand-over of the check (electron/spell-docs.cjs)
require('./spell-docs.cjs').install({ ipcMain, contexts, request: (message, ctx) => request(message, ctx), appwin, log });

function enginePath() {
  if (process.env.FOLIO_ENGINE) return process.env.FOLIO_ENGINE;
  const name = process.platform === 'win32' ? 'folio-engine.exe' : 'folio-engine';
  const candidates = [
    path.join(process.resourcesPath || '', name),
    path.join(__dirname, '..', '..', 'engine', 'target', 'release', name),
  ];
  return candidates.find((p) => fs.existsSync(p));
}

let engineStartErrorShown = false;
function showEngineStartError(ctx, message, detail) {
  if (engineStartErrorShown || !ctx.win || ctx.win.isDestroyed()) return;
  engineStartErrorShown = true;
  void dialog.showMessageBox(ctx.win, {
    type: 'error', title: tr('engineStartError'), message: tr(message), detail,
  }).catch(error => log(`engine start dialog failed: ${error.message}`));
}

function startEngine(ctx) {
  const exe = enginePath();
  if (!exe) {
    console.error('folio-engine.exe not found; build it with `npm run build:engine`');
    log('engine executable not found');
    showEngineStartError(ctx, 'engineExecutableMissing');
    return;
  }
  const engine = ctx.engine = spawn(exe, [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true,
    env: { ...process.env, FOLIO_USER_DATA: app.getPath('userData') } });
  log(`engine started ${exe}`);
  let buffer = Buffer.alloc(0);
  let header = null;
  let receivedReply = false;
  engine.stdout.on('data', (chunk) => {
    buffer = Buffer.concat([buffer, chunk]);
    for (;;) {
      if (!header) {
        if (buffer.length < 4) return;
        const len = buffer.readUInt32LE(0);
        if (buffer.length < 4 + len) return;
        header = JSON.parse(buffer.subarray(4, 4 + len).toString('utf8'));
        buffer = buffer.subarray(4 + len);
      }
      const need = header.bodyBytes || 0;
      if (buffer.length < need) return;
      const body = Buffer.from(buffer.subarray(0, need));
      buffer = buffer.subarray(need);
      if (!header.event) receivedReply = true;
      dispatch(ctx, header, body);
      header = null;
    }
  });
  engine.stderr.on('data', (d) => { process.stderr.write(`[engine] ${d}`); if (mac) log(`engine stderr ${d}`); });
  // every platform: an unhandled stream or spawn error would end the main process
  engine.stdin.on('error', (error) => {
    log(`engine input failed: ${error.stack || error}`);
    for (const [, p] of ctx.pending) p.reject(error);
    ctx.pending.clear();
  });
  engine.on('error', (error) => {
    log(`engine spawn failed: ${error.stack || error}`);
    for (const [, p] of ctx.pending) p.reject(error);
    ctx.pending.clear();
    if (ctx.engine === engine) ctx.engine = null;
    if (!receivedReply) {
      showEngineStartError(ctx, error.code === 'ENOENT' ? 'engineExecutableMissing' : 'engineStartError', error.code);
    }
  });
  engine.on('exit', (code, signal) => {
    if (mac) log(`engine exited code=${code} signal=${signal}`);
    for (const [, p] of ctx.pending) p.reject(new Error(`engine exited (${code})`));
    ctx.pending.clear();
    if (ctx.win && !ctx.win.isDestroyed()) ctx.win.webContents.send('folio:event', { event: 'engine-exit', code });
    if (ctx.engine === engine) ctx.engine = null;
  });
  // 'close' comes after stdout has drained, including a final reply sent just before exit.
  engine.on('close', (code, signal) => {
    if (!receivedReply) {
      const windowsCode = process.platform === 'win32' && code !== null ? code >>> 0 : null;
      const systemFileError = windowsCode === 0xC0000135 || windowsCode === 0xC0000139;
      const status = windowsCode !== null && windowsCode > 0xFFFF ? `0x${windowsCode.toString(16).toUpperCase()}` : code ?? signal;
      showEngineStartError(ctx, systemFileError ? 'engineSystemFileError' : 'engineStartError',
        tr(code === null ? 'engineExitSignal' : 'engineExitCode').replace('{status}', String(status)));
    }
  });
}

function dispatch(ctx, header, body) {
  if (header.event) {
    if (ctx.win && !ctx.win.isDestroyed()) ctx.win.webContents.send('folio:event', header);
    return;
  }
  log(`reply id=${header.id} ok=${header.ok} type=${header.type ?? header.event} ${header.error ?? ''} bytes=${body.length}`);
  const p = ctx.pending.get(header.id);
  if (!p) return;
  ctx.pending.delete(header.id);
  p.resolve({ header, body: new Uint8Array(body.buffer, body.byteOffset, body.byteLength) });
}

function request(message, ctx = mainCtx) {
  if (!ctx.engine) startEngine(ctx);
  if (!ctx.engine) return Promise.reject(new Error('engine not available'));
  const id = ctx.nextId++;
  const reply = new Promise((resolve, reject) => {
    ctx.pending.set(id, { resolve, reject });
    ctx.engine.stdin.write(JSON.stringify({ ...message, id }) + '\n');
    log(`request id=${id} ${message.cmd} ${message.page ?? message.path ?? ''}`);
  });
  if (message.cmd === 'open') {
    reply.then(({ header }) => {
      if (!header.ok) return;
      // a .folio belongs to a project: the project goes into the recent projects
      const folio = header.document && header.document.folio;
      if (folio && folio.project) rememberProject({ root: folio.project.root, id: folio.project.id, name: folio.project.name }, ctx).catch(e => log(String(e)));
      else remember(message.path, ctx).catch(e => log(String(e)));
    }, () => {});
  }
  return reply;
}

// Recent paths share the engine's user settings store and migration.
async function recentFiles(ctx = mainCtx) {
  const { header } = await request({ cmd: 'recentFiles' }, ctx);
  if (!header.ok) throw new Error(header.error);
  return header;
}
const existingProjects = list => list.filter(q => fs.existsSync(path.join(q.root, 'folio.project')));
async function rememberProject(p, ctx = mainCtx) {
  const { header } = await request({ cmd: 'rememberRecent', project: p }, ctx);
  if (!header.ok) throw new Error(header.error);
  return existingProjects(header.projects);
}
async function remember(file, ctx = mainCtx) {
  const { header } = await request({ cmd: 'rememberRecent', document: file }, ctx);
  if (!header.ok) throw new Error(header.error);
  app.addRecentDocument(file);
}

// User settings (link search folders, export and package choices), in the app's data folder.
const settingsFile = () => path.join(app.getPath('userData'), 'settings.json');
const DEFAULT_SETTINGS = { language: 'system', linkRoots: [], exportFolder: null, export: null, packageFolder: null, package: null, packageContact: null, documentPresets: [] };
const systemLanguage = () => process.env.FOLIO_TEST_LANGUAGE || app.getLocale();
const tr = key => translator(readSettings().language, systemLanguage())(key);
function readSettings() {
  try {
    const s = JSON.parse(fs.readFileSync(settingsFile(), 'utf8'));
    return { ...DEFAULT_SETTINGS, ...(s && typeof s === 'object' ? s : {}), systemLanguage: systemLanguage() };
  } catch {
    return { ...DEFAULT_SETTINGS, systemLanguage: systemLanguage() };
  }
}
async function saveSettings(patch) {
  let next;
  try {
    const { systemLanguage: _os, ...stored } = patch;
    const { header } = await request({ cmd: 'userSettings', patch: stored });
    if (!header.ok) throw new Error(String(header.error));
    next = { ...DEFAULT_SETTINGS, ...header.settings, systemLanguage: systemLanguage() };
  } catch (e) {
    log(`settings not saved: ${e}`);
    throw e;
  }
  if (mac) mac.menu(macLogs.dir, tr);
  applyInterfaceScale(next);
  return next;
}

function applyInterfaceScale(settings) {
  const value = settings.preferences?.['folio.uiScale'];
  const scale = ['100', '125', '150', '175', '200'].includes(value) ? Number(value) / 100 : 1;
  for (const w of BrowserWindow.getAllWindows()) if (!w.isDestroyed()) w.webContents.setZoomFactor(scale);
}

function createWindow(initialPath = null) {
  // The first window is `win`; File > Open in New Window makes more, each with its own engine (see the contexts above).
  const primary = !win || win.isDestroyed();
  // The menu bar is drawn by the window in the title row (as InDesign does on Windows); the
  // system draws only the window buttons there.
  const chrome = process.platform === 'darwin'
    ? { titleBarStyle: 'hidden', trafficLightPosition: { x: 12, y: 9 } }
    : { titleBarStyle: 'hidden', titleBarOverlay: { color: '#1b1c20', symbolColor: '#dfe1e6', height: 32 } };
  // another window sits a step down and right of the first, so it does not hide it
  const at = !primary && win && !win.isDestroyed() ? win.getBounds() : null;
  // test mode only: FOLIO_TEST_WINDOW=1920x1080 sizes the window (next/oracle/uix-shots.py shoots the screens at two sizes)
  const testSize = process.env.FOLIO_TEST_STEPS && /^(\d{3,4})x(\d{3,4})$/.exec(process.env.FOLIO_TEST_WINDOW || '');
  // test mode only: FOLIO_TEST_HIDDEN=inactive shows the window without making it the active one (it draws frames, so a screenshot works,
  // and it never takes the foreground from another program; next/oracle/pen-c1-shots.py)
  const testInactive = !!process.env.FOLIO_TEST_STEPS && process.env.FOLIO_TEST_HIDDEN === 'inactive';
  const w = new BrowserWindow({
    show: !(process.env.FOLIO_TEST_STEPS && (process.env.FOLIO_TEST_HIDDEN === '1' || testInactive)),
    width: testSize ? Number(testSize[1]) : 1480,
    height: testSize ? Number(testSize[2]) : 940,
    ...(testSize ? { useContentSize: true } : {}),
    ...(at ? { x: at.x + 32 * created.length, y: at.y + 32 * created.length } : {}),
    backgroundColor: '#16171a',
    title: 'Folio Studio',
    autoHideMenuBar: true,
    ...chrome,
    webPreferences: { preload: path.join(__dirname, 'preload.cjs'), contextIsolation: true, sandbox: false, backgroundThrottling: false },
  });
  if (testInactive) w.once('ready-to-show', () => w.showInactive());
  const id = w.webContents.id;
  const ctx = { win: w, engine: null, nextId: 1, pending: new Map(), initialPath };
  contexts.set(id, ctx);
  created.push(w);
  if (primary) { win = w; mainCtx = ctx; }
  appwin.add(ctx);
  w.on('closed', () => appwin.remove(id));
  // FOLIO_DIST=<dir>: a test build of the window (leaves the user's dist alone)
  w.loadFile(path.join(process.env.FOLIO_DIST || path.join(__dirname, '..', 'dist'), 'index.html'));
  w.webContents.on('did-finish-load', () => applyInterfaceScale(readSettings()));
  w.webContents.on('console-message', (e) => log(`ui ${e.message}`));
  // a file dropped outside the drop handler must not replace the app page
  w.webContents.on('will-navigate', (e) => e.preventDefault());
  // A panel group taken out to another monitor (renderer/src/ui/dock.tsx PopoutWindow, uix-d2): an empty page of this window's own
  // renderer that the window draws the group into (the same engine session and document state; nothing of the engine here). A tool
  // window: it stays above its app window and closes with it. Nothing else may open a window.
  w.webContents.setWindowOpenHandler(({ url, frameName }) => (url === 'about:blank' && /^folio-panel-/.test(frameName)
    ? { action: 'allow', overrideBrowserWindowOptions: { parent: w, autoHideMenuBar: true, minimizable: false, fullscreenable: false, backgroundColor: '#16171a', minWidth: 120, minHeight: 80,
        // (its web preferences are its app window's: the window scripts the page it opened, in the same renderer)
        show: !(process.env.FOLIO_TEST_STEPS && process.env.FOLIO_TEST_HIDDEN === '1') } }
    : { action: 'deny' }));
  w.webContents.on('did-create-window', (child, details) => {
    popouts.push(child);
    popoutNames.set(child, details.frameName);
    child.setMenu(null);
    child.webContents.on('will-navigate', (e) => e.preventDefault());
    child.webContents.on('did-finish-load', () => applyInterfaceScale(readSettings()));
    child.on('closed', () => { const i = popouts.indexOf(child); if (i >= 0) popouts.splice(i, 1); });
  });
  if (!primary || process.platform !== 'darwin') {
    // a window's engine ends with its window, the first one's too (its document's lock goes, the other windows go on); the Mac app
    // stays open without windows and keeps the first window's engine (see below)
    w.on('closed', () => {
      contexts.delete(id);
      if (ctx.engine) { const e = ctx.engine; ctx.engine = null; e.stdin.end(JSON.stringify({ cmd: 'quit' }) + '\n'); }
    });
  }
  if (mac && primary) {
    w.webContents.on('render-process-gone', (_e, details) => log(`renderer exited ${JSON.stringify(details)}`));
    w.webContents.on('did-fail-load', (_e, code, description) => log(`UI load failed ${code} ${description}`));
    w.on('closed', () => {
      win = null;
      // A closed window must release its project lock even while the Mac app stays open.
      if (mainCtx.engine) request({ cmd: 'close' }).catch((e) => log(`close: ${e}`));
    });
  }
  return w;
}

// The panel groups taken out to OS windows of their own (newest last; a test step's `popout` names one by its place here).
const popouts = [];
// ... and the monitors they may go to: each monitor's work area in desktop pixels
ipcMain.handle('folio:displays', () => screen.getAllDisplays().map((d) => ({ x: d.workArea.x, y: d.workArea.y, w: d.workArea.width, h: d.workArea.height })));
// each one's window name (`folio-panel-<first panel>`, the name the page opened it with)
const popoutNames = new WeakMap();
// The app window of a call from it or from one of its panel windows, and its panel windows (newest last).
function appWindowOf(sender) {
  const w = BrowserWindow.fromWebContents(sender);
  if (!w) return null;
  return popouts.includes(w) ? w.getParentWindow() : w;
}
const popoutsOf = (app) => popouts.filter((p) => !p.isDestroyed() && p.getParentWindow() === app);
// uix-d3: a panel tab dragged between the app window and its panel windows (renderer/src/ui/dockwindows.ts). The page tells which window
// the pointer is over from where each window's page is on the desktop: the content rectangles (desktop pixels) and the interface size
// (the zoom). Only windows and places: what a drop does stays in the page.
const pagePlace = (w) => { const b = w.getContentBounds(); return { x: b.x, y: b.y, w: b.width, h: b.height, zoom: w.webContents.getZoomFactor() }; };
ipcMain.handle('folio:window-places', (e) => {
  const app = appWindowOf(e.sender);
  if (!app || app.isDestroyed()) return null;
  return { main: pagePlace(app), popouts: popoutsOf(app).map((p) => ({ name: popoutNames.get(p) || '', ...pagePlace(p) })) };
});
// ... and the panel window being dragged from: see-through while the pointer is outside it (it must not hide the place it is dragged
// to), back when it returns or the drag ends; moved or sized to a place (a tab of its own let go on the desktop: the window goes there)
ipcMain.handle('folio:popout-window', (e, name, how) => {
  const app = appWindowOf(e.sender);
  const p = app && popoutsOf(app).find((x) => popoutNames.get(x) === name);
  if (!p || !how || typeof how !== 'object') return false;
  if (typeof how.opacity === 'number' && Number.isFinite(how.opacity)) {
    p.setOpacity(Math.min(1, Math.max(0.2, how.opacity)));
    if (process.env.FOLIO_TEST_STEPS) log(`popout ${name} opacity ${p.getOpacity()}`);
  }
  const b = how.bounds;
  if (b && [b.x, b.y, b.w, b.h].every((v) => typeof v === 'number' && Number.isFinite(v)) && b.w >= 120 && b.h >= 80) p.setBounds({ x: Math.round(b.x), y: Math.round(b.y), width: Math.round(b.w), height: Math.round(b.h) });
  return true;
});

// File > Open in New Window: another window; it opens `path` when it is ready (the window asks for it with folio:initial-path).
ipcMain.handle('folio:new-window', (_e, p) => {
  createWindow(typeof p === 'string' && p ? p : null);
  return true;
});

// the window of the IPC call being handled (dialogs open on it, titles and the minimize act on it); handlers read it before they wait
let senderWin = null;
const handle = ipcMain.handle.bind(ipcMain);
ipcMain.handle = (channel, fn) => handle(channel, (e, ...a) => { senderWin = BrowserWindow.fromWebContents(e.sender); return fn(e, ...a); });
ipcMain.handle('folio:request', (e, message) => request(message, ctxOf(e.sender)));
ipcMain.handle('folio:clipboard-command', (event, how) => {
  if (!['copy', 'cut', 'paste'].includes(how)) throw new Error('invalid clipboard command');
  event.sender[how]();
});
ipcMain.handle('folio:recent', async () => (await recentFiles()).documents.filter(p => fs.existsSync(p)));
// The window asks for a document given on the command line once it is ready to open it.
ipcMain.handle('folio:initial-path', (e) => { const c = ctxOf(e.sender); if (c !== mainCtx) return c.initialPath; return process.argv.slice(1).find((a) => /\.(indd|indt|idml|folio)$/i.test(a) || (fs.existsSync(a) && fs.existsSync(path.join(a, 'folio.project')))) ?? null; });
ipcMain.handle('folio:recent-projects', async () => existingProjects((await recentFiles()).projects));
ipcMain.handle('folio:remember-project', (_e, p) => rememberProject(p));
// Test mode: FOLIO_TEST_OPEN_ANY / FOLIO_TEST_SAVE_IDML answer these dialogs without showing them.
ipcMain.handle('folio:open-any', async () => {
  if (process.env.FOLIO_TEST_OPEN_ANY) return process.env.FOLIO_TEST_OPEN_ANY;
  const r = await dialog.showOpenDialog(senderWin || win, {
    title: tr('open'),
    properties: ['openFile'],
    filters: [{ name: 'Folio · InDesign', extensions: ['folio', 'indd', 'indt', 'idml'] }],
  });
  return r.canceled ? null : r.filePaths[0];
});
ipcMain.handle('folio:save-idml', async (_e, defaultName) => {
  if (process.env.FOLIO_TEST_SAVE_IDML) return process.env.FOLIO_TEST_SAVE_IDML;
  const folder = readSettings().exportFolder || app.getPath('documents');
  const r = await dialog.showSaveDialog(senderWin || win, {
    title: tr('idml'),
    defaultPath: path.join(folder, String(defaultName || 'document.idml')),
    filters: [{ name: 'InDesign Markup (IDML)', extensions: ['idml'] }],
  });
  return r.canceled || !r.filePath ? null : r.filePath;
});
// File ▸ Save As / Save a Copy: where the .folio goes (the engine keeps it in the project)
ipcMain.handle('folio:save-folio', async (_e, defaultPath) => {
  if (process.env.FOLIO_TEST_SAVE_FOLIO) return process.env.FOLIO_TEST_SAVE_FOLIO;
  const r = await dialog.showSaveDialog(senderWin || win, {
    title: tr('saveFolio'),
    ...(typeof defaultPath === 'string' && defaultPath ? { defaultPath } : {}),
    filters: [{ name: tr('folioDocument'), extensions: ['folio'] }],
  });
  return r.canceled || !r.filePath ? null : r.filePath;
});
ipcMain.handle('folio:set-title', (_e, title) => { const w = senderWin || win; if (w && !w.isDestroyed() && typeof title === 'string') w.setTitle(title); });
// the window buttons follow the interface colour theme
ipcMain.handle('folio:set-chrome', (_e, color, symbol) => {
  const w = senderWin || win;
  if (!w || w.isDestroyed() || process.platform === 'darwin') return;
  if (typeof color === 'string' && typeof symbol === 'string' && /^#[0-9a-f]{6}$/i.test(color) && /^#[0-9a-f]{6}$/i.test(symbol)) {
    try { w.setTitleBarOverlay({ color, symbolColor: symbol, height: 32 }); } catch (e) { log(`title bar overlay: ${e}`); }
  }
});
// Window > Arrange > Minimize: the application window goes to the task bar. A test run (FOLIO_TEST_STEPS) records that it was
// asked and the window state, then brings the window back so the next steps can be captured.
ipcMain.handle('folio:minimize', async () => {
  const w = senderWin || win;
  if (!w || w.isDestroyed()) return false;
  w.minimize();
  if (process.env.FOLIO_TEST_STEPS) {
    await new Promise((r) => setTimeout(r, 250));
    log(`window minimize -> minimized=${w.isMinimized()}`);
    w.restore();
  }
  return true;
});
ipcMain.handle('folio:open-dialog', async (_e, kind) => {
  if (kind === 'pdfPreset' && process.env.FOLIO_TEST_PDF_PRESET) return process.env.FOLIO_TEST_PDF_PRESET;
  if (kind === 'printPreset' && process.env.FOLIO_TEST_PRINT_PRESET) return process.env.FOLIO_TEST_PRINT_PRESET;
  const r = await dialog.showOpenDialog(senderWin || win, {
    title: kind === 'pdfPreset' ? 'Folio PDF JSON' : kind === 'printPreset' ? 'Folio print presets JSON' : tr('openInDesign'),
    properties: ['openFile'],
    filters: kind === 'pdfPreset' || kind === 'printPreset' ? [{ name: 'JSON', extensions: ['json'] }] : [{ name: tr('inDesignDocument'), extensions: ['indd', 'indt', 'idml'] }],
  });
  return r.canceled ? null : r.filePaths[0];
});
require('./place.cjs')(ipcMain, dialog, () => senderWin || win, tr);
require('./pdfcomments.cjs')(ipcMain, dialog, () => senderWin || win, tr);
ipcMain.handle('folio:save-dialog', async (_e, defaultName) => {
  if (String(defaultName).endsWith('.json') && process.env.FOLIO_TEST_PDF_PRESET) return process.env.FOLIO_TEST_PDF_PRESET;
  // Test mode: FOLIO_TEST_SAVE_PATH=<file.pdf> answers the save dialog without showing it.
  if (process.env.FOLIO_TEST_SAVE_PATH) return process.env.FOLIO_TEST_SAVE_PATH;
  const folder = readSettings().exportFolder || app.getPath('documents');
  const r = await dialog.showSaveDialog(senderWin || win, {
    title: tr('pdf'),
    defaultPath: path.join(folder, String(defaultName || 'document.pdf')),
    filters: String(defaultName).endsWith('.json') ? [{ name: 'Folio PDF JSON', extensions: ['json'] }] : [{ name: 'PDF', extensions: ['pdf'] }],
  });
  return r.canceled || !r.filePath ? null : r.filePath;
});
// A text file the renderer made (Preflight panel ▸ Save Report): asks where, writes UTF-8.
ipcMain.handle('folio:save-text', async (_e, defaultName, text) => {
  if (typeof text !== 'string') return null;
  // Test mode: FOLIO_TEST_SAVE_TEXT=<file.txt> answers the dialog without showing it.
  let file = process.env.FOLIO_TEST_SAVE_TEXT || null;
  if (!file) {
    const folder = readSettings().exportFolder || app.getPath('documents');
    const r = await dialog.showSaveDialog(senderWin || win, {
      title: tr('saveReport'),
      defaultPath: path.join(folder, String(defaultName || 'report.txt').replace(/[<>:"/\|?*]/g, '_')),
      filters: [{ name: tr('textFile'), extensions: ['txt'] }],
    });
    if (r.canceled || !r.filePath) return null;
    file = r.filePath;
  }
  fs.writeFileSync(file, text, 'utf8');
  return file;
});
// Swatches panel ▸ Load Swatches / Save Swatches: an Adobe Swatch Exchange file (.ase). The renderer
// gets the bytes (no path is kept anywhere) and hands bytes to write.
ipcMain.handle('folio:swatch-open', async () => {
  // Test mode: FOLIO_TEST_SWATCH_OPEN=<file.ase> answers the dialog without showing it.
  let file = process.env.FOLIO_TEST_SWATCH_OPEN || null;
  if (!file) {
    const r = await dialog.showOpenDialog(senderWin || win, { title: tr('loadSwatches'), properties: ['openFile'], filters: [{ name: tr('swatchFile'), extensions: ['ase'] }] });
    if (r.canceled || !r.filePaths[0]) return null;
    file = r.filePaths[0];
  }
  return fs.readFileSync(file);
});
ipcMain.handle('folio:swatch-save', async (_e, defaultName, bytes) => {
  if (!(bytes instanceof Uint8Array)) return null;
  let file = process.env.FOLIO_TEST_SWATCH_SAVE || null;
  if (!file) {
    const folder = readSettings().exportFolder || app.getPath('documents');
    const r = await dialog.showSaveDialog(senderWin || win, {
      title: tr('saveSwatches'),
      defaultPath: path.join(folder, String(defaultName || 'swatches.ase').replace(/[<>:"/\|?*]/g, '_')),
      filters: [{ name: tr('swatchFile'), extensions: ['ase'] }],
    });
    if (r.canceled || !r.filePath) return null;
    file = r.filePath;
  }
  fs.writeFileSync(file, bytes);
  return file;
});
ipcMain.handle('folio:choose-folder', async (_e, title, defaultPath) => {
  if (process.env.FOLIO_TEST_CHOOSE_FOLDER) return process.env.FOLIO_TEST_CHOOSE_FOLDER; // the isolated UI checks answer the dialog
  const r = await dialog.showOpenDialog(senderWin || win, {
    title: typeof title === 'string' && title ? title : tr('linksFolder'),
    ...(typeof defaultPath === 'string' && fs.existsSync(defaultPath) ? { defaultPath } : {}),
    properties: ['openDirectory', 'createDirectory'],
  });
  return r.canceled ? null : r.filePaths[0];
});
// the files under a folder (a few levels deep, at most 20000), for Relink to Folder
ipcMain.handle('folio:list-files', (_e, dir) => {
  const out = [];
  const walk = (d, depth) => {
    let entries = [];
    try { entries = fs.readdirSync(d, { withFileTypes: true }); } catch { return; }
    for (const e of entries) {
      if (out.length >= 20000) return;
      const p = path.join(d, e.name);
      if (e.isDirectory()) { if (depth < 8) walk(p, depth + 1); } else if (e.isFile()) out.push(p);
    }
  };
  if (typeof dir === 'string' && fs.existsSync(dir)) walk(dir, 0);
  return out;
});
// Utilities > Copy Link(s) To…: the files go to the folder picked; a name already there is never overwritten
ipcMain.handle('folio:copy-files', (_e, dir, files) => {
  if (typeof dir !== 'string' || !Array.isArray(files) || !fs.existsSync(dir)) return 0;
  let n = 0;
  for (const f of files) {
    if (typeof f !== 'string' || !fs.existsSync(f)) continue;
    const ext = path.extname(f), stem = path.basename(f, ext);
    let to = path.join(dir, stem + ext);
    for (let i = 2; fs.existsSync(to); i++) to = path.join(dir, `${stem} (${i})${ext}`);
    fs.copyFileSync(f, to);
    n++;
  }
  return n;
});
ipcMain.handle('folio:path-exists', (_e, file) => typeof file === 'string' && fs.existsSync(file));
ipcMain.handle('folio:reveal', (_e, file) => {
  if (typeof file !== 'string' || !fs.existsSync(file)) return;
  if (process.env.FOLIO_TEST_REVEAL_LOG) fs.appendFileSync(process.env.FOLIO_TEST_REVEAL_LOG, file + String.fromCharCode(10)); // the isolated UI checks only record it
  else shell.showItemInFolder(file);
});
// Hyperlinks > Go To Destination on a URL destination: the system browser, for http, https and mailto addresses only (externalurl.cjs).
// An isolated test run (FOLIO_TEST_STEPS, or FOLIO_TEST_OPEN_EXTERNAL_LOG) never starts a browser: it answers as if it had, and may log the address.
ipcMain.handle('folio:open-external', async (_e, url) => {
  const checked = checkExternalUrl(url);
  if (!checked.ok) return checked;
  if (process.env.FOLIO_TEST_STEPS || process.env.FOLIO_TEST_OPEN_EXTERNAL_LOG) {
    if (process.env.FOLIO_TEST_OPEN_EXTERNAL_LOG) fs.appendFileSync(process.env.FOLIO_TEST_OPEN_EXTERNAL_LOG, checked.url + String.fromCharCode(10));
    return { ok: true, url: checked.url, opened: false };
  }
  try { await shell.openExternal(checked.url); return { ok: true, url: checked.url, opened: true }; }
  catch (e) { log(`open external: ${e}`); return { ok: false, reason: 'failed' }; }
});
// opens a folder or file with its default program (the package folder, its report)
ipcMain.handle('folio:open-path', async (_e, target) => (typeof target === 'string' && fs.existsSync(target) ? shell.openPath(target) : 'not found'));
// Edit ▸ Edit With ▸ Other…: a program picked by the person (FOLIO_TEST_PROGRAM answers the dialog), and a file opened with it. The
// The program path is stored in the local user profile.
// FOLIO_TEST_OPEN_WITH_LOG=<file>: the isolated UI checks only record the program and the file.
ipcMain.handle('folio:choose-program', async (e) => {
  if (process.env.FOLIO_TEST_PROGRAM) return process.env.FOLIO_TEST_PROGRAM;
  const options = { properties: ['openFile'], filters: process.platform === 'win32' ? [{ name: 'Programs', extensions: ['exe', 'bat', 'cmd', 'com'] }, { name: 'All Files', extensions: ['*'] }] : [] };
  const r = await dialog.showOpenDialog(BrowserWindow.fromWebContents(e.sender) || undefined, options);
  return r.canceled || !r.filePaths[0] ? null : r.filePaths[0];
});
ipcMain.handle('folio:open-with', async (_e, program, file) => {
  if (typeof program !== 'string' || typeof file !== 'string' || !fs.existsSync(program) || !fs.existsSync(file)) return 'not found';
  if (process.env.FOLIO_TEST_OPEN_WITH_LOG) { fs.appendFileSync(process.env.FOLIO_TEST_OPEN_WITH_LOG, `${program}\t${file}\n`); return ''; }
  try {
    const app_ = process.platform === 'darwin' && program.endsWith('.app');
    const child = app_ ? spawn('open', ['-a', program, file], { detached: true, stdio: 'ignore' }) : spawn(program, [file], { detached: true, stdio: 'ignore' });
    await new Promise((resolve, reject) => { child.once('error', reject); child.once('spawn', resolve); });
    child.unref();
    return '';
  } catch (err) { return String(err.message || err); }
});
// Links panel ▸ Utilities ▸ XMP File Info… and Versions…: what the link's file says about itself, read only. The XMP packet is the text
// from <x:xmpmeta to </x:xmpmeta> (at most 4 MB of it), found in chunks so a big picture is never read whole.
ipcMain.handle('folio:file-xmp', (_e, file) => {
  if (typeof file !== 'string' || !fs.existsSync(file)) return null;
  const stat = fs.statSync(file);
  const open = Buffer.from('<x:xmpmeta'), close = Buffer.from('</x:xmpmeta>');
  const chunk = 4 * 1024 * 1024, limit = 4 * 1024 * 1024, buf = Buffer.alloc(chunk);
  let packet = null, found = null, tail = Buffer.alloc(0), at = 0;
  const fd = fs.openSync(file, 'r');
  try {
    for (;;) {
      const n = fs.readSync(fd, buf, 0, chunk, at);
      if (n <= 0) break; // no end of a packet: no XMP
      at += n;
      if (packet === null) {
        const view = Buffer.concat([tail, buf.subarray(0, n)]);
        const i = view.indexOf(open);
        if (i < 0) { tail = Buffer.from(view.subarray(Math.max(0, view.length - open.length + 1))); continue; }
        packet = Buffer.from(view.subarray(i));
      } else packet = Buffer.concat([packet, buf.subarray(0, n)]);
      const j = packet.indexOf(close);
      if (j >= 0) { found = packet.subarray(0, j + close.length).toString('utf8'); break; }
      if (packet.length > limit) break;
    }
  } finally { fs.closeSync(fd); }
  return { size: stat.size, modified: stat.mtimeMs, packet: found };
});
// FOLIO_SAMPLE_HOME=<dir>: tests keep the sample project out of the person's Documents.
ipcMain.handle('folio:sample', () => {
  try { return sample.prepare({ resources: process.resourcesPath, documents: process.env.FOLIO_SAMPLE_HOME || app.getPath('documents'), engine: enginePath() }); }
  catch (e) { log(`sample: ${e.stack || e}`); return { error: String(e.message || e) }; }
});
ipcMain.handle('folio:sample-imported', (_e, document) => { try { return sample.imported(document); } catch (e) { log(`sample: ${e.stack || e}`); return false; } });
ipcMain.handle('folio:app-info', () => ({ version: app.getVersion(), documents: app.getPath('documents') }));
ipcMain.handle('folio:settings', () => readSettings());
require('./settings-transfer.cjs').register(ipcMain, dialog, () => senderWin || win, readSettings, saveSettings, tr);
ipcMain.handle('folio:save-settings', (_e, patch) => saveSettings(patch && typeof patch === 'object' ? patch : {}));

// A test run opens many windows at once on one screen: Chromium would treat the covered ones as hidden and slow them down, which
// makes timing-sensitive checks fail only when they run side by side. Test mode only; the app itself keeps Chromium's defaults.
if (process.env.FOLIO_TEST_STEPS) {
  app.commandLine.appendSwitch('disable-features', 'CalculateNativeWinOcclusion');
  app.commandLine.appendSwitch('disable-renderer-backgrounding');
  app.commandLine.appendSwitch('disable-background-timer-throttling');
  app.commandLine.appendSwitch('disable-backgrounding-occluded-windows');
}
const focused = new WeakSet();
function emulateFocus(w) {
  if (focused.has(w)) return;
  focused.add(w);
  try {
    w.webContents.debugger.attach('1.3');
    w.webContents.debugger.sendCommand('Emulation.setFocusEmulationEnabled', { enabled: true })
      .catch((e) => log(`focus emulation failed: ${e}`));
  } catch (e) { log(`focus emulation failed: ${e}`); }
}
app.whenReady().then(() => {
  if (mac) mac.menu(macLogs.dir, tr);
  createWindow();
  startEngine(mainCtx);
  if (mac) void mac.check(request, enginePath, log);

  // Test mode: FOLIO_TEST_STEPS=<steps.json> runs [{ js?, wait?, shot? }] in order after
  // FOLIO_SCREENSHOT_DELAY ms: JavaScript in the window, a pause, a capture to a PNG file.
  if (process.env.FOLIO_TEST_STEPS) {
    const steps = JSON.parse(fs.readFileSync(process.env.FOLIO_TEST_STEPS, 'utf8'));
    win.webContents.once('did-finish-load', () => {
      // A test window the OS does not activate (the user works in another window, or eight test windows run side by side) gets
      // no focus or blur events, so a field that commits on blur never commits and the check times out (color-ui, 2026-10-28).
      // Make the page believe it has focus, whatever the OS does with the window.
      emulateFocus(win);
      setTimeout(async () => {
        for (const step of steps) {
          try {
            // a step names the window it runs in (`window`: 0 is the first, 1 the next one made...); it waits until that window is loaded
            const n = step.window || 0;
            for (let t = 0; t < 200 && !(created[n] && !created[n].isDestroyed() && !created[n].webContents.isLoading()); t++) await new Promise((r) => setTimeout(r, 100));
            const target = created[n];
            if (!target) throw new Error(`no window ${n}`);
            if (n > 0) emulateFocus(target);
            if (step.js) log(`step js${n ? ` (window ${n})` : ''} -> ${JSON.stringify(await target.webContents.executeJavaScript(step.js))}`);
            // `inputJs` (uix-d3): the input events are what that JavaScript gives (places measured in the run itself, as where the
            // windows are); `popoutInput`: the input goes to a panel group's own OS window (`popout`: its place among them) instead
            const input = step.inputJs ? await target.webContents.executeJavaScript(step.inputJs) : step.input;
            if (step.inputJs) log(`step inputJs -> ${JSON.stringify(input).slice(0, 400)}`);
            if (Array.isArray(input) && step.popoutInput) {
              const p = popouts[step.popout || 0];
              if (!p || p.isDestroyed()) log(`step: no popout ${step.popout || 0}`);
              else { emulateFocus(p); for (const event of input) p.webContents.sendInputEvent(event); }
            } else if (Array.isArray(input)) for (const event of input) target.webContents.sendInputEvent(event);
            if (step.wait) await new Promise((r) => setTimeout(r, step.wait));
            if (step.shot) fs.writeFileSync(step.shot, (await target.webContents.capturePage()).toPNG());
            // a panel group's own OS window (uix-d2): `popout` is its place among them, `popoutShot` the file
            if (step.popoutShot) {
              // (`popoutShotOf`: another one than the input went to, uix-d3)
              const k = step.popoutShotOf ?? step.popout ?? 0;
              const p = popouts[k];
              if (!p || p.isDestroyed()) log(`step: no popout ${k}`);
              else { log(`step popout bounds ${JSON.stringify(p.getBounds())}`); fs.writeFileSync(step.popoutShot, (await p.webContents.capturePage()).toPNG()); }
            }
          } catch (e) {
            log(`step failed: ${e}`);
          }
        }
        // app.exit: a test run ends here even with unsaved edits (no "save changes?" question)
        if (process.env.FOLIO_SCREENSHOT_QUIT) app.exit(0);
      }, Number(process.env.FOLIO_SCREENSHOT_DELAY || 8000));
    });
  }

  // Test mode: FOLIO_SCREENSHOT=<file.png> captures the window after FOLIO_SCREENSHOT_DELAY ms.
  if (process.env.FOLIO_SCREENSHOT) {
    win.webContents.once('did-finish-load', () => {
      setTimeout(async () => {
        for (let attempt = 0; attempt < 5; attempt++) {
          try {
            const image = await win.webContents.capturePage();
            fs.writeFileSync(process.env.FOLIO_SCREENSHOT, image.toPNG());
            break;
          } catch (e) {
            log(`screenshot attempt ${attempt} failed: ${e}`);
            await new Promise((r) => setTimeout(r, 1000));
          }
        }
        // app.exit: a test run ends here even with unsaved edits (no "save changes?" question)
        if (process.env.FOLIO_SCREENSHOT_QUIT) app.exit(0);
      }, Number(process.env.FOLIO_SCREENSHOT_DELAY || 8000));
    });
  }
});

app.on('window-all-closed', () => {
  if (mac) return;
  for (const c of contexts.values()) if (c.engine) c.engine.stdin.end(JSON.stringify({ cmd: 'quit' }) + '\n');
  app.quit();
});
if (mac) {
  app.on('activate', () => { if (!win || win.isDestroyed()) createWindow(); });
  app.on('will-quit', () => {
    log('app quitting');
    for (const c of contexts.values()) if (c.engine) c.engine.stdin.end(JSON.stringify({ cmd: 'quit' }) + '\n');
  });
}
