// Hidden, one-page placement fixture; no Adobe fonts or external artwork.
(function () {
  var CONFIG=CONFIG_PLACEHOLDER;
  if(app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, d=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    d=app.documents.add(false);
    d.documentPreferences.properties={facingPages:false,pageWidth:612,pageHeight:792,pagesPerDocument:1};
    d.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    d.textDefaults.appliedFont=app.fonts.itemByName('Arial');
    d.pageItemDefaults.properties={fillColor:d.swatches.itemByName('None'),strokeColor:d.swatches.itemByName('None'),strokeWeight:0};
    var p=d.pages[0], made=[];
    function frame(id,box,fill) {
      var r=p.rectangles.add({geometricBounds:box,contentType:ContentType.GRAPHIC_TYPE,appliedObjectStyle:d.objectStyles.itemByName('$ID/[None]'),fillColor:fill || d.swatches.itemByName('None'),strokeColor:d.swatches.itemByName('None'),strokeWeight:0});
      r.insertLabel('oracle.id',id);made.push(id);return r;
    }
    frame('frame',[60,60,240,360]);
    var blue=d.colors.add({name:'PDF blue',space:ColorSpace.CMYK,model:ColorModel.PROCESS,colorValue:[100,0,0,0]});
    frame('background',[300,60,600,360],blue);
    frame('overprint',[300,60,600,360]);
    d.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["frame","background","overprint"]}';
  } catch(e) {return '{"ok":false,"error":"'+String(e).replace(/"/g,"'")+'"}';}
  finally {if(d)d.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
