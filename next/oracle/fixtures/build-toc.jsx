// Four pages, three heading levels, parent page numbers and two sections.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, doc=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS; sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480,pagesPerDocument:4};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.textDefaults.properties={appliedFont:font,pointSize:12,leading:16};
    doc.paragraphStyles[1].properties={appliedFont:font,pointSize:12,leading:16,hyphenation:false};
    var body=doc.paragraphStyles.add({name:'Body',basedOn:doc.paragraphStyles[1]});
    for(var i=1;i<=3;i++) {
      doc.paragraphStyles.add({name:'H'+i,basedOn:body,pointSize:18-i*2});
      var entry=doc.paragraphStyles.add({name:'TOC'+i,basedOn:body,leftIndent:(i-1)*12});
      entry.tabStops.add({position:260,alignment:TabStopAlignment.RIGHT_ALIGN,leader:'.'});
    }
    doc.paragraphStyles.add({name:'TOC Title',basedOn:body,pointSize:20});
    doc.characterStyles.add({name:'Page Number',fontStyle:'Bold'});
    var master=doc.masterSpreads[0].pages[0];
    var footer=master.textFrames.add({geometricBounds:[450,36,475,324]});
    footer.contents=SpecialCharacters.AUTO_PAGE_NUMBER;
    doc.sections[0].pageNumberStyle=PageNumberStyle.LOWER_ROMAN;
    doc.sections.add(doc.pages[2],{continueNumbering:false,pageNumberStart:10,pageNumberStyle:PageNumberStyle.ARABIC});
    var made=[];
    for(var p=1;p<4;p++) {
      var frame=doc.pages[p].textFrames.add({geometricBounds:[36,36,420,324]});
      frame.insertLabel('oracle.id','head-'+p); made.push('head-'+p);
      frame.contents='Chapter '+p+'\rSection '+p+'\rDetail '+p+'\rBody text.';
      frame.parentStory.texts[0].applyParagraphStyle(body,true);
      for(var h=0;h<3;h++) frame.paragraphs[h].applyParagraphStyle(doc.paragraphStyles.itemByName('H'+(h+1)),true);
    }
    var hidden=doc.layers.add({name:'Hidden headings'});
    var hf=doc.pages[3].textFrames.add({geometricBounds:[425,36,445,324],itemLayer:hidden});
    hf.contents='Hidden heading'; hf.paragraphs[0].applyParagraphStyle(doc.paragraphStyles.itemByName('H1'),true); hidden.visible=false;
    doc.activeLayer=doc.layers[1];
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["head-1","head-2","head-3"]}';
  } catch(e) { return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}'; }
  finally { if(doc) doc.close(SaveOptions.NO); sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw; }
})();
