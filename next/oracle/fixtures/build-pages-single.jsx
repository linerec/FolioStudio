// Same page-owned objects with Facing Pages disabled, measured from the pages fixture.
(function(){
  var CONFIG=CONFIG_PLACEHOLDER,doc=null;
  if(app.documents.length)return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences,saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try{
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.open(File(File(CONFIG.out).parent.fsName+'/pages.idml'),false);
    doc.documentPreferences.facingPages=false;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["single-pages"]}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
