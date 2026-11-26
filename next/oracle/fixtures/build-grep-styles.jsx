(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[];
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:600,pagesPerDocument:1};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular'};
    var red=doc.colors.add({name:'Grep Red',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[220,0,0]});
    var blue=doc.colors.add({name:'Grep Blue',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[0,0,220]});
    doc.characterStyles.add({name:'Red',fillColor:red});
    doc.characterStyles.add({name:'Blue',fillColor:blue});
    doc.characterStyles.add({name:'Bold',fontStyle:'Bold'});
    var contents=['Word word WORD 123 (inside) 45','abc ABC 123 abc','first\nsecond\rthird fourth','A\u00a0B C\u2009D 123 (x)'];
    for(var i=0;i<4;i++){
      var p=doc.paragraphStyles.add({name:'Grep '+i,appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular',pointSize:12,leading:18,hyphenation:false,composer:'$ID/HL Single',justification:Justification.LEFT_ALIGN});
      var f=doc.pages[0].textFrames.add({geometricBounds:[30+i*130,30,145+i*130,370]});
      f.insertLabel('oracle.id','grep-'+i);f.contents=contents[i];f.parentStory.texts[0].applyParagraphStyle(p,true);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["grep-0","grep-1","grep-2","grep-3"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
