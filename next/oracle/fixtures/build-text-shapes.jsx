// Oracle fixture "text-shapes": text in frames that are not rectangles, to measure where
// InDesign puts each line (the textState of every scenario records the lines). Frames on two
// pages, each labelled `oracle.id`; Malgun Gothic 11 pt, auto leading, no hyphenation. Hidden
// document, preferences restored. Run with capture.py build text-shapes.
//   ts-oval         240 × 140 ellipse, insets 0
//   ts-oval-inset   the same with a 6 pt inset
//   ts-circle       300 × 300 circle, longer text
//   ts-triangle     240 × 180 triangle (apex up)
//   ts-hexagon      240 × 180 hexagon
//   ts-rounded      240 × 140 rectangle with 30 pt rounded corners
//   ts-oval-stroke  240 × 140 ellipse with a 4 pt centred stroke
//   ts-oval-leading 240 × 140 ellipse, leading 20 pt
//   ts-oval-cap     240 × 140 ellipse, first baseline at the cap height
//   ts-oval-2col    300 × 200 ellipse, 2 columns, gutter 12
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [], errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 2 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    var basic = doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle');
    basic.properties = { appliedFont: font, pointSize: 11, hyphenation: false };
    var black = doc.swatches.itemByName('Black');
    var TEXT = 'Oracle text frame. The quick brown fox jumps over the lazy dog. 다람쥐 헌 쳇바퀴에 타고파. ';
    function frame(page, id, bounds, shape, text) {
      var f = doc.pages[page].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id); made.push(id);
      f.contents = text;
      f.parentStory.appliedFont = font;
      f.parentStory.pointSize = 11;
      if (shape) f.convertShape(shape);
      return f;
    }
    var S = ConvertShapeOptions;
    frame(0, 'ts-oval', [40, 40, 180, 280], S.CONVERT_TO_OVAL, TEXT + TEXT);
    var inset = frame(0, 'ts-oval-inset', [40, 320, 180, 560], S.CONVERT_TO_OVAL, TEXT + TEXT);
    inset.textFramePreferences.insetSpacing = 6;
    frame(0, 'ts-circle', [220, 40, 520, 340], S.CONVERT_TO_OVAL, TEXT + TEXT + TEXT + TEXT + TEXT);
    frame(0, 'ts-triangle', [560, 40, 740, 280], S.CONVERT_TO_TRIANGLE, TEXT + TEXT + TEXT);
    frame(0, 'ts-hexagon', [560, 320, 740, 560], S.CONVERT_TO_POLYGON, TEXT + TEXT + TEXT);
    var rounded = frame(1, 'ts-rounded', [40, 40, 180, 280], null, TEXT + TEXT);
    rounded.properties = { topLeftCornerOption: CornerOptions.ROUNDED_CORNER, topRightCornerOption: CornerOptions.ROUNDED_CORNER,
      bottomLeftCornerOption: CornerOptions.ROUNDED_CORNER, bottomRightCornerOption: CornerOptions.ROUNDED_CORNER,
      topLeftCornerRadius: 30, topRightCornerRadius: 30, bottomLeftCornerRadius: 30, bottomRightCornerRadius: 30 };
    var stroked = frame(1, 'ts-oval-stroke', [40, 320, 180, 560], S.CONVERT_TO_OVAL, TEXT + TEXT);
    stroked.properties = { strokeColor: black, strokeWeight: 4 };
    var led = frame(1, 'ts-oval-leading', [220, 40, 360, 280], S.CONVERT_TO_OVAL, TEXT + TEXT);
    led.parentStory.leading = 20;
    var cap = frame(1, 'ts-oval-cap', [220, 320, 360, 560], S.CONVERT_TO_OVAL, TEXT + TEXT);
    cap.textFramePreferences.firstBaselineOffset = FirstBaseline.CAP_HEIGHT;
    var two = frame(1, 'ts-oval-2col', [400, 40, 600, 340], S.CONVERT_TO_OVAL, TEXT + TEXT + TEXT + TEXT);
    two.textFramePreferences.properties = { textColumnCount: 2, textColumnGutter: 12 };
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
