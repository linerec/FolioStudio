(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  if(app.documents.length)return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.open(File(File(CONFIG.out).parent.fullName+'/parcomp-grid.idml'),false);
    var original=doc.pages[0].textFrames[0],angles=[0,15,90,180];
    original.geometricBounds=[36,36,216,216];
    var frames=[original,original.duplicate(),original.duplicate(),original.duplicate()];
    for(var i=0;i<frames.length;i++){
      var f=frames[i];f.insertLabel('oracle.id','grid'+angles[i]);f.rotationAngle=angles[i];
      var b=f.geometricBounds,x=i%2?330:50,y=i<2?50:400;
      f.move(undefined,[x-b[1],y-b[0]]);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["grid0","grid15","grid90","grid180"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
