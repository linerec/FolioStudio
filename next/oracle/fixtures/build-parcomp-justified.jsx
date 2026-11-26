(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  if(app.documents.length)return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.open(File(File(CONFIG.out).parent.fullName+'/parcomp-balance.idml'),false);
    var alignments=['LEFT_JUSTIFIED','CENTER_JUSTIFIED','RIGHT_JUSTIFIED','FULLY_JUSTIFIED'];
    for(var i=0;i<doc.pages[0].textFrames.length;i++){
      var f=doc.pages[0].textFrames[i],label=f.extractLabel('oracle.id');
      if(!label)continue;
      var index=Number(label.substr(7));
      f.parentStory.texts[0].justification=Justification[alignments[index]];
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["balance0","balance1","balance2","balance3"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
