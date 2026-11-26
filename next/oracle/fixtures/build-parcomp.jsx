(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  if(app.documents.length)return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:595,pageHeight:842,pagesPerDocument:1};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular'};
    var base=doc.paragraphStyles.add({name:'Parcomp Base',appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular',pointSize:12,leading:18,hyphenation:false,composer:'$ID/HL Composer',justification:Justification.LEFT_JUSTIFIED});
    var child=doc.paragraphStyles.add({name:'Parcomp Child',basedOn:base});
    var data=[['latin','The quick brown fox jumps over the lazy dog. Publishing requires careful spacing between words and consistent line breaks across a narrow column.\rA short title with several uneven words.'],['korean','“한글 문장부호와 인용부호” 문장을 English words 함께 씁니다. 짧은 제목과 마지막 줄.'],['arabic','English العربية نص عربي words العربية نص عربي.']];
    for(var i=0;i<data.length;i++){
      var f=doc.pages[0].textFrames.add({geometricBounds:[36+i*250,36,276+i*250,i===0?180:216]});
      f.textFramePreferences.properties={firstBaselineOffset:FirstBaseline.FIXED_HEIGHT,minimumFirstBaselineOffset:20};
      f.insertLabel('oracle.id',data[i][0]);f.contents=data[i][1];f.parentStory.texts[0].applyParagraphStyle(child,true);
      if(i==1)f.parentStory.texts[0].appliedFont=app.fonts.itemByName('Malgun Gothic\tRegular');
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["latin","korean","arabic"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
