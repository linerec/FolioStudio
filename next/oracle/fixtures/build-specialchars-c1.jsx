// Fixture specialchars-c1: paragraphs with markers (page number, anchored object, End Nested Style Character, text variable)
// for nested styles and GREP styles that end at or match them. Frames are labelled oracle.id sc-<n>.
(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[],made=[];
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:600,pagesPerDocument:1};
    var arial=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:arial,fontStyle:'Regular'};
    var red=doc.colors.add({name:'SC Red',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[220,0,0]});
    var blue=doc.colors.add({name:'SC Blue',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[0,0,220]});
    doc.characterStyles.add({name:'Red',fillColor:red});
    doc.characterStyles.add({name:'Blue',fillColor:blue});
    doc.characterStyles.add({name:'Bold',fontStyle:'Bold'});
    var tv=doc.textVariables.add({name:'CT',variableType:VariableTypes.CUSTOM_TEXT_TYPE});
    tv.variableOptions.contents='VAL';
    var black=doc.swatches.itemByName('Black'),none=doc.swatches.itemByName('None');
    // parts: a string, or a marker name (SpecialCharacters), INLINE (anchored square), VAR (text variable instance)
    var frames=[
      ['Page ','AUTO_PAGE_NUMBER',' of ab cd'],
      ['ab ','AUTO_PAGE_NUMBER',' cd ef'],
      ['ab ','INLINE',' cd ef'],
      ['ab cd','END_NESTED_STYLE','ef gh ij'],
      ['ab','END_NESTED_STYLE','cd ef gh'],
      ['Page ','AUTO_PAGE_NUMBER',' of ab cd'],
      ['ab ','AUTO_PAGE_NUMBER',' cd ef'],
      ['ab','END_NESTED_STYLE','cd ef'],
      ['ab ','INLINE',' cd ef'],
      ['ab ','VAR',' cd ef']
    ];
    for(var i=0;i<frames.length;i++){
      var p=doc.paragraphStyles.add({name:'SC '+i,appliedFont:arial,fontStyle:'Regular',pointSize:12,leading:18,hyphenation:false,composer:'$ID/HL Single',justification:Justification.LEFT_ALIGN});
      var f=doc.pages[0].textFrames.add({geometricBounds:[20+Math.floor(i/2)*72,i%2?210:20,86+Math.floor(i/2)*72,i%2?385:195]});
      f.insertLabel('oracle.id','sc-'+i);
      var story=f.parentStory;story.contents='';
      for(var k=0;k<frames[i].length;k++){
        var part=frames[i][k],ip=story.insertionPoints[-1];
        if(part==='INLINE'){ip.rectangles.add({geometricBounds:[0,0,8,8],fillColor:black,strokeColor:none});}
        else if(part==='VAR'){ip.textVariableInstances.add({associatedTextVariable:tv});}
        else if(/^[A-Z_]+$/.test(part)&&part.length>3){ip.contents=SpecialCharacters[part];}
        else{ip.contents=part;}
      }
      story.texts[0].applyParagraphStyle(p,true);
      made.push('sc-'+i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["'+made.join('","')+'"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
