(function () {
  var CONFIG=CONFIG_PLACEHOLDER;
  if(app.documents.length)return '{"ok":false,"error":"documents open"}';
  if(!app.userName||app.userName==='Unknown User Name'||app.userName==='$ID/Unknown User Name')return '{"ok":false,"error":"default username cannot be restored through the DOM; fixture build stopped before changing it"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw,user:app.userName},doc=null,result='';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.textDefaults.properties={appliedFont:font,pointSize:12,leading:16};
    doc.paragraphStyles[1].properties={appliedFont:font,pointSize:12,leading:16,hyphenation:false};
    for(var i=0;i<2;i++) {
      var frame=doc.pages[0].textFrames.add({geometricBounds:[36+i*180,36,180+i*180,324]});
      frame.insertLabel('oracle.id','body-'+i);
      var story=frame.parentStory;story.contents='ABCDE';story.trackChanges=true;
      app.userName='Reviewer A';story.insertionPoints[2].contents='xy';
      app.userName='Reviewer B';story.characters.itemByRange(4,5).remove();
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    result='{"ok":true,"made":["body-0","body-1"]}';
  } catch(e) {result='{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
  finally {if(doc)doc.close(SaveOptions.NO);try{app.userName=saved.user;}catch(e){result='{"ok":false,"error":"restore user: '+String(e.message).replace(/["\\\r\n]/g,' ')+'"}';}sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
  return result;
})();
