// Drawing with document text defaults, using Windows' Malgun Gothic.
(function () {
  var CONFIG=CONFIG_PLACEHOLDER;
  if(app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences,saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw},d=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    d=app.open(File(File(CONFIG.out).parent.fsName+'/basic.idml'),false);
    d.textDefaults.appliedFont=app.fonts.itemByName('Malgun Gothic');
    d.textDefaults.pointSize=14;d.textDefaults.leading=18;d.textDefaults.spaceAfter=7;
    d.textFramePreferences.properties={textColumnCount:2,textColumnGutter:18,insetSpacing:[3,4,5,6]};
    d.pageItemDefaults.fillColor=d.swatches.itemByName('Oracle Blue');
    d.pageItemDefaults.strokeColor=d.swatches.itemByName('Black');d.pageItemDefaults.strokeWeight=3;
    d.activeLayer=d.layers.itemByName('Top');
    d.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":[]}';
  } catch(e) {return '{"ok":false,"error":"'+String(e).replace(/"/g,"'")+'"}';}
  finally {if(d)d.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
