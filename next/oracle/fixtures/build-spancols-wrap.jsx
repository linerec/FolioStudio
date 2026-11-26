(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, old = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw }, doc = null;
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    doc.paragraphStyles[1].properties = { appliedFont: app.fonts.itemByName('Arial\tRegular'), fontStyle: 'Regular', pointSize: 12, leading: 14, hyphenation: false };
    function lines(prefix, n) { var a = []; for (var i = 1; i <= n; i++) a.push(prefix + i); return a.join('\n'); }
    function frame(id, bounds, columns) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id);
      f.textFramePreferences.properties = { textColumnCount: columns, textColumnGutter: 12 };
      f.contents = lines('lead', 7) + '\r' + lines('body', 7) + '\rafter1\nafter2';
      return f;
    }
    frame('span', [36, 36, 300, 540], 4);
    frame('split', [350, 36, 750, 540], 1);

    function obstacle(id, bounds) {
      var r = doc.pages[0].rectangles.add({geometricBounds: bounds, fillColor: doc.swatches.itemByName('Black'), strokeWeight: 0});
      r.insertLabel('oracle.id', id);
    }
    obstacle('wrapspan', [36, 165, 65, 282]);
    obstacle('wrapsplit', [456, 206, 485, 370]);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":["span","split"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/"/g, "'") + '"}'; }
  finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = old.ui; sp.measurementUnit = old.unit; sp.enableRedraw = old.redraw;
  }
})();
