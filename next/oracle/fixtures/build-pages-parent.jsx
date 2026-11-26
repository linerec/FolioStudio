// Parent-item fixture: five facing pages (US Letter) on parent A, whose two pages each hold rectangles, an oval, a text frame, a
// line and a group, one rectangle with Allow Overrides off; the pages hold a box of their own. The DOM overrides only the first
// rectangle of a parent page, so that one (`parent-rect-L/R`) is the one the scenarios override; the others are for the
// engine's own checks (kinds, text copies, Override All, Allow Overrides).
// Hidden document, system font only, no persistent application preference changes.
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
    doc.marginPreferences.properties = { top:24, bottom:24, left:24, right:24 };
    // a new document starts with a primary text frame on the parent and the first page: this fixture has none
    doc.masterSpreads[0].pageItems.everyItem().remove();
    doc.pages[0].pageItems.everyItem().remove();
    // US Letter as a new document has it: resizing the pages would leave the first page and the parent with bounds offset from the others
    doc.documentPreferences.properties = { facingPages:true, pagesPerDocument:5, allowPageShuffle:true, preserveLayoutWhenShuffling:true };
    var black = doc.swatches.itemByName('Black'), none = doc.swatches.itemByName('None');
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    var master = doc.masterSpreads[0], tag = ['L', 'R'];
    for (var q = 0; q < master.pages.length; q++) {
      var mp = master.pages[q], t = tag[q];
      // the first rectangle of each parent page: the one InDesign's scripting can override
      label(mp.rectangles.add({geometricBounds:on(mp,[40,24,70,100]),fillColor:black,strokeColor:none}), 'parent-rect-' + t);
      label(mp.rectangles.add({geometricBounds:on(mp,[90,24,120,100]),fillColor:black,fillTint:40,strokeColor:none}), 'parent-rect2-' + t);
      label(mp.ovals.add({geometricBounds:on(mp,[140,24,180,100]),fillColor:black,fillTint:20,strokeColor:none}), 'parent-oval-' + t);
      var tf = label(mp.textFrames.add({geometricBounds:on(mp,[200,24,230,160])}), 'parent-text-' + t);
      tf.contents = 'Parent ' + t;
      tf.parentStory.texts[0].properties = {appliedFont:font, pointSize:12, leading:16};
      label(mp.graphicLines.add({geometricBounds:on(mp,[250,24,250,160])}), 'parent-line-' + t);
      var g1 = mp.rectangles.add({geometricBounds:on(mp,[270,24,290,60]),fillColor:black,strokeColor:none});
      var g2 = mp.ovals.add({geometricBounds:on(mp,[270,70,290,106]),fillColor:black,strokeColor:none});
      label(mp.groups.add([g1, g2]), 'parent-group-' + t);
      var locked = label(mp.rectangles.add({geometricBounds:on(mp,[310,24,340,100]),fillColor:black,fillTint:60,strokeColor:none}), 'parent-fixed-' + t);
      locked.allowOverrides = false;
    }
    for (var i = 0; i < doc.pages.length; i++) {
      var page = doc.pages[i];
      label(page.rectangles.add({geometricBounds:on(page,[350,150,380,200]),fillColor:black,fillTint:30,strokeColor:none}), 'box-' + i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  } catch(e) {
    return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';
  } finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
})();
