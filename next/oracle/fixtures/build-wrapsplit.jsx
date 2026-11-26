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
    var heights = [10, 20, 29, 38, 50];
    for (var g = 0; g < 2; g++) {
      for (var i = 0; i < heights.length; i++) {
        var k = g * 5 + i, pg = doc.pages[k];
        var f = pg.textFrames.add({ geometricBounds: [36, 36, 400, 540] });
        f.insertLabel('oracle.id', 'f' + k);
        f.textFramePreferences.properties = { textColumnCount: 1 };
        f.contents = (g == 0 ? lines('lead', 7) + '\r' : '') + lines('body', 7) + '\rafter1\nafter2';
        var top = g == 0 ? 142 : 36;
        var r = pg.rectangles.add({geometricBounds: [top, 206, top + heights[i], 370], fillColor: doc.swatches.itemByName('Black'), strokeWeight: 0});
        r.insertLabel('oracle.id', 'w' + k);
      }
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":["wrapsplit"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/"/g, "'") + '"}'; }
  finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = old.ui; sp.measurementUnit = old.unit; sp.enableRedraw = old.redraw;
  }
})();
