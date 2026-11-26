(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  if(app.documents.length)return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:595,pageHeight:842,pagesPerDocument:1};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular'};
    var base=doc.paragraphStyles.add({name:'Parcomp Balance',appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular',pointSize:12,leading:18,hyphenation:false,composer:'$ID/HL Composer',justification:Justification.LEFT_ALIGN});
    var widths=[180,96,120,144],title='A short title with several uneven words';
    for(var i=0;i<widths.length;i++){
      var y=36+i*180,f=doc.pages[0].textFrames.add({geometricBounds:[y,36,y+160,36+widths[i]]});
      f.textFramePreferences.properties={firstBaselineOffset:FirstBaseline.FIXED_HEIGHT,minimumFirstBaselineOffset:20};
      f.insertLabel('oracle.id','balance'+i);f.contents=title+'\rThe quick brown fox jumps over the lazy dog. Publishing requires careful spacing between words.';
      f.parentStory.texts[0].applyParagraphStyle(base,true);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["balance0","balance1","balance2","balance3"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
