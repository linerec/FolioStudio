(function () {
  // Flattener Preview test page (windowmenu-c1/-c2): A opaque, B opaque under C, C 50 % opacity, D drop shadow,
  // E multiply over F, G text above E, H stroke above C, I/J alone, K1 text / K2 stroke / K3 gradient / K4 image
  // under the 50 % objects L / L2 / L3. Same geometry as artifacts/windowmenu-c1-20261023/tools/testdoc.py.
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, doc=null, made=[];
  function label(it,name){it.insertLabel('oracle.id',name);made.push(name);return it;}
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:500,pageHeight:400,pagesPerDocument:1};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    function col(n, c) { return doc.colors.add({name: n, model: ColorModel.PROCESS, space: ColorSpace.CMYK, colorValue: c}); }
    var red = col('R', [0, 100, 100, 0]), yel = col('Y', [0, 0, 100, 0]), blu = col('B', [100, 80, 0, 0]), grn = col('G', [80, 0, 100, 0]),
        cya = col('C', [100, 0, 0, 0]), mag = col('M', [0, 100, 0, 0]), blk = doc.swatches.itemByName('Black'), none = doc.swatches.itemByName('None');
    var pg = doc.pages[0];
    function rect(b, fill, name) { return label(pg.rectangles.add({geometricBounds: b, fillColor: fill, strokeColor: none, strokeWeight: 0}), name); }
    function text(b, s, name) { var t = label(pg.textFrames.add({geometricBounds: b}), name); t.contents = s; t.texts[0].pointSize = 18; t.texts[0].appliedFont = app.fonts.itemByName('Arial'); return t; }
    rect([40, 40, 100, 120], red, 'A');
    rect([140, 40, 200, 120], yel, 'B');
    var C = rect([160, 70, 220, 150], blu, 'C'); C.transparencySettings.blendingSettings.opacity = 50;
    var D = rect([40, 200, 100, 280], grn, 'D'); D.transparencySettings.dropShadowSettings.mode = ShadowMode.DROP;
    rect([140, 200, 200, 280], cya, 'F');
    var E = rect([160, 230, 220, 310], mag, 'E'); E.transparencySettings.blendingSettings.blendMode = BlendMode.MULTIPLY;
    text([200, 100, 240, 260], 'Over transparency', 'G');
    label(pg.rectangles.add({geometricBounds: [150, 110, 190, 190], fillColor: none, strokeColor: blk, strokeWeight: 6}), 'H');
    rect([300, 40, 360, 120], red, 'I');
    text([300, 200, 340, 400], 'Unaffected text', 'J');
    text([60, 330, 90, 470], 'Text beneath', 'K1');
    label(pg.rectangles.add({geometricBounds: [100, 340, 130, 400], fillColor: none, strokeColor: blk, strokeWeight: 6}), 'K2');
    var L = rect([70, 350, 120, 440], mag, 'L'); L.transparencySettings.blendingSettings.opacity = 50;
    var gr = doc.gradients.add(); gr.name = 'Gd'; gr.type = GradientType.LINEAR; gr.gradientStops[0].stopColor = red; gr.gradientStops[1].stopColor = blu;
    label(pg.rectangles.add({geometricBounds: [150, 330, 200, 400], fillColor: gr, strokeColor: none, strokeWeight: 0}), 'K3');
    var L2 = rect([170, 360, 230, 440], cya, 'L2'); L2.transparencySettings.blendingSettings.opacity = 50;
    var K4 = label(pg.rectangles.add({geometricBounds: [240, 330, 290, 400], strokeColor: none, strokeWeight: 0}), 'K4');
    K4.place(File(CONFIG.image)); K4.fit(FitOptions.FILL_PROPORTIONALLY);
    // embedded, so the fixture holds the picture and needs no link folder
    K4.allGraphics[0].itemLink.unlink();
    var L3 = rect([260, 350, 300, 430], yel, 'L3'); L3.transparencySettings.blendingSettings.opacity = 50;
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
