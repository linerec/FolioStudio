// Create Outlines fixture: text frames of Arial 48 pt without insets (one line; two lines; paint; kerning; baseline shift; mixed fill; a lone rectangular glyph).
// Hidden document, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function frame(name, b, text) {
    var f = doc.pages[0].textFrames.add({ geometricBounds: b });
    f.contents = text;
    f.insertLabel('oracle.id', name); made.push(name);
    var t = f.texts[0];
    t.appliedFont = app.fonts.itemByName('Arial\tRegular'); t.pointSize = 48; t.kerningMethod = 'Metrics';
    return f;
  }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.PAGE_ORIGIN };
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 612, pageHeight: 792, pagesPerDocument: 1 };
    doc.marginPreferences.properties = { top: 24, bottom: 24, left: 24, right: 24 };
    doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle').appliedFont = app.fonts.itemByName('Arial\tRegular');
    doc.textDefaults.appliedFont = app.fonts.itemByName('Arial\tRegular');
    frame('line', [40, 80, 100, 420], 'AV Oag');
    var two = frame('two', [120, 80, 260, 420], 'AV\rOag'); two.texts[0].leading = 60;
    var paint = frame('paint', [280, 80, 340, 420], 'AV Oag');
    paint.texts[0].strokeColor = doc.swatches.itemByName('Black'); paint.texts[0].strokeWeight = 2; paint.texts[0].fillColor = doc.swatches.itemByName('Paper');
    var kern = frame('kern', [360, 80, 420, 420], 'AV Oag'); kern.texts[0].tracking = 80; kern.insertionPoints[1].kerningValue = -150;
    var shift = frame('shift', [440, 80, 520, 420], 'AV Oag'); shift.characters[1].baselineShift = 9;
    var mixed = frame('mixed', [540, 80, 600, 420], 'AV Oag'); mixed.characters[4].fillColor = doc.swatches.itemByName('Paper');
    frame('rect', [620, 80, 680, 420], 'I');
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + made.toSource().replace(/'/g, '"') + '}';
  } catch (e) {
    return '{"ok":false,"error":"' + String(e.message).replace(/["\\\r\n]/g, ' ') + ' @' + e.line + '"}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
})();
