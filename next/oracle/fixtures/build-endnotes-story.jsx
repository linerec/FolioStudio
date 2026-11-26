// Two stories on two pages (frames "a" and "b", not threaded), for the endnote scope Story and the endnote frame placed by hand (rest-c1).
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = '';
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:400,pageHeight:400,pagesPerDocument:2};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular',pointSize:12,leading:14,hyphenation:false};
    var ids=['a','b'],texts=['Alpha story text for notes.','Beta story text for notes.'];
    for(var i=0;i<2;i++){
      var f=doc.pages[i].textFrames.add({geometricBounds:[36,36,200,364]});
      f.insertLabel('oracle.id',ids[i]);
      f.contents=texts[i];
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {error=String(e.message)+' @'+e.line;}
  finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!error)+',"made":["a","b"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
