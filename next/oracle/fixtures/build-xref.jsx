(function () {
 var CONFIG=CONFIG_PLACEHOLDER,doc=null,sp=app.scriptPreferences;
 var old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
 try {
  sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
  doc=app.documents.add(false);
  doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480,pagesPerDocument:2};
  doc.textDefaults.properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),pointSize:12,leading:16};
  doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),pointSize:12,leading:16,hyphenation:false};
  var a=doc.pages[0].textFrames.add({geometricBounds:[36,36,400,324]}),b=doc.pages[1].textFrames.add({geometricBounds:[36,36,400,324]});
  a.insertLabel('oracle.id','source');b.insertLabel('oracle.id','target');
  a.contents='Website link\rReference: ';b.contents='Target heading\rSecond paragraph';
  var source=doc.hyperlinkTextSources.add(a.characters.itemByRange(0,6));
  var url=doc.hyperlinkURLDestinations.add('https://example.org/',{name:'Example'});
  doc.hyperlinks.add(source,url,{name:'Website'});
  var format=doc.crossReferenceFormats.add({name:'Folio paragraph'});format.buildingBlocks.add(BuildingBlockTypes.PARAGRAPH_TEXT_BUILDING_BLOCK);
  var anchor=doc.hyperlinkTextDestinations.add(b.insertionPoints[0],{name:'Heading'});
  var xref=doc.crossReferenceSources.add(a.insertionPoints[-1],format);
  doc.hyperlinks.add(xref,anchor,{name:'Reference'});xref.update();
  doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  return '{"ok":true,"made":["source","target"]}';
 }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
 finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
})();
