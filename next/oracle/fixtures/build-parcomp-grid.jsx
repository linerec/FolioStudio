(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  if(app.documents.length)return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:595,pageHeight:842,pagesPerDocument:1};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular'};
    doc.gridPreferences.properties={baselineStart:0,baselineDivision:24,baselineGridShown:false};
    var base=doc.paragraphStyles.add({name:'Parcomp Grid',appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular',pointSize:12,leading:18,hyphenation:false,composer:'$ID/HL Single',justification:Justification.LEFT_ALIGN});
    var f=doc.pages[0].textFrames.add({geometricBounds:[36,36,500,216]});
    f.textFramePreferences.properties={firstBaselineOffset:FirstBaseline.FIXED_HEIGHT,minimumFirstBaselineOffset:20};
    f.insertLabel('oracle.id','grid');f.contents='A short title with several uneven words.\rThe quick brown fox jumps over the lazy dog. Publishing requires careful spacing between words and consistent line breaks across a narrow column.';
    f.parentStory.texts[0].applyParagraphStyle(base,true);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["grid"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
