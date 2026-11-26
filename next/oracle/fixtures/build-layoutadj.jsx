// Oracle fixture "layoutadj": File > Adjust Layout on a single-page document (300 x 400 pt, margins 30). Two pages, a
// parent with a rectangle, and plain rectangles and text frames on the pages that sit on the margin lines, on the page edges
// and between (hairline, thick and no stroke), guides, and a two-column second page. Every item is labelled `oracle.id`.
// Run with capture.py build layoutadj.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length != 0) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function label(it, id) { it.insertLabel('oracle.id', id); made.push(id); return it; }
  function rect(page, id, b, weight) {
    var r = page.rectangles.add({ geometricBounds: b });
    if (weight === null) { r.strokeColor = doc.swatches.itemByName('None'); } else { r.strokeWeight = weight; }
    return label(r, id);
  }
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
    dp.facingPages = false;
    dp.pageWidth = 300;
    dp.pageHeight = 400;
    dp.pagesPerDocument = 2;
    doc.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    var p1 = doc.pages[0], p2 = doc.pages[1];
    p1.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    p2.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30, columnCount: 2, columnGutter: 20 };
    rect(doc.masterSpreads[0].pages[0], 'o-parent', [40, 50, 100, 130], 1);
    rect(p1, 'o-hair', [40, 50, 100, 130], 0);
    rect(p1, 'o-thick', [120, 50, 160, 130], 4);
    rect(p1, 'o-none', [170, 50, 210, 130], null);
    rect(p1, 'o-margins', [30, 30, 370, 270], null);
    rect(p1, 'o-page', [0, 0, 400, 300], null);
    rect(p1, 'o-corner', [0, 0, 30, 30], 0);
    rect(p1, 'o-thin', [220, 31, 221, 31.6], null);
    var tf = p1.textFrames.add({ geometricBounds: [230, 50, 300, 130] });
    tf.contents = 'Adjust layout moves the edges of a frame, and the lines of this text wrap anew.';
    label(tf, 'o-text');
    var g1 = p1.guides.add({ orientation: HorizontalOrVertical.VERTICAL, location: 50 });
    var g2 = p2.guides.add({ orientation: HorizontalOrVertical.HORIZONTAL, location: 200 });
    label(g1, 'o-guide-v'); label(g2, 'o-guide-h');
    rect(p2, 'o-col1', [60, 30, 120, 140], null);
    rect(p2, 'o-col-gutter', [130, 100, 190, 200], 1);
    rect(p2, 'o-col2', [200, 160, 260, 270], null);
    rect(p2, 'o-cols', [270, 30, 330, 270], 0.5);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/["\\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
