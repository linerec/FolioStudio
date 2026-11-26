// Keep options around tables and footnotes, using Windows system fonts.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = '';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:300,pagesPerDocument:2};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular',pointSize:12,leading:14,hyphenation:false};
    for(var k=0;k<2;k++) {
      var f=doc.pages[k].textFrames.add({geometricBounds:[36,36,136,276]});
      f.insertLabel('oracle.id',k===0?'table':'note');
      f.textFramePreferences.properties={textColumnCount:2,textColumnGutter:12};
      f.contents='lead1\nlead2\nlead3\nlead4\nlead5\rbody1\nbody2\nbody3\nbody4\rafter';
      var p=f.parentStory.paragraphs[1];
      if(k===0) {
        var table=p.insertionPoints[0].tables.add({bodyRowCount:2,columnCount:2});
        for(var ci=0;ci<table.cells.length;ci++) {
          table.cells[ci].contents='cell'+ci;
          table.cells[ci].texts[0].properties={appliedFont:font,fontStyle:'Regular',pointSize:10,leading:12};
        }
      } else {
        var note=p.insertionPoints[2].footnotes.add();
        note.texts[0].contents='note first line\nnote second line';
        note.texts[0].properties={appliedFont:font,fontStyle:'Regular',pointSize:10,leading:12};
      }
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {error=String(e.message)+' @'+e.line;}
  finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!error)+',"made":["table","note"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
