// Oracle fixture "layoutadj4f": File > Adjust Layout on facing pages with right-to-left binding (300 x 400 pt, inside margin 20,
// outside 40): rectangles on the first spread with two pages, guides of the spread and of its pages. Every item is labelled
// `oracle.id`. Run with capture.py build layoutadj4f.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length != 0) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function label(it, id) { it.insertLabel('oracle.id', id); made.push(id); return it; }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    doc = app.documents.add(false);
    var vp = doc.viewPreferences;
    vp.horizontalMeasurementUnits = MeasurementUnits.POINTS;
    vp.verticalMeasurementUnits = MeasurementUnits.POINTS;
    vp.rulerOrigin = RulerOrigin.SPREAD_ORIGIN;
    var dp = doc.documentPreferences;
    dp.facingPages = true; dp.pageWidth = 300; dp.pageHeight = 400; dp.pagesPerDocument = 4;
    dp.pageBinding = PageBindingOptions.RIGHT_TO_LEFT;
    doc.marginPreferences.properties = { top: 30, bottom: 30, left: 20, right: 40 };
    for (var k = 0; k < doc.pages.length; k++) doc.pages[k].marginPreferences.properties = { top: 30, bottom: 30, left: 20, right: 40 };
    for (var m = 0; m < doc.masterSpreads[0].pages.length; m++) doc.masterSpreads[0].pages[m].marginPreferences.properties = { top: 30, bottom: 30, left: 20, right: 40 };
    var s = doc.spreads[1];
    var xs = [[100, 50, 200, 250], [100, 350, 200, 550], [36, 36, 364, 264], [100, 40, 160, 100]];
    for (var i = 0; i < xs.length; i++) { var r = s.rectangles.add({ geometricBounds: xs[i] }); r.strokeWeight = 1; label(r, 'o-r' + i); }
    var gs = [[-250, 'V'], [-100, 'V'], [0, 'V'], [100, 'V'], [250, 'V'], [400, 'V'], [200, 'H'], [-20, 'H'], [450, 'H']];
    for (var g = 0; g < gs.length; g++) s.guides.add({ orientation: gs[g][1] == 'V' ? HorizontalOrVertical.VERTICAL : HorizontalOrVertical.HORIZONTAL, location: gs[g][0], fitToPage: false });
    doc.pages[1].guides.add({ orientation: HorizontalOrVertical.VERTICAL, location: 100 });
    doc.pages[2].guides.add({ orientation: HorizontalOrVertical.HORIZONTAL, location: 150 });
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message + ' @' + e.line).replace(/["\\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
