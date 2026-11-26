// Oracle fixture "layoutadj4m": File > Adjust Layout with margins on a document of three single pages (300 x 400 pt): the first
// page never touched, the second with margins set to the parent's own value, the third with its own margins. Rectangles on each page.
// Every item is labelled `oracle.id`. Run with capture.py build layoutadj4m.
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
    dp.facingPages = false; dp.pageWidth = 300; dp.pageHeight = 400; dp.pagesPerDocument = 3;
    doc.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    doc.masterSpreads[0].pages[0].marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    doc.pages[1].marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    doc.pages[2].marginPreferences.properties = { top: 30, bottom: 30, left: 50, right: 30 };
    // the parent's primary text frame has a copy on every page; on a page with its own margins InDesign rescales it by the parent's
    // change (measured once, not built), so the fixture has none
    for (var q = 0; q < doc.pages.length; q++) while (doc.pages[q].textFrames.length > 0) doc.pages[q].textFrames[0].remove();
    var xs = [[100, 0, 200, 300], [100, 36, 200, 264], [100, 50, 200, 100], [60, 60, 120, 200]];
    for (var pi = 0; pi < 3; pi++) for (var i = 0; i < xs.length; i++) { var r = doc.pages[pi].rectangles.add({ geometricBounds: xs[i] }); r.strokeWeight = 1; label(r, 'o-p' + pi + '-' + i); }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message + ' @' + e.line).replace(/["\\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
