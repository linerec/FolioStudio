// Primary Text Frame fixture: parents with empty text frames (alone, and as a thread of two), no primary text frame.
// Hidden document, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function label(it, name) { it.insertLabel('oracle.id', name); made.push(name); return it; }
  function on(page, b) { var p = page.bounds; return [p[0]+b[0],p[1]+b[1],p[0]+b[2],p[1]+b[3]]; }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    doc.documentPreferences.properties = { facingPages:true, pageWidth:300, pageHeight:400, pagesPerDocument:3 };
    doc.marginPreferences.properties = { top:24, bottom:24, left:24, right:24 };
    var a = doc.masterSpreads[0], b = doc.masterSpreads.add({namePrefix:'B',baseName:'Alternate'});
    // parent A: one frame on each page, and a thread of two (left page to right page)
    label(a.pages[0].textFrames.add({geometricBounds:on(a.pages[0],[24,24,120,140])}), 'master-0-0');
    label(a.pages[1].textFrames.add({geometricBounds:on(a.pages[1],[24,24,120,140])}), 'master-0-1');
    var ta = label(a.pages[0].textFrames.add({geometricBounds:on(a.pages[0],[160,24,360,140])}), 'thread-a');
    var tb = label(a.pages[1].textFrames.add({geometricBounds:on(a.pages[1],[160,24,360,140])}), 'thread-b');
    ta.nextTextFrame = tb;
    // parent B: one frame
    label(b.pages[0].textFrames.add({geometricBounds:on(b.pages[0],[24,24,360,140])}), 'master-1-0');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  } catch(e) {
    return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';
  } finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
})();
