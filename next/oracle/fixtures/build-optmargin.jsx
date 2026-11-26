// Optical margin fixture: system font and three independent paragraphs.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    doc.textPreferences.typographersQuotes = false;
    var body = doc.paragraphStyles.add({ name: 'Optical Body', appliedFont: app.fonts.itemByName('Arial\tRegular'), fontStyle: 'Regular', pointSize: 12, leading: 18, hyphenation: false });
    var f = doc.pages[0].textFrames.add({ geometricBounds: [36,36,300,286] });
    f.insertLabel('oracle.id','optmargin');
    f.contents = '“HAMBURG.”\r-HAMBURG-\rAHAMBURGA';
    f.parentStory.texts[0].applyParagraphStyle(body, true);
    f.parentStory.storyPreferences.properties = { opticalMarginAlignment: false, opticalMarginSize: 12 };
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["optmargin"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
