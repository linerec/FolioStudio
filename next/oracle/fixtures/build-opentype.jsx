(function () {
  var CONFIG=CONFIG_PLACEHOLDER, doc=null, errors=[], made=[];
  if(app.documents.length) return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences, old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:595,pageHeight:842,pagesPerDocument:1};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular'};
    var fonts=['Arial','Calibri','Cambria','Gabriola','Segoe UI','Courier New'];
    for(var i=0;i<fonts.length;i++) {
      var id=fonts[i].toLowerCase().replace(/ /g,'-');
      var style=doc.paragraphStyles.add({name:'OT '+fonts[i],appliedFont:app.fonts.itemByName(fonts[i]+'\tRegular'),fontStyle:'Regular',pointSize:18,leading:26,hyphenation:false});
      var f=doc.pages[0].textFrames.add({geometricBounds:[36+i*105,36,130+i*105,559]});
      f.textFramePreferences.properties={firstBaselineOffset:FirstBaseline.FIXED_HEIGHT,minimumFirstBaselineOffset:22};
      f.insertLabel('oracle.id',id); made.push(id);
      f.contents='ffi fi st 0123456789 1/2 3/4 1st 2nd ABC abc';
      f.parentStory.texts[0].applyParagraphStyle(style,true);
    }
    doc.characterStyles.add({name:'OT test'});
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {errors.push(String(e)+' @'+e.line);}
  finally {
    if(doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!errors.length)+',"made":["'+made.join('\",\"')+'"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
