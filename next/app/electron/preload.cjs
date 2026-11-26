const { contextBridge, ipcRenderer, webUtils } = require('electron');
const { localize } = require('./engine-message.cjs');

// The screen's language and the English engine sentences (the screen sends them: setEngineLocale); a failure answer comes back in that language.
let engineLocale = { language: 'ko', english: null };

contextBridge.exposeInMainWorld('folio', {
  request: async (message) => localize(await ipcRenderer.invoke('folio:request', message), engineLocale),
  setEngineLocale: (language, english) => { engineLocale = { language, english }; },
  clipboardCommand: (how) => ipcRenderer.invoke('folio:clipboard-command', how),
  openDialog: (kind) => ipcRenderer.invoke('folio:open-dialog', kind),
  placeDialog: () => ipcRenderer.invoke('folio:place-dialog'),
  // File ▸ Import PDF Comments…: the file open window (the project folder first), and a PDF from outside the project copied into it once the person agreed
  pdfCommentsDialog: (root) => ipcRenderer.invoke('folio:pdf-comments-dialog', root),
  copyPdfIntoProject: (source, root) => ipcRenderer.invoke('folio:pdf-comments-copy', source, root),
  initialPath: () => ipcRenderer.invoke('folio:initial-path'),
  // File > Open in New Window: another window with an engine of its own (opens `path` when given)
  newWindow: (path) => ipcRenderer.invoke('folio:new-window', path),
  // Window > Arrange across documents: a command of electron/appwin.cjs (activate, consolidate, arrange, floatDoc, cascade ...) or a query
  // (state, layout, view), and this window's own tab label and unsaved mark; the answers are structured values
  appWindows: (cmd) => ipcRenderer.invoke('folio:appwin', cmd),
  appWindowMeta: (meta) => ipcRenderer.invoke('folio:appwin-meta', meta),
  // a panel group taken out to an OS window of its own (renderer/src/ui/dock.tsx PopoutWindow): the monitors' work areas, to put it
  // back on the monitor it was on, or inside the main window when that monitor is gone
  displays: () => ipcRenderer.invoke('folio:displays'),
  // ... dragged between windows (uix-d3): where this app window's page and its panel windows' pages are on the desktop, and the panel
  // window a drag comes from made see-through or moved (renderer/src/ui/dockwindows.ts)
  windowPlaces: () => ipcRenderer.invoke('folio:window-places'),
  popoutWindow: (name, how) => ipcRenderer.invoke('folio:popout-window', name, how),
  // Check Spelling > All Documents (electron/spell-docs.cjs): the windows in the order their documents were opened, one read-only spelling query
  // asked of another window's engine, and the hand-over of the check to another window (it comes to the front and gets a `spell-handoff` event)
  spellWindows: () => ipcRenderer.invoke('folio:spell-windows'),
  windowQuery: (window, message) => ipcRenderer.invoke('folio:window-query', window, message),
  spellHandoff: (window, payload) => ipcRenderer.invoke('folio:spell-handoff', window, payload),
  recent: () => ipcRenderer.invoke('folio:recent'),
  // path of a file dropped on the window ('' when it has none, e.g. dragged from a browser)
  pathForFile: (file) => webUtils.getPathForFile(file),
  saveDialog: (defaultName) => ipcRenderer.invoke('folio:save-dialog', defaultName),
  saveText: (defaultName, text) => ipcRenderer.invoke('folio:save-text', defaultName, text),
  swatchFileOpen: () => ipcRenderer.invoke('folio:swatch-open'),
  swatchFileSave: (defaultName, bytes) => ipcRenderer.invoke('folio:swatch-save', defaultName, bytes),
  chooseFolder: (title, defaultPath) => ipcRenderer.invoke('folio:choose-folder', title, defaultPath),
  reveal: (file) => ipcRenderer.invoke('folio:reveal', file),
  openExternal: (url) => ipcRenderer.invoke('folio:open-external', url),
  listFiles: (dir) => ipcRenderer.invoke('folio:list-files', dir),
  copyFiles: (dir, files) => ipcRenderer.invoke('folio:copy-files', dir, files),
  pathExists: (file) => ipcRenderer.invoke('folio:path-exists', file),
  openPath: (target) => ipcRenderer.invoke('folio:open-path', target),
  chooseProgram: () => ipcRenderer.invoke('folio:choose-program'),
  openWith: (program, file) => ipcRenderer.invoke('folio:open-with', program, file),
  fileXmp: (file) => ipcRenderer.invoke('folio:file-xmp', file),
  appInfo: () => ipcRenderer.invoke('folio:app-info'),
  sample: (kind) => ipcRenderer.invoke('folio:sample', kind),
  sampleImported: (document) => ipcRenderer.invoke('folio:sample-imported', document),
  settings: () => ipcRenderer.invoke('folio:settings'),
  saveSettings: (patch) => ipcRenderer.invoke('folio:save-settings', patch),
  transferSettings: (mode) => ipcRenderer.invoke('folio:transfer-settings', mode),
  onEvent: (listener) => {
    const wrapped = (_e, header) => listener(header);
    ipcRenderer.on('folio:event', wrapped);
    return () => ipcRenderer.removeListener('folio:event', wrapped);
  },
});

// The window frame: colours of the system window buttons in the title row.
contextBridge.exposeInMainWorld('folioWindow', {
  setChrome: (color, symbol) => ipcRenderer.invoke('folio:set-chrome', color, symbol),
  minimize: () => ipcRenderer.invoke('folio:minimize'),
});

// Folio projects: recent projects (machine settings), dialogs, the window title.
contextBridge.exposeInMainWorld('folioProject', {
  recent: () => ipcRenderer.invoke('folio:recent-projects'),
  remember: (p) => ipcRenderer.invoke('folio:remember-project', p),
  openAny: () => ipcRenderer.invoke('folio:open-any'),
  saveIdml: (defaultName) => ipcRenderer.invoke('folio:save-idml', defaultName),
  saveFolio: (defaultPath) => ipcRenderer.invoke('folio:save-folio', defaultPath),
  setTitle: (title) => ipcRenderer.invoke('folio:set-title', title),
});
