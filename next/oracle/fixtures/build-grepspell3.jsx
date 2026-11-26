// Fixture grepspell3: paragraphs with an index marker (PageReference), an indent-to-here tab and an editorial note anchor, for GREP styles
// that must pass over those invisible characters (`c.d` does not match across them, `(?<=c)d` and `\w\w\w` do). Frames are labelled oracle.id gs-<n>.
(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[],made=[];
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:600,pagesPerDocument:1};
    var arial=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:arial,fontStyle:'Regular'};
    var red=doc.colors.add({name:'GS Red',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[220,0,0]});
    var blue=doc.colors.add({name:'GS Blue',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[0,0,220]});
    doc.characterStyles.add({name:'Red',fillColor:red});
    doc.characterStyles.add({name:'Blue',fillColor:blue});
    doc.characterStyles.add({name:'Bold',fontStyle:'Bold'});
    // parts: a string, or a marker name: INDEX (index marker), NOTE (editorial note anchor), INDENT_HERE_TAB (SpecialCharacters)
    var frames=[
      ['abc','INDEX','def ghi'],
      ['ab','INDENT_HERE_TAB','cd ef'],
      ['ab','NOTE','cd ef'],
      ['abc','INDEX','def ghi'],
      ['ab','INDENT_HERE_TAB','cd ef'],
      ['ab','NOTE','cd ef']
    ];
    var idx=doc.indexes.add(),tp=idx.topics.add('T');
    for(var i=0;i<frames.length;i++){
      var p=doc.paragraphStyles.add({name:'GS '+i,appliedFont:arial,fontStyle:'Regular',pointSize:18,leading:24,hyphenation:false,composer:'$ID/HL Single',justification:Justification.LEFT_ALIGN});
      var f=doc.pages[0].textFrames.add({geometricBounds:[20+Math.floor(i/2)*72,i%2?210:20,86+Math.floor(i/2)*72,i%2?385:195]});
      f.insertLabel('oracle.id','gs-'+i);
      var story=f.parentStory;story.contents='';
      for(var k=0;k<frames[i].length;k++){
        var part=frames[i][k],ip=story.insertionPoints[-1];
        if(part==='INDEX'){tp.pageReferences.add(story.insertionPoints[-1],PageReferenceType.CURRENT_PAGE);}
        else if(part==='NOTE'){ip.notes.add();}
        else if(/^[A-Z_]+$/.test(part)&&part.length>3){ip.contents=SpecialCharacters[part];}
        else{ip.contents=part;}
      }
      story.texts[0].applyParagraphStyle(p,true);
      made.push('gs-'+i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["'+made.join('","')+'"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
