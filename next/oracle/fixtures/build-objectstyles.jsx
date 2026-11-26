(function () {
  var CONFIG=CONFIG_PLACEHOLDER;
  if(app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, d=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    d=app.open(File(File(CONFIG.out).parent.fsName+'/basic-styles.idml'),false);
    d.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    var methods=[], sm=d.objectStyles[0].reflect.methods, pm=d.rectangles[0].reflect.methods;
    for(var i=0;i<sm.length;i++) methods.push('style.'+sm[i].name);
    for(var j=0;j<pm.length;j++) if(/style|override/i.test(pm[j].name)) methods.push('item.'+pm[j].name);
    return '{"ok":true,"made":["objectstyles"],"methods":"'+methods.join(',')+'"}';
  } catch(e) {return '{"ok":false,"error":"'+String(e).replace(/"/g,"'")+'"}';}
  finally {if(d)d.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
