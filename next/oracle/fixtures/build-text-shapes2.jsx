// Oracle fixture "text-shapes2": more frames that are not rectangles, for the two steps of
// InDesign's text area that text-shapes does not pin down: the inset (an offset path of the
// frame's shape) and columns cutting the shape (a pathfinder intersection). Same text setup as
// text-shapes (Malgun Gothic 11 pt, auto leading, no hyphenation). Hidden document,
// preferences restored. Run with capture.py build text-shapes2.
// Page 1:
//   ts2-rounded-inset    240 × 140, 10 pt rounded corners, 5 pt inset (as basic/tfo-insets-rounded)
//   ts2-rounded30-inset  240 × 140, 30 pt rounded corners, 6 pt inset
//   ts2-hexagon-inset    240 × 180 hexagon, 6 pt inset
//   ts2-triangle-inset   240 × 180 triangle, 6 pt inset
//   ts2-oval-3col        300 × 200 ellipse, 3 columns, gutter 12
//   ts2-oval-inset-2col  300 × 140 ellipse, 6 pt inset, 2 columns, gutter 12
// Page 2:
//   ts2-circle-2col      300 × 300 circle, 2 columns, gutter 12
//   ts2-rounded-2col     300 × 140, 30 pt rounded corners, 2 columns, gutter 12
//   ts2-oval-stroke-inset 240 × 140 ellipse, 4 pt centred stroke and 3 pt inset
//   ts2-oval-inset12     240 × 140 ellipse, 12 pt inset
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
    function times(n) { var s = ''; for (var i = 0; i < n; i++) s += TEXT; return s; }
    function frame(page, id, bounds, shape, text) {
      var f = doc.pages[page].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id); made.push(id);
      f.contents = text;
      f.parentStory.appliedFont = font;
      f.parentStory.pointSize = 11;
      if (shape) f.convertShape(shape);
      return f;
    }
    function rounded(f, r) {
      f.properties = { topLeftCornerOption: CornerOptions.ROUNDED_CORNER, topRightCornerOption: CornerOptions.ROUNDED_CORNER,
        bottomLeftCornerOption: CornerOptions.ROUNDED_CORNER, bottomRightCornerOption: CornerOptions.ROUNDED_CORNER,
        topLeftCornerRadius: r, topRightCornerRadius: r, bottomLeftCornerRadius: r, bottomRightCornerRadius: r };
    }
    function columns(f, n) { f.textFramePreferences.properties = { textColumnCount: n, textColumnGutter: 12 }; }
    var S = ConvertShapeOptions;
    var f;
    f = frame(0, 'ts2-rounded-inset', [40, 40, 180, 280], null, times(2)); rounded(f, 10); f.textFramePreferences.insetSpacing = 5;
    f = frame(0, 'ts2-rounded30-inset', [40, 320, 180, 560], null, times(2)); rounded(f, 30); f.textFramePreferences.insetSpacing = 6;
    f = frame(0, 'ts2-hexagon-inset', [220, 40, 400, 280], S.CONVERT_TO_POLYGON, times(3)); f.textFramePreferences.insetSpacing = 6;
    f = frame(0, 'ts2-triangle-inset', [220, 320, 400, 560], S.CONVERT_TO_TRIANGLE, times(3)); f.textFramePreferences.insetSpacing = 6;
    f = frame(0, 'ts2-oval-3col', [440, 40, 640, 340], S.CONVERT_TO_OVAL, times(4)); columns(f, 3);
    f = frame(0, 'ts2-oval-inset-2col', [660, 40, 800, 340], S.CONVERT_TO_OVAL, times(3)); f.textFramePreferences.insetSpacing = 6; columns(f, 2);
    f = frame(1, 'ts2-circle-2col', [40, 40, 340, 340], S.CONVERT_TO_OVAL, times(6)); columns(f, 2);
    f = frame(1, 'ts2-rounded-2col', [380, 40, 520, 340], null, times(3)); rounded(f, 30); columns(f, 2);
    f = frame(1, 'ts2-oval-stroke-inset', [560, 40, 700, 280], S.CONVERT_TO_OVAL, times(2)); f.properties = { strokeColor: black, strokeWeight: 4 }; f.textFramePreferences.insetSpacing = 3;
    f = frame(1, 'ts2-oval-inset12', [560, 320, 700, 560], S.CONVERT_TO_OVAL, times(2)); f.textFramePreferences.insetSpacing = 12;
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
