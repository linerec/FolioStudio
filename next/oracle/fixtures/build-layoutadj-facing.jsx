// Oracle fixture "layoutadj-facing": File > Adjust Layout on facing pages (300 x 400 pt, inside margin 20, outside 40).
// Three pages (the first alone on the right, then a left and a right page), a parent rectangle, and rectangles and text frames
// near the margins of the left and the right page. Every item is labelled `oracle.id`. Run with capture.py build layoutadj-facing.
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
    dp.facingPages = true;
    dp.pageWidth = 300;
    dp.pageHeight = 400;
    dp.pagesPerDocument = 3;
    for (var k = 0; k < doc.pages.length; k++) doc.pages[k].marginPreferences.properties = { top: 30, bottom: 30, left: 20, right: 40 };
    doc.masterSpreads[0].pages[0].marginPreferences.properties = { top: 30, bottom: 30, left: 20, right: 40 };
    if (doc.masterSpreads[0].pages.length > 1) doc.masterSpreads[0].pages[1].marginPreferences.properties = { top: 30, bottom: 30, left: 20, right: 40 };
    rect(doc.masterSpreads[0].pages[0], 'o-parent', [100, 60, 160, 150], 1);
    rect(doc.pages[0], 'o-right-first', [100, 20, 200, 260], 0);
    rect(doc.pages[1], 'o-left', [100, 40, 200, 150], 0);
    rect(doc.pages[1], 'o-left-inner', [60, 150, 120, 280], null);
    rect(doc.pages[2], 'o-right', [100, 20, 200, 150], 1);
    rect(doc.pages[2], 'o-right-outer', [220, 60, 280, 260], null);
    var tf = doc.pages[2].textFrames.add({ geometricBounds: [300, 20, 360, 260] });
    tf.contents = 'Facing pages: the inside margin of a left page is on its right.';
    label(tf, 'o-text');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/["\\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
