// Oracle fixture "tables": text frames for table editing, each labelled `oracle.id`. A new
// document without a window (single pages, A4, points, 1 page), saved as IDML to CONFIG.out
// and closed without saving; preferences restored. Run with run-jsx.ps1.
//   tbl       a 4 × 3 table (one header row) after a paragraph, cells "h1".."c3", 11 pt
//   tbl-conv  tab-separated text to convert to a table
//   tbl-new   an empty frame to insert a table into
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
    dp.facingPages = false;
    dp.pageWidth = 595.276;
    dp.pageHeight = 841.89;
    dp.pagesPerDocument = 1;
    doc.marginPreferences.properties = { top: 36, bottom: 36, left: 36, right: 36 };
    doc.colors.add({ name: 'Oracle Orange', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [0, 60, 90, 0] });
    doc.colors.add({ name: 'Oracle Blue', model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: [90, 50, 0, 0] });
    var page = doc.pages[0];
    function on(b) { var pb = page.bounds; return [pb[0] + b[0], pb[1] + b[1], pb[0] + b[2], pb[1] + b[3]]; }
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    // new cells take [Basic Paragraph]: a font Folio has (InDesign's own fonts are not for Folio)
    var basic = doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle');
    basic.appliedFont = font;
    basic.pointSize = 11;

    var f = label(page.textFrames.add({ geometricBounds: on([60, 60, 360, 400]) }), 'tbl');
    f.contents = 'Before the table.\r';
    f.parentStory.appliedFont = font;
    f.parentStory.pointSize = 11;
    var t = f.parentStory.insertionPoints[-1].tables.add({ headerRowCount: 1, bodyRowCount: 3, columnCount: 3 });
    var names = [['h1', 'h2', 'h3'], ['a1', 'a2', 'a3'], ['b1', 'b2', 'b3'], ['c1', 'c2', 'c3']];
    for (var r = 0; r < 4; r++) for (var c = 0; c < 3; c++) t.rows[r].cells[c].contents = names[r][c];
    f.parentStory.insertionPoints[-1].contents = '\rAfter the table.';
    f.parentStory.appliedFont = font;
    f.parentStory.pointSize = 11;

    var conv = label(page.textFrames.add({ geometricBounds: on([400, 60, 520, 400]) }), 'tbl-conv');
    conv.contents = 'one\ttwo\tthree\rfour\tfive\tsix';
    conv.parentStory.appliedFont = font;
    conv.parentStory.pointSize = 11;

    var empty = label(page.textFrames.add({ geometricBounds: on([560, 60, 760, 400]) }), 'tbl-new');
    empty.parentStory.appliedFont = font;
    empty.parentStory.pointSize = 11;

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
