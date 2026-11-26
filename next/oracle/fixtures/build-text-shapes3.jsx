// Oracle fixture "text-shapes3": the same shaped frame at fractional positions. Rounded-corner
// (10 pt) text frames with a 5 pt inset, 240 x 140, at x = 40 + f (and 320 + f) for twenty
// fractions f, labelled ts3-<i>. InDesign floors a line's edge in the column's own coordinates
// after its offset path snaps to AGM's grid, so the fraction moves the first line by 1 pt for 7
// of them (artifacts/shaped-frames-20261021/measure/noise.json, static-coords/findings.txt).
// Hidden document, preferences restored. Run with capture.py build text-shapes3.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length != 0) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  function json(v) {
    if (v === null || v === undefined) return 'null';
    var t = typeof v;
    if (t === 'number') return isFinite(v) ? String(v) : 'null';
    if (t === 'boolean') return v ? 'true' : 'false';
    if (t === 'string') return '"' + v.split('\\').join('\\\\').split('"').join('\\"').split('\r').join('\\r').split('\n').join('\\n') + '"';
    if (v instanceof Array) { var a = []; for (var i = 0; i < v.length; i++) a.push(json(v[i])); return '[' + a.join(',') + ']'; }
    var o = []; for (var k in v) if (v.hasOwnProperty(k)) o.push(json(k) + ':' + json(v[k]));
    return '{' + o.join(',') + '}';
  }
  var doc = null, result = { ok: true, frames: {} }, made = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 2 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle').properties = { appliedFont: font, pointSize: 11, hyphenation: false };
    var TEXT = 'Oracle text frame. The quick brown fox jumps over the lazy dog. 다람쥐 헌 쳇바퀴에 타고파. ';
    var fr = [0, 0.1, 0.2, 0.25, 0.276, 0.3, 0.4, 0.5, 0.6, 0.7, 0.75, 0.8, 0.9, 0.95, 0.999, 0.001, 0.05, 0.55, 0.45, 0.35];
    for (var i = 0; i < fr.length; i++) {
      var page = doc.pages[i < 10 ? 0 : 1];
      var k = i % 10;
      var x = (k % 2 == 0 ? 40 : 320) + fr[i];
      var y = 40 + Math.floor(k / 2) * 160;
      var f = page.textFrames.add({ geometricBounds: [y, x, y + 140, x + 240] });
      f.insertLabel('oracle.id', 'ts3-' + i); made.push('ts3-' + i);
      f.contents = TEXT + TEXT;
      f.parentStory.appliedFont = font;
      f.parentStory.pointSize = 11;
      f.properties = { topLeftCornerOption: CornerOptions.ROUNDED_CORNER, topRightCornerOption: CornerOptions.ROUNDED_CORNER,
        bottomLeftCornerOption: CornerOptions.ROUNDED_CORNER, bottomRightCornerOption: CornerOptions.ROUNDED_CORNER,
        topLeftCornerRadius: 10, topRightCornerRadius: 10, bottomLeftCornerRadius: 10, bottomRightCornerRadius: 10 };
      f.textFramePreferences.insetSpacing = 5;
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) {
    result.ok = false; result.error = String(e) + ' line ' + e.line;
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  result.made = made;
  return json(result);
})();
