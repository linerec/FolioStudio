// Keep options meeting a paragraph that spans the columns: the span heading ends up at the bottom of a short frame.
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
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 460, pageHeight: 300, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 14, hyphenation: false };
    var f = doc.pages[0].textFrames.add({ geometricBounds: [36, 36, 66, 426] });
    f.insertLabel('oracle.id', 'span');
    f.textFramePreferences.properties = { textColumnCount: 3, textColumnGutter: 12 };
    f.contents = 'a1\na2\rHeading\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8\nb9\nb10\nb11\nb12';
    var f2 = doc.pages[0].textFrames.add({ geometricBounds: [130, 36, 250, 426] });
    f2.insertLabel('oracle.id', 'span-next');
    f2.textFramePreferences.properties = { textColumnCount: 3, textColumnGutter: 12 };
    f.nextTextFrame = f2;
    var head = f.parentStory.paragraphs[1];
    head.spanColumnType = SpanColumnTypeOptions.SPAN_COLUMNS;
    head.spanSplitColumnCount = SpanColumnCountOptions.ALL;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["span","span-next"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
