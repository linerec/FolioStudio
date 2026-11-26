(function () {
  // composer-c1: six frames in the paragraph style "Hyphen Body" for the hyphen weight slider (HyphenWeight 0-10).
  // Texts, widths, sizes and alignments are cases of artifacts/composer-c1-20261023 (weight sweeps) whose line starts
  // Folio reproduces at every slider value.
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc=null, errors=[];
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:1000,pageHeight:1000,pagesPerDocument:1};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular'};
    var body=doc.paragraphStyles.add({name:'Hyphen Body',appliedFont:font,fontStyle:'Regular',pointSize:12,leading:16,appliedLanguage:'English: USA',hyphenation:true,hyphenationZone:36,hyphenWeight:5,composer:'Adobe Paragraph Composer'});
    var T1='Publication international communication typography information. Publication international communication typography information. Extraordinary organizational paragraph internationalization.';
    var T2='The administration established representative communications throughout the organization, recommending comprehensive documentation, professional development, and interdisciplinary collaboration for international publications. Unquestionably, understanding characteristics of typographical arrangements requires considerable experimentation, determination, and extraordinary perseverance.';
    var T4='Notwithstanding administrative complications, professionals demonstrated extraordinary determination; fundamentally, characteristic imagination, appreciation, and cooperation strengthened relationships throughout multinational publishing organizations worldwide.';
    var T5='Developmental psychologists acknowledged that representation, interpretation, and communication of mathematical information necessitate systematic investigation. Unfortunately, misunderstandings regarding typographic conventions occasionally undermine professional publications.';
    var L=Justification.LEFT_ALIGN, J=Justification.LEFT_JUSTIFIED;
    var cases=[[T1,150,10,L],[T1,200,12,L],[T1,170,12,J],[T2,150,12,J],[T4,180,12,L],[T5,140,12,L]];
    var made=[];
    for(var i=0;i<cases.length;i++){
      var c=cases[i], x=36+(i%3)*260, y=36+Math.floor(i/3)*440;
      var f=doc.pages[0].textFrames.add({geometricBounds:[y,x,y+400,x+c[1]]});
      f.insertLabel('oracle.id','f'+(i+1));f.contents=c[0];made.push('f'+(i+1));
      var t=f.parentStory.texts[0];
      t.applyParagraphStyle(body,true);
      t.justification=c[3];
      if(c[2]!==12){t.pointSize=c[2];t.leading=Math.round(c[2]*4/3);}
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e.message)+' @'+e.line);}
  finally{
    try{if(doc)doc.close(SaveOptions.NO);}catch(e2){errors.push(e2.message);}
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
  return '{"ok":'+(errors.length===0)+',"made":["f1","f2","f3","f4","f5","f6"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
