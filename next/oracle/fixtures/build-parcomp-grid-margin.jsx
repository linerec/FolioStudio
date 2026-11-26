(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  if(app.documents.length)return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.open(File(File(CONFIG.out).parent.fullName+'/parcomp-grid.idml'),false);
    doc.pages[0].marginPreferences.top=25;
    doc.gridPreferences.properties={baselineStart:3,baselineGridRelativeOption:BaselineGridRelativeOption.TOP_OF_MARGIN_OF_BASELINE_GRID_RELATIVE_OPTION};
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["grid"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
