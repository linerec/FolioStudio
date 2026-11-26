// Keep options with paragraph spacing at the column break (space before is dropped at the top of a column).
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
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 400, pageHeight: 400, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular' };
    var body = doc.paragraphStyles.add({ name: 'Space Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 14, hyphenation: false, spaceAfter: 6 });
    var head = doc.paragraphStyles.add({ name: 'Space Head', basedOn: body, spaceBefore: 10, spaceAfter: 4 });
    var f = doc.pages[0].textFrames.add({ geometricBounds: [36, 36, 120, 276] });
    f.insertLabel('oracle.id', 'space');
    f.textFramePreferences.properties = { textColumnCount: 2, textColumnGutter: 12 };
    f.contents = 'b1a\nb1b\nb1c\rHead\rb2a\nb2b\nb2c\nb2d\nb2e\rb3a\nb3b';
    var ps = f.parentStory.paragraphs;
    ps.everyItem().applyParagraphStyle(body, true);
    ps[1].applyParagraphStyle(head, true);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["space"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
