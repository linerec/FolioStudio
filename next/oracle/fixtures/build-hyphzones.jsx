(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc=null, errors=[];
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:600,pageHeight:600,pagesPerDocument:1};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular'};
    var body=doc.paragraphStyles.add({name:'Hyphen Body',appliedFont:font,fontStyle:'Regular',pointSize:12,leading:16,appliedLanguage:'English: USA',hyphenation:true,hyphenationZone:36,hyphenWeight:5,composer:'Adobe Paragraph Composer'});
    var text='Publication international communication typography information. Publication international communication typography information. Extraordinary organizational paragraph internationalization.';
    var align=[Justification.LEFT_ALIGN,Justification.CENTER_ALIGN,Justification.LEFT_JUSTIFIED];
    for(var i=0;i<3;i++){
      var f=doc.pages[0].textFrames.add({geometricBounds:[36,36+i*180,300,186+i*180]});
      f.insertLabel('oracle.id',['left','center','justified'][i]);f.contents=text;
      f.parentStory.texts[0].applyParagraphStyle(body,true);
      f.parentStory.texts[0].justification=align[i];
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e.message)+' @'+e.line);}
  finally{
    try{if(doc)doc.close(SaveOptions.NO);}catch(e2){errors.push(e2.message);}
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
  return '{"ok":'+(errors.length===0)+',"made":["left","center","justified"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
