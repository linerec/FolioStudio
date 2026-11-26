// 픽스처를 InDesign 으로 열어 그대로 IDML 로 다시 내보낸다(열 때마다 개체 번호가 바뀌는 픽스처를 InDesign 번호로 고정; build-b 2026-11-21).
(function(){
  var CONFIG=CONFIG_PLACEHOLDER,sp=app.scriptPreferences,saved={ui:sp.userInteractionLevel},doc=null,error='';
  try{
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    doc=app.open(File(CONFIG.source),false);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){error=String(e).replace(/"/g,"'");}finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;}
  return '{"ok":'+(!error)+',"error":"'+error+'"}';
}());
