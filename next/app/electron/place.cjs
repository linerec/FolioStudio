// Kept separate from platform menu / path work.
const PICTURES = ['jpg', 'jpeg', 'png', 'tif', 'tiff', 'psd', 'gif', 'bmp', 'pdf', 'ai', 'svg'];
// text files: plain text, Word and RTF (oracle placetext-c1, placetext-c2)
const TEXT = ['txt', 'docx', 'rtf'];
module.exports = (ipcMain, dialog, getWindow, tr) => {
  ipcMain.handle('folio:place-dialog', async () => {
    if (process.env.FOLIO_TEST_STEPS && process.env.FOLIO_TEST_PLACE_FILES) {
      return JSON.parse(process.env.FOLIO_TEST_PLACE_FILES);
    }
    const result = await dialog.showOpenDialog(getWindow(), {
      title: tr('place'),
      properties: ['openFile', 'multiSelections'],
      filters: [
        { name: tr('placeable'), extensions: [...PICTURES, ...TEXT] },
        { name: tr('pictures'), extensions: PICTURES },
        { name: tr('placeText'), extensions: TEXT },
        // EPS, SVG, EMF, WMF and the like can be picked too: the screen then says by name that Folio does not place them
        { name: tr('allFiles'), extensions: ['*'] },
      ],
    });
    return result.canceled ? [] : result.filePaths;
  });
};
