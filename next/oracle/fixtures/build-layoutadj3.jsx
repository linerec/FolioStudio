// Oracle fixture "layoutadj3": File > Adjust Layout on a single page document (300 x 400 pt, margins 30) with ovals, locked
// objects, and objects on the pasteboard and across the page edge. Every item is labelled `oracle.id`.
// Run with capture.py build layoutadj3.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length != 0) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function label(it, id) { it.insertLabel('oracle.id', id); made.push(id); return it; }
  function rect(page, id, b, weight, locked) {
    var r = page.rectangles.add({ geometricBounds: b });
    if (weight === null) { r.strokeColor = doc.swatches.itemByName('None'); } else { r.strokeWeight = weight; }
    if (locked) r.locked = true;
    return label(r, id);
  }
  function oval(page, id, b, weight, locked) {
    var r = page.ovals.add({ geometricBounds: b });
    if (weight === null) { r.strokeColor = doc.swatches.itemByName('None'); } else { r.strokeWeight = weight; }
    if (locked) r.locked = true;
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
    dp.pagesPerDocument = 1;
    doc.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    var p1 = doc.pages[0];
    p1.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    rect(p1, 'o-rect', [40, 50, 100, 130], 1);
    oval(p1, 'o-oval', [40, 150, 100, 230], 1);
    oval(p1, 'o-oval-thick', [120, 50, 160, 200], 4);
    oval(p1, 'o-oval-none', [200, 60, 240, 100], null);
    rect(p1, 'o-lock-rect', [200, 130, 260, 200], 1, true);
    oval(p1, 'o-lock-oval', [270, 60, 330, 140], 1, true);
    var tf = p1.textFrames.add({ geometricBounds: [270, 160, 330, 260] });
    tf.contents = 'locked frame';
    label(tf, 'o-lock-text');
    tf.locked = true;
    rect(p1, 'o-pb-left', [100, -90, 160, -40], 1);
    rect(p1, 'o-pb-right', [100, 330, 160, 380], 1);
    rect(p1, 'o-pb-above', [-60, 100, -10, 160], 1);
    rect(p1, 'o-pb-below', [420, 100, 470, 160], 1);
    rect(p1, 'o-pb-corner', [-60, -60, -10, -10], 1);
    rect(p1, 'o-pb-corner-br', [420, 330, 470, 380], 0);
    rect(p1, 'o-straddle-left', [100, -30, 160, 40], 1);
    rect(p1, 'o-straddle-bottom', [370, 100, 430, 160], 1);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message).replace(/["\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
