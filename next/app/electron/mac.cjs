// macOS startup diagnostics and native app menu. Never writes into a project or app bundle.
const { app, Menu, shell } = require('electron');
const { execFile } = require('node:child_process');
const fs = require('node:fs');
const path = require('node:path');

function setup() {
  const dir = process.env.FOLIO_USER_DATA
    ? path.join(app.getPath('userData'), 'logs')
    : path.join(app.getPath('home'), 'Library', 'Logs', 'Folio Studio');
  fs.mkdirSync(dir, { recursive: true });
  const file = process.env.FOLIO_LOG || path.join(dir, 'app.log');
  // Keep one previous run, so a relaunch doesn't hide the failure.
  if (!process.env.FOLIO_LOG && fs.existsSync(file)) fs.renameSync(file, path.join(dir, 'app.previous.log'));
  process.env.FOLIO_LOG = file;
  return { file, dir };
}

function menu(dir, t) {
  Menu.setApplicationMenu(Menu.buildFromTemplate([
    { label: 'Folio Studio', submenu: [
      { role: 'about', label: t('about') }, { type: 'separator' }, { role: 'services', label: t('services') }, { type: 'separator' },
      // The renderer uses Command+H for frame edges. A native hide role would consume it.
      { label: t('hide'), click: () => app.hide() },
      { role: 'hideOthers', label: t('hideOthers') }, { role: 'unhide', label: t('unhide') }, { type: 'separator' }, { role: 'quit', label: t('quit') },
    ] },
    { label: t('window'), submenu: [
      { role: 'minimize', label: t('minimize') }, { role: 'zoom', label: t('zoom') },
      { type: 'separator' }, { role: 'front', label: t('front') },
    ] },
    { role: 'help', label: t('help'), submenu: [{ label: t('logs'), click: () => shell.openPath(dir) }] },
  ]));
}

async function check(request, enginePath, log) {
  log(`startup app=${app.getVersion()} electron=${process.versions.electron} os=${process.platform} arch=${process.arch} resources=${process.resourcesPath}`);
  let timer;
  try {
    const result = await Promise.race([
      request({ cmd: 'selfCheck' }),
      new Promise((_, reject) => { timer = setTimeout(() => reject(new Error('engine self-check timed out after 30s')), 30000); }),
    ]);
    log(`self-check ${JSON.stringify(result.header)}`);
  } catch (e) {
    log(`self-check failed: ${e.stack || e}`);
  } finally {
    clearTimeout(timer);
  }
  const exe = enginePath();
  if (!exe) return;
  const reader = process.env.FOLIO_INDD_READER || path.join(path.dirname(exe), 'folio-indd-reader');
  // No arguments: reader prints usage and exits with 2, without opening any document.
  execFile(reader, [], { timeout: 10000 }, (error, stdout, stderr) => {
    const healthy = error?.code === 2 && stderr.includes('usage: folio-indd-reader');
    log(`reader launch ok=${healthy} code=${error?.code ?? 0} signal=${error?.signal || ''} ${stderr.trim()} ${healthy ? '' : error || ''}`);
  });
}

module.exports = { setup, menu, check };
