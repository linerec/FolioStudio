(function () {
 // Cross-reference formats that need the composed document: page numbers (a section on page 3), paragraph numbers, full paragraphs.
 var CONFIG=CONFIG_PLACEHOLDER,doc=null,sp=app.scriptPreferences;
 var old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
 try {
  sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
  doc=app.documents.add(false);
  doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480,pagesPerDocument:3};
  doc.textDefaults.properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),pointSize:12,leading:16};
  var base={appliedFont:app.fonts.itemByName('Arial\tRegular'),pointSize:12,leading:16,hyphenation:false};
  doc.paragraphStyles[1].properties=base;
  var num=doc.paragraphStyles.add({name:'Num'});num.properties=base;num.bulletsAndNumberingListType=ListType.NUMBERED_LIST;
  var bul=doc.paragraphStyles.add({name:'Bul'});bul.properties=base;bul.bulletsAndNumberingListType=ListType.BULLET_LIST;
  doc.sections.add(doc.pages[2],{pageNumberStart:10,sectionPrefix:'S',includeSectionPrefix:true,continueNumbering:false});
  var a=doc.pages[0].textFrames.add({geometricBounds:[36,36,200,324]});
  var b=doc.pages[1].textFrames.add({geometricBounds:[36,36,400,324]});
  var c=doc.pages[2].textFrames.add({geometricBounds:[36,36,400,324]});
  a.insertLabel('oracle.id','source');b.insertLabel('oracle.id','target');c.insertLabel('oracle.id','third');
  a.contents='Reference: ';
  b.contents='Target heading\rNumbered one\rNumbered two: with delimiter\rBullet item\rPlain tail';
  c.contents='Third page numbered\rThird plain';
  b.paragraphs[1].appliedParagraphStyle=num;b.paragraphs[2].appliedParagraphStyle=num;b.paragraphs[3].appliedParagraphStyle=bul;
  c.paragraphs[0].appliedParagraphStyle=num;
  var t=BuildingBlockTypes;
  var f1=doc.crossReferenceFormats.add({name:'Folio delimiter'});f1.buildingBlocks.add(t.PARAGRAPH_TEXT_BUILDING_BLOCK,{appliedDelimiter:':',includeDelimiter:false});
  var f2=doc.crossReferenceFormats.add({name:'Folio delimiter included'});f2.buildingBlocks.add(t.PARAGRAPH_TEXT_BUILDING_BLOCK,{appliedDelimiter:':',includeDelimiter:true});
  doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  return '{"ok":true,"made":["source","target","third"]}';
 }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
 finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
})();
