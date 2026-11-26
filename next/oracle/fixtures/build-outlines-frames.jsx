// Create Outlines on whole frames, the remaining cases (outlines-c2): underline, strike-through, a paragraph rule, different strokes, a threaded pair. Arial 48 pt, no insets.
// Hidden document, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function frame(name, b, text) {
    var f = doc.pages[0].textFrames.add({ geometricBounds: b }); f.textFramePreferences.insetSpacing = [0, 0, 0, 0];
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
    frame('under', [40, 80, 100, 420], 'AV Oag').texts[0].underline = true;
    frame('strike', [120, 80, 180, 420], 'AV Oag').texts[0].strikeThru = true;
    var rule = frame('rule', [200, 80, 280, 420], 'AV Oag'); rule.paragraphs[0].ruleAbove = true; rule.paragraphs[0].ruleAboveLineWeight = 3; rule.paragraphs[0].ruleAboveOffset = 0;
    var strokes = frame('strokes', [300, 80, 360, 420], 'AV Oag'); strokes.characters.itemByRange(0, 2).strokeColor = doc.swatches.itemByName('Black'); strokes.characters.itemByRange(0, 2).strokeWeight = 2; strokes.characters.itemByRange(0, 2).fillColor = doc.swatches.itemByName('Paper');
    var s2 = frame('strokes2', [540, 80, 660, 420], 'AV\rOag'); s2.texts[0].leading = 60; s2.texts[0].strokeColor = doc.swatches.itemByName('Black'); s2.texts[0].strokeWeight = 2; s2.texts[0].fillColor = doc.swatches.itemByName('Paper');
    var ta = frame('threadA', [380, 80, 440, 260], 'AV Oag Bog Tea Mix Zig'); var tb = doc.pages[0].textFrames.add({ geometricBounds: [460, 80, 520, 260] }); tb.textFramePreferences.insetSpacing = [0, 0, 0, 0]; ta.nextTextFrame = tb; tb.insertLabel('oracle.id', 'threadB'); made.push('threadB');
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
