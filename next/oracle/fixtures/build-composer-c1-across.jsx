(function () {
  // composer-c1: 24 multi-column frames (2 and 3 columns, six heights, ragged and justified) of the style "Hyphen Body" for
  // HyphenateAcrossColumns. Same text, widths and heights as artifacts/composer-c1-20261023 (across-sweep).
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc=null, errors=[];
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:1800,pageHeight:1000,pagesPerDocument:1};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular'};
    var body=doc.paragraphStyles.add({name:'Hyphen Body',appliedFont:font,fontStyle:'Regular',pointSize:12,leading:16,appliedLanguage:'English: USA',hyphenation:true,hyphenationZone:36,hyphenWeight:5,hyphenateAcrossColumns:true,composer:'Adobe Paragraph Composer'});
    var T1='Publication international communication typography information. Publication international communication typography information. Extraordinary organizational paragraph internationalization.';
    var heights=[48,60,76,92,108,124], aligns=[Justification.LEFT_ALIGN,Justification.LEFT_JUSTIFIED];
    var n=0, made=[];
    for(var block=0;block<2;block++){
      var cols=block==0?2:3, width=block==0?110:90, fw=cols*width+12*(cols-1), x0=block==0?36:800, step=block==0?245:310;
      for(var h=0;h<heights.length;h++)for(var a=0;a<2;a++){
        n++;var i=h*2+a;
        var x=x0+(i%3)*step, y=36+Math.floor(i/3)*150;
        var f=doc.pages[0].textFrames.add({geometricBounds:[y,x,y+heights[h],x+fw]});
        f.textFramePreferences.properties={textColumnCount:cols,textColumnGutter:12};
        f.insertLabel('oracle.id','f'+n);f.contents=T1;made.push('"f'+n+'"');
        var t=f.parentStory.texts[0];
        t.applyParagraphStyle(body,true);t.justification=aligns[a];
      }
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(String(e.message)+' @'+e.line);}
  finally{
    try{if(doc)doc.close(SaveOptions.NO);}catch(e2){errors.push(e2.message);}
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
  return '{"ok":'+(errors.length===0)+',"made":['+made.join(',')+'],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
