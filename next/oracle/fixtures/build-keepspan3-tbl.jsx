// Keep options around a table with header and footer rows that runs on into a second frame, at several frame heights (keepspan-c3).
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = '';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:400,pagesPerDocument:5};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular',pointSize:12,leading:14,hyphenation:false};
    var heights=[90,104,118,132,160], made=[];
    for (var i=0;i<heights.length;i++) {
      var f=doc.pages[i].textFrames.add({geometricBounds:[36,36,36+heights[i],276]});
      f.insertLabel('oracle.id','t'+(i+1));
      f.contents='lead1\rlead2\rlead3\rafter1\nafter2\nafter3\rlast1\rlast2';
      var f2=doc.pages[i].textFrames.add({geometricBounds:[200,36,380,276]});
      f2.insertLabel('oracle.id','t'+(i+1)+'-next');
      f.nextTextFrame=f2;
      var p=f.parentStory.paragraphs[3];
      var table=p.insertionPoints[0].tables.add({headerRowCount:1,footerRowCount:1,bodyRowCount:5,columnCount:2});
      for(var ci=0;ci<table.cells.length;ci++) {
        table.cells[ci].contents='c'+ci;
        table.cells[ci].texts[0].properties={appliedFont:font,fontStyle:'Regular',pointSize:10,leading:12};
      }
      made.push('t'+(i+1),'t'+(i+1)+'-next');
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {error=String(e.message)+' @'+e.line;}
  finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!error)+',"made":["t1","t1-next","t2","t2-next","t3","t3-next","t4","t4-next","t5","t5-next"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
