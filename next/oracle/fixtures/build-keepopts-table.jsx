// Keep options around a table with repeating header and footer rows that runs on into a second frame.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = '';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:400,pagesPerDocument:1};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular',pointSize:12,leading:14,hyphenation:false};
    var f=doc.pages[0].textFrames.add({geometricBounds:[36,36,136,276]});
    f.insertLabel('oracle.id','tbl');
    f.contents='lead1\rlead2\rlead3\rafter1\nafter2\nafter3\rlast';
    var f2=doc.pages[0].textFrames.add({geometricBounds:[160,36,300,276]});
    f2.insertLabel('oracle.id','tbl-next');
    f.nextTextFrame=f2;
    var p=f.parentStory.paragraphs[3];
    var table=p.insertionPoints[0].tables.add({headerRowCount:1,footerRowCount:1,bodyRowCount:5,columnCount:2});
    for(var ci=0;ci<table.cells.length;ci++) {
      table.cells[ci].contents='c'+ci;
      table.cells[ci].texts[0].properties={appliedFont:font,fontStyle:'Regular',pointSize:10,leading:12};
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {error=String(e.message)+' @'+e.line;}
  finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!error)+',"made":["tbl","tbl-next"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
