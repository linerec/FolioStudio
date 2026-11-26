// Keep options meeting other conditions: threaded frames of different widths and column counts, and a text wrap beside the lines.
// Arial only (a system font). Body 12/14 pt, no hyphenation, left aligned.
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
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 2 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular' };
    var body = doc.paragraphStyles.add({ name: 'Compound Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 14, hyphenation: false, justification: Justification.LEFT_ALIGN });
    function frame(id, bounds, text, columns) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id);
      if (text !== null) { f.contents = text; f.parentStory.paragraphs.everyItem().applyParagraphStyle(body, true); }
      if (columns) f.textFramePreferences.properties = { textColumnCount: columns, textColumnGutter: 12 };
      return f;
    }
    var words = 'alpha bravo charlie delta echo foxtrot golf hotel india juliet kilo lima mike november oscar papa quebec romeo sierra tango';
    // A: a 240 pt frame then a 120 pt frame (the same paragraph sets in half as many words per line)
    var a1 = frame('wide', [36, 36, 120, 276], 'Heading\r' + words + '\r' + words + '\rtail ' + words, 1);
    var a2 = frame('narrow', [132, 36, 244, 156], '', 1);
    a1.nextTextFrame = a2;
    // B: two columns, then one wide column
    var b1 = frame('twocol', [260, 36, 344, 276], 'Heading\r' + words + '\r' + words + '\rtail ' + words, 2);
    var b2 = frame('onecol', [356, 36, 456, 276], '', 1);
    b1.nextTextFrame = b2;
    // C: a wrap beside the lines (the text keeps the room right of the box)
    var c1 = frame('wrapped', [470, 36, 554, 276], 'Heading\r' + words + '\r' + words + '\rtail ' + words, 1);
    var c2 = frame('wrapped-next', [566, 36, 666, 276], '', 1);
    c1.nextTextFrame = c2;
    var box = doc.pages[0].rectangles.add({ geometricBounds: [470, 36, 520, 96], fillColor: doc.swatches.itemByName('Black'), strokeWeight: 0 });
    box.insertLabel('oracle.id', 'box');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["wide","narrow","twocol","onecol","wrapped","wrapped-next","box"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
