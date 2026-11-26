// Create Outlines on a text range: Arial 36 pt frames without insets (one line, tracking, colours, mixed sizes, baseline shift,
// a small leading, manual kerning, skew and scale, Hangul, mixed fonts; a wrapped frame, two paragraphs, an inline object).
// Hidden document, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function frame(name, b, text) {
    var f = doc.pages[0].textFrames.add({ geometricBounds: b });
    f.textFramePreferences.insetSpacing = [0, 0, 0, 0];
    f.contents = text;
    f.insertLabel('oracle.id', name); made.push(name);
    var t = f.texts[0];
    t.appliedFont = app.fonts.itemByName('Arial\tRegular'); t.pointSize = 36; t.kerningMethod = 'Metrics';
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
    var paper = doc.swatches.itemByName('Paper');
    frame('one', [20, 20, 70, 300], 'AV Oag');
    var track = frame('track', [80, 20, 130, 300], 'AV Oag'); track.texts[0].tracking = 100;
    var colors = frame('colors', [140, 20, 190, 300], 'AV Oag'); colors.characters.itemByRange(3, 5).fillColor = paper;
    var sizes = frame('sizes', [200, 20, 270, 300], 'AV Oag'); sizes.characters[1].pointSize = 72;
    var shift = frame('shift', [280, 20, 340, 300], 'AV Oag'); shift.characters[1].baselineShift = 9;
    var lead = frame('lead', [350, 20, 400, 300], 'Oag Bog'); lead.texts[0].leading = 20;
    var kern = frame('kern', [410, 20, 460, 300], 'AV Oag'); kern.insertionPoints[1].kerningValue = -150;
    var skew = frame('skew', [470, 20, 520, 300], 'AV Oag'); skew.texts[0].skew = 15; skew.texts[0].horizontalScale = 150; skew.texts[0].verticalScale = 80;
    var han = frame('hangul', [530, 20, 590, 300], '가나 다'); han.texts[0].appliedFont = app.fonts.itemByName('Malgun Gothic\tRegular');
    var mix = frame('fontmix', [600, 20, 650, 300], 'AV Oag'); mix.characters.itemByRange(3, 5).appliedFont = app.fonts.itemByName('Times New Roman\tRegular');
    frame('wrap', [20, 320, 130, 470], 'AV Oag Bog Tea');
    frame('para', [140, 320, 240, 590], 'AV\rOag');
    var inl = frame('inline', [250, 320, 310, 590], 'AV Oag');
    var r = inl.insertionPoints[3].rectangles.add({ geometricBounds: [0, 0, 20, 30] });
    r.anchoredObjectSettings.anchoredPosition = AnchorPosition.INLINE_POSITION;
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
