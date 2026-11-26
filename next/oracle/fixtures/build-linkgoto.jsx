(function () {
 var CONFIG=CONFIG_PLACEHOLDER,doc=null,sp=app.scriptPreferences;
 var old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
 try {
  sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
  doc=app.documents.add(false);
  doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480,pagesPerDocument:2};
  doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),pointSize:12,leading:16,hyphenation:false};
  var a=doc.pages[0].textFrames.add({geometricBounds:[36,36,100,324]}),b=doc.pages[1].textFrames.add({geometricBounds:[36,36,100,324]});
  a.insertLabel('oracle.id','source');b.insertLabel('oracle.id','target');
  a.contents='SOURCE URL PAGE BAD';b.contents='Before target text';
  var dest=doc.hyperlinkTextDestinations.add(b.insertionPoints[7],{name:'Offset seven'});
  doc.hyperlinks.add(doc.hyperlinkTextSources.add(a.characters.itemByRange(0,5)),dest,{name:'Internal'});
  doc.hyperlinks.add(doc.hyperlinkTextSources.add(a.characters.itemByRange(7,9)),doc.hyperlinkURLDestinations.add('https://example.invalid/linkgoto'),{name:'External URL'});
  doc.hyperlinks.add(doc.hyperlinkTextSources.add(a.characters.itemByRange(11,14)),doc.hyperlinkPageDestinations.add(doc.pages[1]),{name:'Page'});
  var broken=doc.hyperlinkTextDestinations.add(b.insertionPoints[-1],{name:'Removed'});
  doc.hyperlinks.add(doc.hyperlinkTextSources.add(a.characters.itemByRange(16,18)),broken,{name:'Broken'});broken.remove();
  var r=doc.pages[0].rectangles.add({geometricBounds:[150,70,180,100]}),s=doc.pages[0].rectangles.add({geometricBounds:[190,70,220,100]});
  r.insertLabel('oracle.id','nested-child');r.fillColor=doc.swatches.itemByName('Black');
  r.rotationAngle=15;r.horizontalScale=125;r.verticalScale=80;r.shearAngle=10;
  var inner=doc.groups.add([r,s]);inner.rotationAngle=30;inner.horizontalScale=120;inner.shearAngle=5;
  var t=doc.pages[0].rectangles.add({geometricBounds:[240,70,270,100]});
  var outer=doc.groups.add([inner,t]);outer.rotationAngle=-20;outer.verticalScale=75;outer.shearAngle=-7;
  var graphic=doc.pages[0].rectangles.add({geometricBounds:[320,36,380,96]});graphic.insertLabel('oracle.id','graphic');graphic.place(File(CONFIG.image));
  doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  return '{"ok":true,"made":["source","target","nested-child","graphic"]}';
 }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
 finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
})();
