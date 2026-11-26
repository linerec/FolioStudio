(function () {
  var CONFIG=CONFIG_PLACEHOLDER, doc=null, errors=[];
  if(app.documents.length) return '{"ok":false,"errors":"documents open"}';
  var sp=app.scriptPreferences, old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:420,pageHeight:400,pagesPerDocument:1};
    var rows=[
      ['arabic','Arial\tRegular','\u0628\u0628\u0628 \u0628 \u0628\u0628',true],
      ['zero','Cascadia Code\tRegular','0 000 0123456789',false],
      ['unsupported','Courier New\tRegular','0123456789 ABC abc',false]
    ];
    for(var i=0;i<rows.length;i++) {
      var row=rows[i],f=doc.pages[0].textFrames.add({geometricBounds:[36+i*105,36,130+i*105,384]});
      f.textFramePreferences.properties={firstBaselineOffset:FirstBaseline.FIXED_HEIGHT,minimumFirstBaselineOffset:24};
      f.insertLabel('oracle.id',row[0]);f.contents=row[2];
      f.parentStory.texts[0].properties={appliedFont:app.fonts.itemByName(row[1]),fontStyle:'Regular',pointSize:20,leading:28,hyphenation:false};
      if(row[3]) f.parentStory.paragraphs[0].properties={composer:'$ID/HL Composer Optyca',paragraphDirection:ParagraphDirectionOptions.RIGHT_TO_LEFT_DIRECTION,justification:Justification.RIGHT_ALIGN};
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {errors.push(String(e)+' @'+e.line);}
  finally {
    if(doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!errors.length)+',"made":["arabic","zero","unsupported"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
