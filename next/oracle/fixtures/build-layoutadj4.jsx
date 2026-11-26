// Oracle fixture "layoutadj4": File > Adjust Layout on a single page document (300 x 400 pt, margins 30) with shapes that are not
// boxes (polygons, curves, polylines, rotated frames), lines, groups and objects across the margins. Every item is labelled
// `oracle.id`. Run with capture.py build layoutadj4.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length != 0) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function label(it, id) { it.insertLabel('oracle.id', id); made.push(id); return it; }
  function rect(page, b, weight) { var r = page.rectangles.add({ geometricBounds: b }); r.strokeWeight = weight; return r; }
  function poly(page, pts, weight) { var g = page.polygons.add({}); g.paths[0].entirePath = pts; g.strokeWeight = weight; return g; }
  function line(page, pts, weight) { var l = page.graphicLines.add({}); l.paths[0].entirePath = pts; l.strokeWeight = weight; return l; }
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
    dp.facingPages = false; dp.pageWidth = 300; dp.pageHeight = 400; dp.pagesPerDocument = 1;
    doc.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    var p = doc.pages[0];
    p.marginPreferences.properties = { top: 30, bottom: 30, left: 30, right: 30 };
    label(line(p, [[50, 60], [120, 100], [90, 160]], 2), 'o-polyline');
    var bz = poly(p, [[150, 60], [210, 60], [210, 120], [150, 120]], 1);
    bz.paths[0].pathPoints[0].rightDirection = [170, 45]; bz.paths[0].pathPoints[1].leftDirection = [190, 45];
    label(bz, 'o-curvy');
    label(poly(p, [[60, 200], [70, 220], [90, 220], [74, 232], [80, 252], [60, 240], [40, 252], [46, 232], [30, 220], [50, 220]], 1), 'o-star');
    label(poly(p, [[110, 200], [190, 200], [210, 240], [90, 240]], 1), 'o-trapezoid');
    var rr = rect(p, [260, 60, 300, 140], 1);
    rr.topLeftCornerOption = CornerOptions.ROUNDED_CORNER; rr.topLeftCornerRadius = 8;
    label(rr, 'o-rounded');
    var r90 = rect(p, [200, 160, 240, 240], 2); r90.rotationAngle = 90; label(r90, 'o-rect90');
    var r30 = rect(p, [300, 150, 330, 200], 1); r30.rotationAngle = 30; label(r30, 'o-rect30');
    var tfr = p.textFrames.add({ geometricBounds: [100, 160, 140, 250], contents: 'Rotated text frame' }); tfr.rotationAngle = 30; label(tfr, 'o-text30');
    var tfo = p.textFrames.add({ geometricBounds: [60, 220, 100, 260], contents: 'Oval' }); tfo.convertShape(ConvertShapeOptions.CONVERT_TO_OVAL); label(tfo, 'o-text-oval');
    label(line(p, [[200, 330], [260, 330]], 4), 'o-hline');
    label(line(p, [[40, 340], [100, 365]], 1), 'o-diagline');
    label(line(p, [[270, 50], [270, 150]], 1), 'o-vline');
    label(poly(p, [[220, 280], [260, 280], [260, 320], [220, 320]], 2), 'o-boxpoly');
    label(poly(p, [[200, 120], [230, 120], [260, 120], [260, 150], [200, 150]], 1), 'o-boxpoly5');
    // across the margins
    var ov1 = p.ovals.add({ geometricBounds: [100, 10, 140, 60] }); ov1.strokeWeight = 1; label(ov1, 'o-oval-lead');
    var ov2 = p.ovals.add({ geometricBounds: [10, 100, 50, 150] }); ov2.strokeWeight = 1; label(ov2, 'o-oval-top');
    var ov3 = p.ovals.add({ geometricBounds: [10, 10, 390, 290] }); ov3.strokeWeight = 1; label(ov3, 'o-oval-big');
    label(poly(p, [[20, 250], [60, 250], [40, 290]], 1), 'o-tri-lead');
    // groups
    var a1 = rect(p, [40, 50, 80, 120], 0.5), a2 = rect(p, [60, 100, 100, 160], 2);
    label(p.groups.add([a1, a2]), 'o-group2');
    var b1 = rect(p, [120, 50, 150, 110], 1), b2 = p.ovals.add({ geometricBounds: [130, 90, 170, 150] }); b2.strokeWeight = 1;
    var gi = p.groups.add([b1, b2]); var b3 = rect(p, [160, 60, 190, 100], 1);
    label(p.groups.add([gi, b3]), 'o-group-nested');
    var c1 = p.textFrames.add({ geometricBounds: [200, 50, 240, 140], contents: 'Text in group' }), c2 = rect(p, [230, 100, 270, 160], 1);
    label(p.groups.add([c1, c2]), 'o-group-text');
    var d1 = rect(p, [280, 50, 310, 110], 1), d2 = rect(p, [290, 100, 330, 130], 1); d2.rotationAngle = 30;
    label(p.groups.add([d1, d2]), 'o-group-rot');
    var e1 = line(p, [[170, 50], [220, 120]], 1), e2 = rect(p, [60, 180, 100, 250], 1);
    label(p.groups.add([e1, e2]), 'o-group-line');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + '["' + made.join('","') + '"]}';
  } catch (e) { return '{"ok":false,"error":"' + String(e.message + ' @' + e.line).replace(/["\\\r\n]/g, ' ') + '"}'; }
  finally { if (doc) doc.close(SaveOptions.NO); sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw; }
})();
