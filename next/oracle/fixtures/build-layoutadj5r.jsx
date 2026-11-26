// Oracle fixture "layoutadj5r": File > Adjust Layout on facing pages with right-to-left binding (300 x 400 pt, inside margin 20, outside 40): objects over the
// pages, across the spine and off the pages of the second spread (two pages) and of the first one (one page). Every item is labelled
// `oracle.id`. Run with capture.py build layoutadj5r.
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
    var xs = [[100, 620, 160, 680], [-60, 500, -20, 700], [420, 100, 460, 500], [-60, 200, -20, 400], [100, 250, 160, 350], [100, 280, 160, 320], [100, 200, 300, 400],
      [360, 250, 420, 350], [-30, 250, 30, 350], [100, 20, 160, 580], [100, 550, 160, 650], [100, 150, 160, 450], [100, 590, 160, 700], [-60, 650, -20, 700],
      [420, 650, 460, 700], [120, 100, 180, 560], [100, -250, 160, -190], [100, 190, 160, 250], [100, -80, 160, 80], [-60, -50, 60, 50], [-60, -100, -20, 100], [100, -50, 160, 650], [200, -1, 260, 601]];
    for (var i = 0; i < xs.length; i++) { var r = s.rectangles.add({ geometricBounds: xs[i] }); r.strokeWeight = 1; label(r, 'o-r' + i); }
    var o1 = s.ovals.add({ geometricBounds: [100, 250, 160, 350] }); o1.strokeWeight = 1; label(o1, 'o-oval0');
    var o2 = s.ovals.add({ geometricBounds: [100, 150, 200, 450] }); o2.strokeWeight = 1; label(o2, 'o-oval1');
    var g1 = s.rectangles.add({ geometricBounds: [100, 250, 160, 290] }), g2 = s.rectangles.add({ geometricBounds: [200, 310, 260, 350] });
    g1.strokeWeight = 1; g2.strokeWeight = 1;
    label(s.groups.add([g1, g2]), 'o-group0');
    var l1 = s.graphicLines.add({ strokeWeight: 2 }); l1.paths[0].entirePath = [[250, 100], [350, 200]]; label(l1, 'o-line0');
    var f = doc.spreads[0];
    var ys = [[100, 40, 160, 100], [100, -100, 160, -40], [100, 340, 160, 400], [-60, 40, -20, 100], [420, 40, 460, 100], [100, -20, 160, 60]];
    for (var j = 0; j < ys.length; j++) { var q = f.rectangles.add({ geometricBounds: ys[j] }); q.strokeWeight = 1; label(q, 'o-f' + j); }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message + ' @' + e.line).replace(/["\\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
