// basic.idml 에 재단 여유(위 3, 아래 6, 안쪽 9, 바깥쪽 12 pt)만 바꿔 basic-bleed.idml 로 내보낸다 (정렬 패널 '재단 여유에 정렬' 대조용).
(function(){
  var CONFIG=CONFIG_PLACEHOLDER,sp=app.scriptPreferences,saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit},doc=null,error='';
  try{
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;
    doc=app.open(File(CONFIG.source),false);
    var dp=doc.documentPreferences;dp.documentBleedUniformSize=false;
    dp.documentBleedTopOffset=3;dp.documentBleedBottomOffset=6;dp.documentBleedInsideOrLeftOffset=9;dp.documentBleedOutsideOrRightOffset=12;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){error=String(e).replace(/"/g,"'");}finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;}
  return '{"ok":'+(!error)+',"error":"'+error+'"}';
}());
