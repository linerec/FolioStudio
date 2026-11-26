// Oracle fixture "loadstyles": the document that Load Paragraph / Character / Object Styles loads into
// (scenarios/loadstyles.json; the document it loads from is "loadstyles-other"). Its names meet the
// other's on purpose: paragraph styles Body, Head (based on Body) and the group Heads with Sub, character style Em,
// object style Box, the swatch Oracle Blue (other values there); and some only here (Target Only, Target Em,
// Target Box). Text frame t1 (Head, Body, Em) and rectangle r1 (Box). the text frame is as wide as [Normal Text Frame] says (144), so it has no override of its own; A4-ish page, no window, exported to CONFIG.out,
// closed without saving; preferences restored. Run with capture.py build loadstyles.
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
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular' };
    var black = doc.swatches.itemByName('Black');
    var blue = doc.colors.add({ name: 'Oracle Blue', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [90, 50, 0, 0] });
    var body = doc.paragraphStyles.add({ name: 'Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 16, fillColor: black });
    var head = doc.paragraphStyles.add({ name: 'Head', basedOn: body, pointSize: 20, leading: 24 });
    doc.paragraphStyles.add({ name: 'Same', basedOn: body, pointSize: 13 });
    doc.paragraphStyles.add({ name: 'Target Only', basedOn: body, pointSize: 11 });
    var heads = doc.paragraphStyleGroups.add({ name: 'Heads' });
    heads.paragraphStyles.add({ name: 'Sub', basedOn: body, pointSize: 15 });
    var em = doc.characterStyles.add({ name: 'Em', fontStyle: 'Italic' });
    doc.characterStyles.add({ name: 'Target Em', fontStyle: 'Bold' });
    var box = doc.objectStyles.add({ name: 'Box', enableFill: true, fillColor: blue });
    doc.objectStyles.add({ name: 'Target Box', enableStroke: true, strokeWeight: 3, strokeColor: black });
    var tf = doc.pages[0].textFrames.add({ geometricBounds: [36, 36, 200, 180] });
    tf.insertLabel('oracle.id', 't1'); made.push('t1');
    tf.contents = 'Heading text\rBody text with emphasis.';
    tf.parentStory.paragraphs[0].appliedParagraphStyle = head;
    tf.parentStory.paragraphs[1].appliedParagraphStyle = body;
    tf.parentStory.paragraphs[1].characters.itemByRange(15, 23).appliedCharacterStyle = em;
    var r1 = doc.pages[0].rectangles.add({ geometricBounds: [240, 36, 300, 200] });
    r1.insertLabel('oracle.id', 'r1'); made.push('r1');
    r1.appliedObjectStyle = box;
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
