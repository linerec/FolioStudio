// Oracle fixture "tablestyles": table and cell styles to apply, edit and delete. A new document
// without a window (single pages, A4, points, 1 page), saved as IDML to CONFIG.out and closed
// without saving; preferences restored. Run with run-jsx.ps1.
//   tbl       a 4 x 3 table (one header row) after a paragraph, cells "h1".."c3", 11 pt
//   tbl2      a 3 x 6 table (one header row, five body rows), cells "p1".."q6"
// Styles: paragraph "Oracle Cell Text"; cell styles "Oracle Cell Orange", "Oracle Cell Blue", "Oracle Cell Child"
// (based on Orange); table styles "Oracle Zebra" (alternating row fills, header region Blue), "Oracle Frame".
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

    // object ids of the fixture: InDesign renumbers a file's ids at open when they collide with the ids it gives its own default objects
    // (the document's styles, swatches ...), and that zone moves from session to session. Spend ids first so that every object below is
    // numbered above it.
    for (var spend = 0; spend < 600; spend++) page.rectangles.add({ geometricBounds: [0, 0, 1, 1] });
    page.rectangles.everyItem().remove();
    var cellText = doc.paragraphStyles.add({ name: 'Oracle Cell Text', pointSize: 14, appliedFont: font });
    var orange = doc.swatches.itemByName('Oracle Orange'), blue = doc.swatches.itemByName('Oracle Blue');
    var csOrange = doc.cellStyles.add({ name: 'Oracle Cell Orange', fillColor: orange, bottomEdgeStrokeWeight: 2, bottomEdgeStrokeColor: blue, verticalJustification: VerticalJustification.CENTER_ALIGN });
    var csBlue = doc.cellStyles.add({ name: 'Oracle Cell Blue', fillColor: blue, fillTint: 50, appliedParagraphStyle: cellText });
    doc.cellStyles.add({ name: 'Oracle Cell Child', basedOn: csOrange, fillTint: 40, topInset: 8 });
    doc.tableStyles.add({ name: 'Oracle Zebra', startRowFillColor: orange, startRowFillCount: 1, startRowFillTint: 20, endRowFillCount: 1, headerRegionCellStyle: csBlue, spaceBefore: 6 });
    doc.tableStyles.add({ name: 'Oracle Frame', topBorderStrokeWeight: 3, bottomBorderStrokeWeight: 3, topBorderStrokeColor: blue, bottomBorderStrokeColor: blue });

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

    var g = label(page.textFrames.add({ geometricBounds: on([420, 60, 760, 400]) }), 'tbl2');
    g.parentStory.appliedFont = font;
    g.parentStory.pointSize = 11;
    var t2 = g.parentStory.insertionPoints[-1].tables.add({ headerRowCount: 1, bodyRowCount: 5, columnCount: 3 });
    for (var r2 = 0; r2 < 6; r2++) for (var c2 = 0; c2 < 3; c2++) t2.rows[r2].cells[c2].contents = (r2 == 0 ? 'q' : 'p') + (r2 + 1) + (c2 + 1);

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
