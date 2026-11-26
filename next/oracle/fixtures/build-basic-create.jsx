// Derived drawing fixture. Run capture.py build basic-create; basic.idml must exist.
(function () {
  var CONFIG=CONFIG_PLACEHOLDER;
  if(app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var d=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    d=app.open(File(File(CONFIG.out).parent.fsName+'/basic.idml'),false);
    d.textDefaults.appliedFont=app.fonts.itemByName('Arial');
    d.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":[]}';
  } catch(e) {return '{"ok":false,"error":"'+String(e).replace(/"/g,"'")+'"}';}
  finally {if(d)d.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
