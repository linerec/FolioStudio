(function () {
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[],made=[];
  var sp=app.scriptPreferences,old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:600,pagesPerDocument:1};
    var arial=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:arial,fontStyle:'Regular'};
    var red=doc.colors.add({name:'Nest Red',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[220,0,0]});
    var blue=doc.colors.add({name:'Nest Blue',model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:[0,0,220]});
    doc.characterStyles.add({name:'Red',fillColor:red});
    doc.characterStyles.add({name:'Blue',fillColor:blue});
    doc.characterStyles.add({name:'Bold',fontStyle:'Bold'});
    var texts=[
      'The quick brown fox jumps.',
      '  Lead word  gap, end.',
      'One two. Mr. Smith? Yes!! 3.5 ok.',
      'a1b2c3d4 abc 123 x',
      'Name\tvalue here\tend line',
      'ab\ncd ef gh ij kl',
      'Key: value; other: x',
      'w1 w2 w3 w4 w5 w6 w7 w8',
      'Alpha beta\rGamma delta\rEpsilon',
      '\uc548\ub155 \ud558\uc138\uc694 abc \ud55c\uae00 def',
      'Short',
      'The quick brown fox jumps.',
      'The quick brown fox jumps.',
      'The quick brown fox jumps.'
    ];
    function props(name,base){
      var o={name:name,appliedFont:name==='NS 9'?app.fonts.itemByName('Malgun Gothic	Regular'):arial,fontStyle:'Regular',pointSize:12,leading:18,hyphenation:false,composer:'$ID/HL Single',justification:Justification.LEFT_ALIGN};
      if(base)o.basedOn=base;
      return o;
    }
    var parent=doc.paragraphStyles.add(props('NS P'));
    for(var i=0;i<texts.length;i++){
      var p=doc.paragraphStyles.add(props('NS '+i,i===11?parent:null));
      var f=doc.pages[0].textFrames.add({geometricBounds:[20+Math.floor(i/2)*72,i%2?210:20,86+Math.floor(i/2)*72,i%2?385:195]});
      f.insertLabel('oracle.id','ns-'+i);f.contents=texts[i];f.parentStory.texts[0].applyParagraphStyle(p,true);
      if(i===12)f.parentStory.characters.itemByRange(4,8).appliedCharacterStyle=doc.characterStyles.itemByName('Blue');
      if(i===13)p.nestedGrepStyles.add({appliedCharacterStyle:doc.characterStyles.itemByName('Blue'),grepExpression:'quick'});
      made.push('ns-'+i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e)+' @'+e.line);}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
  return '{"ok":'+(!errors.length)+',"made":["'+made.join('","')+'"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
