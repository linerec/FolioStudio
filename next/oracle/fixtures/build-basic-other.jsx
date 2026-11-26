// Oracle fixture "basic-other": a second document to copy from into "basic" (Edit > Paste across
// documents). Its names meet basic's on purpose: a swatch "Oracle Blue" with other values, a
// layer "Base" like basic's, [Basic Paragraph] with another size; and some are new: swatch
// "Oracle Green", spot "Oracle Spot 2", paragraph style "Oracle Body", character style
// "Oracle Em", object style "Oracle Card", layer "Other", a placed image. Every item is labelled
// `oracle.id` (o-…). A4 facing pages like basic, no window, exported to CONFIG.out, closed
// without saving; preferences restored. Run with capture.py build basic-other.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length != 0) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var errs = [], doc = null, made = [];
  function label(it, id) { it.insertLabel('oracle.id', id); made.push(id); return it; }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    doc = app.documents.add(false);
    var vp = doc.viewPreferences;
    vp.horizontalMeasurementUnits = MeasurementUnits.POINTS;
    vp.verticalMeasurementUnits = MeasurementUnits.POINTS;
    vp.rulerOrigin = RulerOrigin.SPREAD_ORIGIN;
    var dp = doc.documentPreferences;
    dp.facingPages = true;
    dp.pageWidth = 595.276;
    dp.pageHeight = 841.89;
    dp.pagesPerDocument = 1;

    // same name as basic's "Oracle Blue" (90 50 0 0), other values
    var blue = doc.colors.add({ name: 'Oracle Blue', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [0, 50, 0, 0] });
    var green = doc.colors.add({ name: 'Oracle Green', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [70, 0, 90, 0] });
    var spot2 = doc.colors.add({ name: 'Oracle Spot 2', model: ColorModel.SPOT, space: ColorSpace.CMYK, colorValue: [0, 30, 100, 0] });
    var black = doc.swatches.itemByName('Black');
    var none = doc.swatches.itemByName('None');
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');

    var basicPara = doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle');
    basicPara.properties = { appliedFont: font, pointSize: 9 };
    var body = doc.paragraphStyles.add({ name: 'Oracle Body', appliedFont: font, pointSize: 14, leading: 20, fillColor: blue });
    var em = doc.characterStyles.add({ name: 'Oracle Em', fillColor: green, underline: true });
    var card = doc.objectStyles.add({ name: 'Oracle Card', enableFill: true, enableStroke: true, fillColor: green, strokeColor: black, strokeWeight: 2 });

    var base = doc.layers[0];
    base.name = 'Base';
    var other = doc.layers.add({ name: 'Other' });

    var p1 = doc.pages[0];
    function on(page, b) { var pb = page.bounds; return [pb[0] + b[0], pb[1] + b[1], pb[0] + b[2], pb[1] + b[3]]; }

    label(p1.rectangles.add(other, { geometricBounds: on(p1, [100, 300, 160, 400]), fillColor: blue, strokeColor: none }), 'o-blue');
    var g = label(p1.rectangles.add(base, { geometricBounds: on(p1, [180, 300, 240, 400]) }), 'o-card');
    g.appliedObjectStyle = card;
    label(p1.ovals.add(other, { geometricBounds: on(p1, [260, 300, 320, 400]), fillColor: spot2, strokeColor: none }), 'o-spot');
    var tf = label(p1.textFrames.add(other, { geometricBounds: on(p1, [340, 300, 440, 520]) }), 'o-text');
    tf.contents = 'Other body text with an emphasised word.';
    tf.parentStory.paragraphs[0].appliedParagraphStyle = body;
    tf.parentStory.characters.itemByRange(24, 33).appliedCharacterStyle = em; // 'emphasised'
    var tb = label(p1.textFrames.add(base, { geometricBounds: on(p1, [460, 300, 520, 520]) }), 'o-basic');
    tb.contents = 'Basic paragraph text from the other document.';
    tb.parentStory.paragraphs[0].appliedParagraphStyle = basicPara;
    var img = label(p1.rectangles.add(base, { geometricBounds: on(p1, [540, 300, 640, 450]), fillColor: none, strokeColor: none }), 'o-img');
    img.place(File(CONFIG.image));
    img.fit(FitOptions.PROPORTIONALLY);

    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) {
    errs.push(String(e.message) + ' @' + e.line);
  } finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errs.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  function q(s) { return '"' + String(s).split('\\').join('/').split('"').join("'") + '"'; }
  var a = [], b = [];
  for (var i = 0; i < made.length; i++) a.push(q(made[i]));
  for (var j = 0; j < errs.length; j++) b.push(q(errs[j]));
  return '{"ok":' + (errs.length == 0) + ',"made":[' + a.join(',') + '],"errors":[' + b.join(',') + ']}';
})();
