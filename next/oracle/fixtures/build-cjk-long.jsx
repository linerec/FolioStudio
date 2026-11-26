// Long Korean word boundary specimens. Run: capture.py build cjk-long.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, old = {ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw};
  var doc = null, errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    if (!font.isValid) throw Error('Malgun Gothic Regular is not installed');
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {facingPages: false, pageWidth: 400, pageHeight: 600, pagesPerDocument: 2};
    doc.viewPreferences.properties = {horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN};
    function repeat(n) { return new Array(n + 1).join('가'); }
    var cases = [
      ['word-31', repeat(31), 2, 2], ['word-32', repeat(32), 2, 2],
      ['word-33', repeat(33), 2, 2], ['suffix-1', repeat(33), 2, 1],
      ['word-41', repeat(41), 2, 2], ['trailing-space', repeat(31) + ' ', 2, 2],
      ['next-word', repeat(32) + ' 나', 2, 2], ['suffix-5', repeat(33), 3, 5]
    ];
    for (var i = 0; i < cases.length; i++) {
      var c = cases[i], x = 30 + (i % 2) * 190, y = 30 + Math.floor((i % 4) / 2) * 280;
      var f = doc.pages[Math.floor(i / 4)].textFrames.add({geometricBounds: [y, x, y + 250, x + 96]});
      f.insertLabel('oracle.id', c[0]);
      f.textFramePreferences.properties = {insetSpacing: 0, firstBaselineOffset: FirstBaseline.ASCENT_OFFSET};
      f.contents = c[1];
      f.parentStory.texts[0].properties = {appliedFont: font, fontStyle: 'Regular', appliedLanguage: 'English: USA',
        pointSize: 11, leading: 15, composer: '$ID/HL Composer', hyphenation: false,
        hyphenateWordsLongerThan: 5, hyphenateAfterFirst: c[2], hyphenateBeforeLast: c[3],
        justification: Justification.LEFT_ALIGN, kerningMethod: 'Metrics', ligatures: false,
        spaceBefore: 0, spaceAfter: 0, leftIndent: 0, rightIndent: 0, firstLineIndent: 0, tracking: 0,
        minimumWordSpacing: 80, desiredWordSpacing: 100, maximumWordSpacing: 133,
        minimumLetterSpacing: 0, desiredLetterSpacing: 0, maximumLetterSpacing: 0,
        minimumGlyphScaling: 100, desiredGlyphScaling: 100, maximumGlyphScaling: 100};
    }
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (closeError) { errors.push(closeError.message); }
    sp.userInteractionLevel = old.ui; sp.measurementUnit = old.unit; sp.enableRedraw = old.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["word-31","word-32","word-33","suffix-1","word-41","trailing-space","next-word","suffix-5"],"errors":["' + errors.join('; ').split('"').join("'") + '"]}';
})();
