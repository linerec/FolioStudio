(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, old = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw }, doc = null;
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 8 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    doc.paragraphStyles[1].properties = { appliedFont: app.fonts.itemByName('Arial\tRegular'), fontStyle: 'Regular', pointSize: 12, leading: 14, hyphenation: false };
    function lines(prefix, n) { var a = []; for (var i = 1; i <= n; i++) a.push(prefix + i); return a.join('\n'); }
    var tops = [100, 100, 100, 100, 100, 100.3, 100.9, 101.5];
    var heights = [10, 20, 29, 38, 50, 20, 20, 20];
    for (var i = 0; i < heights.length; i++) {
      var pg = doc.pages[i];
      var f = pg.textFrames.add({ geometricBounds: [36, 36, 400, 540] });
      f.insertLabel('oracle.id', 'f' + i);
      f.textFramePreferences.properties = { textColumnCount: 1 };
      f.contents = lines('t', 24);
      var r = pg.rectangles.add({geometricBounds: [tops[i], 30, tops[i] + heights[i], 546], fillColor: doc.swatches.itemByName('Black'), strokeWeight: 0});
      r.insertLabel('oracle.id', 'w' + i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":["wrapmid"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/"/g, "'") + '"}'; }
  finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = old.ui; sp.measurementUnit = old.unit; sp.enableRedraw = old.redraw;
  }
})();
