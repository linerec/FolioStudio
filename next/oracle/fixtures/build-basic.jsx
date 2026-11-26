// Oracle fixture "basic": the objects editing touches most, each labelled `oracle.id` so a
// scenario can name it in InDesign and in Folio alike. A new document without a window
// (facing pages, A4, points, 3 pages, two layers, a parent-page item), saved as IDML to
// CONFIG.out and closed without saving; preferences restored. Run with run-jsx.ps1.
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
    dp.pagesPerDocument = 3;
    doc.marginPreferences.properties = { top: 36, bottom: 36, left: 36, right: 36 };

    var orange = doc.colors.add({ name: 'Oracle Orange', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [0, 60, 90, 0] });
    var blue = doc.colors.add({ name: 'Oracle Blue', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [90, 50, 0, 0] });
    var spot = doc.colors.add({ name: 'Oracle Spot', model: ColorModel.SPOT, space: ColorSpace.CMYK, colorValue: [0, 100, 0, 0] });
    var black = doc.swatches.itemByName('Black');
    var none = doc.swatches.itemByName('None');

    var base = doc.layers[0];
    base.name = 'Base';
    var top = doc.layers.add({ name: 'Top' });
    var locked = doc.layers.add({ name: 'Locked layer' });

    var p1 = doc.pages[0], p2 = doc.pages[1], p3 = doc.pages[2];
    function on(page, b) { var pb = page.bounds; return [pb[0] + b[0], pb[1] + b[1], pb[0] + b[2], pb[1] + b[3]]; }

    // page 1 (right page alone in the first spread)
    label(p1.rectangles.add(base, { geometricBounds: on(p1, [60, 60, 160, 220]), fillColor: none, strokeColor: black, strokeWeight: 1 }), 'r-plain');
    label(p1.rectangles.add(base, { geometricBounds: on(p1, [60, 260, 160, 420]), fillColor: orange, strokeColor: none }), 'r-fill');
    var rot = label(p1.rectangles.add(base, { geometricBounds: on(p1, [200, 60, 300, 220]), fillColor: blue, strokeColor: black, strokeWeight: 2 }), 'r-rot');
    rot.rotationAngle = 30;
    var sh = label(p1.rectangles.add(base, { geometricBounds: on(p1, [200, 300, 300, 460]), fillColor: none, strokeColor: blue, strokeWeight: 3 }), 'r-shear');
    sh.shearAngle = 15;
    label(p1.ovals.add(base, { geometricBounds: on(p1, [340, 60, 440, 200]), fillColor: spot, strokeColor: none }), 'oval');
    label(p1.polygons.add(base, { geometricBounds: on(p1, [340, 260, 440, 380]), numberOfSides: 6, fillColor: none, strokeColor: black, strokeWeight: 1 }), 'poly');
    label(p1.graphicLines.add(base, { geometricBounds: on(p1, [480, 60, 520, 300]), strokeColor: black, strokeWeight: 2 }), 'line');
    var ga = p1.rectangles.add(base, { geometricBounds: on(p1, [560, 60, 620, 160]), fillColor: orange, strokeColor: none });
    var gb = p1.rectangles.add(base, { geometricBounds: on(p1, [580, 120, 660, 240]), fillColor: blue, strokeColor: none });
    label(ga, 'grp-a'); label(gb, 'grp-b');
    label(p1.groups.add([ga, gb]), 'grp');
    var lk = label(p1.rectangles.add(base, { geometricBounds: on(p1, [560, 300, 620, 420]), fillColor: orange, strokeColor: none }), 'r-locked');
    lk.locked = true;
    var hd = label(p1.rectangles.add(base, { geometricBounds: on(p1, [640, 300, 700, 420]), fillColor: blue, strokeColor: none }), 'r-hidden');
    hd.visible = false;
    label(p1.rectangles.add(top, { geometricBounds: on(p1, [720, 60, 780, 200]), fillColor: blue, strokeColor: black, strokeWeight: 0.5 }), 'r-top');
    label(p1.rectangles.add(locked, { geometricBounds: on(p1, [720, 260, 780, 400]), fillColor: orange, strokeColor: none }), 'r-on-locked');

    // page 2 (left page of the second spread): text frames, one threaded into page 3
    var tf = label(p2.textFrames.add(base, { geometricBounds: on(p2, [60, 60, 200, 300]) }), 'tf');
    tf.contents = 'Oracle text frame. The quick brown fox jumps over the lazy dog. 다람쥐 헌 쳇바퀴에 타고파.';
    var t1 = label(p2.textFrames.add(base, { geometricBounds: on(p2, [240, 60, 320, 300]) }), 'tf-1');
    var t2 = label(p3.textFrames.add(base, { geometricBounds: on(p3, [240, 60, 400, 300]) }), 'tf-2');
    t1.nextTextFrame = t2;
    var longText = [];
    for (var i = 0; i < 18; i++) longText.push('Threaded line ' + (i + 1) + ' flows from the first frame into the second.');
    t1.contents = longText.join('\r');
    // a system font (InDesign's own fonts are not for Folio): Malgun Gothic, Latin and Hangul
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    tf.parentStory.appliedFont = font;
    t1.parentStory.appliedFont = font;
    tf.parentStory.pointSize = 11;
    t1.parentStory.pointSize = 10;

    // page 3 (right page of the second spread): a placed image, scaled and offset
    var img = label(p3.rectangles.add(base, { geometricBounds: on(p3, [60, 60, 200, 260]), fillColor: none, strokeColor: black, strokeWeight: 1 }), 'img');
    var g = img.place(File(CONFIG.image))[0];
    g.horizontalScale = 75; g.verticalScale = 75;
    g.move(undefined, [10, 8]);
    label(p3.rectangles.add(base, { geometricBounds: on(p3, [480, 60, 560, 220]), fillColor: orange, strokeColor: none }), 'r-p3');

    // a parent-page item on A-Parent (both sides)
    var parent = doc.masterSpreads[0];
    label(parent.pages[1].rectangles.add(base, { geometricBounds: on(parent.pages[1], [800, 36, 820, 559]), fillColor: blue, strokeColor: none }), 'parent-bar');

    locked.locked = true;
    doc.activeLayer = base;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) {
    errs.push(String(e.message) + ' @' + e.line);
  } finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errs.push('close: ' + e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  function q(s) { return '"' + String(s).split(String.fromCharCode(92)).join('/').split('"').join("'") + '"'; }
  var m = []; for (var j = 0; j < made.length; j++) m.push(q(made[j]));
  var e = []; for (var k = 0; k < errs.length; k++) e.push(q(errs[k]));
  return '{"ok":' + (errs.length == 0) + ',"made":[' + m.join(',') + '],"errors":[' + e.join(',') + '],"docsAfter":' + app.documents.length + '}';
})();
