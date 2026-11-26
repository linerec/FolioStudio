// linkgoto.idml plus two hyperlinks whose source is a page item (not text): `Object to text` (the placed graphic's frame → the text
// destination "Offset seven") and `Object to page` (a small rectangle → the page destination). Opens the fixture, adds them, exports
// next/oracle/fixtures/linkgoto-objects.idml. Run: python -I artifacts/linkgoto-goto-20261028/build-objects.py (under the InDesign lock).
(function () {
 var CONFIG=CONFIG_PLACEHOLDER,doc=null,sp=app.scriptPreferences,step='';
 var old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
 try {
  sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
  doc=app.open(File(CONFIG.fixture),false);
  var graphic=null,rects=doc.rectangles;
  for(var i=0;i<rects.length;i++){if(rects[i].extractLabel('oracle.id')=='graphic')graphic=rects[i];}
  var textDest=doc.hyperlinkTextDestinations.itemByName('Offset seven'),pageDest=doc.hyperlinkPageDestinations[0];
  step='source 1';var s1=doc.hyperlinkPageItemSources.add(graphic,{name:'Object source'});
  step='link 1';doc.hyperlinks.add(s1,textDest,{name:'Object to text'});
  step='source 2';var small=doc.pages[0].rectangles.add({geometricBounds:[200,200,230,230]});small.insertLabel('oracle.id','obj2');
  var s2=doc.hyperlinkPageItemSources.add(small,{name:'Object source 2'});
  step='link 2';doc.hyperlinks.add(s2,pageDest,{name:'Object to page'});step='';
  doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  return '{"ok":true}';
 }catch(e){return '{"ok":false,"step":"'+step+'","error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
 finally{try{if(doc)doc.close(SaveOptions.NO);}catch(e2){}sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
})();
