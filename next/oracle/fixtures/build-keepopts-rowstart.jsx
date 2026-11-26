// A table that runs through threaded frames over four facing pages: two frames on page 1 (recto), one on pages 2 (verso), 3 (recto) and 4 (verso).
// Start Row of a body row decides where that row goes: the next frame (page 1's second frame), the next page (2), the next odd page (3) or
// the next even page (2; from page 2: 4).
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = '';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:true,pageWidth:300,pageHeight:300,pagesPerDocument:4};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular',pointSize:12,leading:14,hyphenation:false};
    var bounds=[[36,36,140,200],[160,36,264,200],[36,36,264,200],[36,36,264,200],[36,36,264,200]];
    var pages=[0,0,1,2,3], ids=['rs','rs-1b','rs-2','rs-3','rs-4'], frames=[];
    for(var i=0;i<bounds.length;i++){
      var f=doc.pages[pages[i]].textFrames.add({geometricBounds:bounds[i]});
      f.insertLabel('oracle.id',ids[i]);
      if(i>0)frames[i-1].nextTextFrame=f;
      frames.push(f);
    }
    frames[0].contents='lead\rlast';
    var p=frames[0].parentStory.paragraphs[1];
    var table=p.insertionPoints[0].tables.add({headerRowCount:0,footerRowCount:0,bodyRowCount:6,columnCount:2});
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
  return '{"ok":'+(!error)+',"made":["rs","rs-1b","rs-2","rs-3","rs-4"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
