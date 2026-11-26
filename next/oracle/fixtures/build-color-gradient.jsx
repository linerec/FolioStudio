(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = {ui:sp.userInteractionLevel, unit:sp.measurementUnit, redraw:sp.enableRedraw}, doc;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS; sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pagesPerDocument:1,pageWidth:320,pageHeight:240};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    var black=doc.swatches.itemByName('Black'), paper=doc.swatches.itemByName('Paper');
    var blue=doc.colors.add({name:'Blue',model:ColorModel.PROCESS,space:ColorSpace.CMYK,colorValue:[100,40,0,0]});
    var orange=doc.colors.add({name:'Orange',model:ColorModel.PROCESS,space:ColorSpace.CMYK,colorValue:[0,60,100,0]});
    var g=doc.gradients.add({name:'Ramp',type:GradientType.LINEAR});
    g.gradientStops[0].properties={stopColor:paper,location:0,midpoint:50};
    g.gradientStops[1].properties={stopColor:black,location:100,midpoint:50};
    var r=doc.pages[0].rectangles.add({geometricBounds:[30,30,130,290],fillColor:g,strokeColor:doc.swatches.itemByName('None'),strokeWeight:0});
    r.insertLabel('oracle.id','ramp');
    var b=doc.pages[0].rectangles.add({geometricBounds:[160,30,210,290],fillColor:blue,strokeColor:orange,strokeWeight:4});
    b.insertLabel('oracle.id','solid');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["ramp","solid"]}';
  } catch(e) { return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}'; }
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
