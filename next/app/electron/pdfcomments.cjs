// File ▸ Import PDF Comments…: InDesign shows a file open window ("Import PDF comments", *.pdf, button "Import comments"). The PDF the
// person picks is read by the engine; a PDF inside the project is used where it is. One from outside is copied into the project first
// (principle 1: the project holds what it refers to), after the person agreed: `copy` puts it in `reviews/` under a name every OS accepts.
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');

const sameContent = (a, b) => {
  try { return fs.statSync(a).size === fs.statSync(b).size && crypto.createHash('sha256').update(fs.readFileSync(a)).digest('hex') === crypto.createHash('sha256').update(fs.readFileSync(b)).digest('hex'); }
  catch { return false; }
};
// Windows' forbidden characters and trailing dots or spaces; reserved device names get an underscore
const portable = (name) => {
  let n = name.normalize('NFC').replace(/[<>:"\\|?*\u0000-\u001f/]/g, '_').replace(/[. ]+$/, '');
  if (!n || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\..*)?$/i.test(n)) n = '_' + n;
  return n;
};

module.exports = (ipcMain, dialog, getWindow, tr) => {
  // FOLIO_TEST_PDF_COMMENTS answers the window in the isolated UI checks (only with FOLIO_TEST_STEPS): a path, or a JSON list of
  // paths, one for each time the window opens (null once the list is used up)
  let answers = null;
  ipcMain.handle('folio:pdf-comments-dialog', async (_e, root) => {
    if (process.env.FOLIO_TEST_STEPS && process.env.FOLIO_TEST_PDF_COMMENTS) {
      const given = process.env.FOLIO_TEST_PDF_COMMENTS;
      if (!given.startsWith('[')) return given;
      answers = answers || JSON.parse(given);
      return answers.shift() ?? null;
    }
    const result = await dialog.showOpenDialog(getWindow(), {
      title: tr('importPdfComments'),
      buttonLabel: tr('importComments'),
      defaultPath: typeof root === 'string' && fs.existsSync(root) ? root : undefined,
      properties: ['openFile'],
      filters: [{ name: tr('pdfFiles'), extensions: ['pdf'] }],
    });
    return result.canceled || !result.filePaths[0] ? null : result.filePaths[0];
  });
  ipcMain.handle('folio:pdf-comments-copy', (_e, source, root) => {
    if (typeof source !== 'string' || typeof root !== 'string' || !/\.pdf$/i.test(source) || !fs.existsSync(source) || !fs.existsSync(path.join(root, 'folio.project'))) return { ok: false, error: 'invalid' };
    const dir = path.join(root, 'reviews');
    fs.mkdirSync(dir, { recursive: true });
    const base = portable(path.basename(source));
    const stem = base.replace(/\.pdf$/i, '');
    for (let n = 1; n < 1000; n++) {
      const name = n === 1 ? base : `${stem} ${n}.pdf`;
      const target = path.join(dir, name);
      const lower = fs.readdirSync(dir).find((f) => f.toLowerCase() === name.toLowerCase());
      if (lower) { if (sameContent(source, path.join(dir, lower))) return { ok: true, path: `reviews/${lower}` }; continue; }
      fs.copyFileSync(source, target, fs.constants.COPYFILE_EXCL);
      return { ok: true, path: `reviews/${name}` };
    }
    return { ok: false, error: 'names' };
  });
};
