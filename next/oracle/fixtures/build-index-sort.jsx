// A document with an index and one text frame, for Index panel ▸ Sort Options (rest-c1): the IndexingSortOption values in designmap.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = '';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:400,pagesPerDocument:1};
    var f=doc.pages[0].textFrames.add({geometricBounds:[36,36,136,276]});
    f.insertLabel('oracle.id','format');
    f.contents='Alpha one.';
    f.parentStory.texts[0].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),pointSize:12};
    if(!doc.indexes.length)doc.indexes.add();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {error=String(e.message)+' @'+e.line;}
  finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!error)+',"made":["format"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
