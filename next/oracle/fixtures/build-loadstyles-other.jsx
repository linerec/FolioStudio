// Oracle fixture "loadstyles-other": the document that scenarios/loadstyles.json loads styles FROM into "loadstyles".
// Same names with other definitions (paragraph Body, Head, Heads:Sub, character Em, object Box, swatch Oracle Blue,
// [Basic Paragraph]); new ones (paragraph New Para based on Head with next style Body, Drop with a drop cap
// character style Cap, Heads:Extra, Other:In Other; character New Char based on Em, Cap; object New Box based on
// Box, Frames:In Frames; swatch Oracle Green). Run with capture.py build loadstyles-other.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, errors = [], made = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular', pointSize: 8 };
    var black = doc.swatches.itemByName('Black');
    var blue = doc.colors.add({ name: 'Oracle Blue', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [0, 50, 0, 0] });
    var green = doc.colors.add({ name: 'Oracle Green', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [70, 0, 90, 0] });
    var em = doc.characterStyles.add({ name: 'Em', underline: true, fillColor: green });
    var cap = doc.characterStyles.add({ name: 'Cap', fontStyle: 'Bold', fillColor: green });
    doc.characterStyles.add({ name: 'New Char', basedOn: em, pointSize: 14, strokeColor: blue });
    var body = doc.paragraphStyles.add({ name: 'Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 9, leading: 12, fillColor: green });
    doc.paragraphStyles.add({ name: 'Same', basedOn: body, pointSize: 13 });
    var head = doc.paragraphStyles.add({ name: 'Head', basedOn: body, pointSize: 30, leading: 34 });
    var np = doc.paragraphStyles.add({ name: 'New Para', basedOn: head, pointSize: 24, leading: 28 });
    np.nextStyle = body;
    doc.paragraphStyles.add({ name: 'Drop', basedOn: body, dropCapLines: 3, dropCapCharacters: 1, dropCapStyle: cap });
    var heads = doc.paragraphStyleGroups.add({ name: 'Heads' });
    heads.paragraphStyles.add({ name: 'Sub', basedOn: body, pointSize: 14 });
    heads.paragraphStyles.add({ name: 'Extra', basedOn: body, pointSize: 13 });
    var other = doc.paragraphStyleGroups.add({ name: 'Other' });
    other.paragraphStyles.add({ name: 'In Other', basedOn: body, pointSize: 10 });
    var box = doc.objectStyles.add({ name: 'Box', enableFill: true, fillColor: green });
    doc.objectStyles.add({ name: 'New Box', basedOn: box, enableStroke: true, strokeWeight: 2, strokeColor: black });
    var frames = doc.objectStyleGroups.add({ name: 'Frames' });
    frames.objectStyles.add({ name: 'In Frames', enableStroke: true, strokeWeight: 4, strokeColor: black });
    // no text frame: a frame made in a document records its width in [Normal Text Frame], which Load Object Styles would carry over
    var r0 = doc.pages[0].rectangles.add({ geometricBounds: [36, 36, 96, 200] });
    r0.insertLabel('oracle.id', 'o1'); made.push('o1');
    r0.appliedObjectStyle = box;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) {
    errors.push(String(e.message) + ' @' + e.line);
  } finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  function q(s) { return '"' + String(s).split('\\').join('/').split('"').join("'") + '"'; }
  var a = [], b = [];
  for (var i = 0; i < made.length; i++) a.push(q(made[i]));
  for (var j = 0; j < errors.length; j++) b.push(q(errors[j]));
  return '{"ok":' + (errors.length == 0) + ',"made":[' + a.join(',') + '],"errors":[' + b.join(',') + ']}';
})();
