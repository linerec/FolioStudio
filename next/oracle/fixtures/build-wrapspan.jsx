(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, old = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw }, doc = null;
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 10 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    doc.paragraphStyles[1].properties = { appliedFont: app.fonts.itemByName('Arial\tRegular'), fontStyle: 'Regular', pointSize: 12, leading: 14, hyphenation: false };
    function lines(prefix, n) { var a = []; for (var i = 1; i <= n; i++) a.push(prefix + i); return a.join('\n'); }
    // columns, lead lines, box column (1-based), box top below the frame top, box height
    var v = [[3, 7, 2, 0, 29], [3, 7, 2, 20, 29], [3, 7, 3, 0, 29], [3, 7, 1, 0, 50], [3, 10, 2, 0, 29],
             [2, 7, 2, 0, 29], [4, 7, 3, 0, 38], [4, 9, 2, 10, 20], [5, 7, 4, 0, 29], [3, 12, 2, 0, 38]];
    for (var i = 0; i < v.length; i++) {
      var pg = doc.pages[i], cols = v[i][0];
      var f = pg.textFrames.add({ geometricBounds: [36, 36, 300, 540] });
      f.insertLabel('oracle.id', 'f' + i);
      f.textFramePreferences.properties = { textColumnCount: cols, textColumnGutter: 12 };
      f.contents = lines('lead', v[i][1]) + '\r' + lines('body', 7) + '\rafter1\nafter2';
      var w = (504 - 12 * (cols - 1)) / cols, left = 36 + (v[i][2] - 1) * (w + 12), top = 36 + v[i][3];
      var r = pg.rectangles.add({geometricBounds: [top, left, top + v[i][4], left + w], fillColor: doc.swatches.itemByName('Black'), strokeWeight: 0});
      r.insertLabel('oracle.id', 'w' + i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":["wrapspan"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/"/g, "'") + '"}'; }
  finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = old.ui; sp.measurementUnit = old.unit; sp.enableRedraw = old.redraw;
  }
})();
