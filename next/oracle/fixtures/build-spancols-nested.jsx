(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp=app.scriptPreferences, old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, doc=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:300,pagesPerDocument:2};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,pointSize:12,leading:14,hyphenation:false};
    for(var k=0;k<2;k++){
      var f=doc.pages[k].textFrames.add({geometricBounds:[36,36,236,336]});
      f.insertLabel('oracle.id',k===0?'table':'note');
      f.textFramePreferences.properties={textColumnCount:2,textColumnGutter:12};
      f.contents='lead1\nlead2\nlead3\rbody1\nbody2\nbody3\nbody4\rafter';
      var p=f.parentStory.paragraphs[1];
      if(k===0){
        var t=p.insertionPoints[0].tables.add({bodyRowCount:2,columnCount:2});
        for(var ci=0;ci<t.cells.length;ci++){
          t.cells[ci].contents='cell'+ci;
          t.cells[ci].texts[0].properties={appliedFont:font,pointSize:10,leading:12};
        }
      }else{
        var note=p.insertionPoints[2].footnotes.add();
        note.texts[0].contents='note first line\nnote second line';
        note.texts[0].properties={appliedFont:font,pointSize:10,leading:12};
      }
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["table","note"]}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/"/g,"'")+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
})();
