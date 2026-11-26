(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp=app.scriptPreferences, ui=sp.userInteractionLevel, unit=sp.measurementUnit, doc=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;
    doc=app.documents.add(false,{documentPreferences:{facingPages:false,pageWidth:300,pageHeight:400,pagesPerDocument:1,createPrimaryTextFrame:false}});
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.PAGE_ORIGIN};
    var master=doc.masterSpreads[0], page=master.pages[0];
    while(master.pages.length>1)master.pages[-1].remove();
    while(master.pageItems.length)master.pageItems[0].remove();
    var frame=page.rectangles.add({geometricBounds:[30,30,90,90],strokeColor:doc.swatches.itemByName('None')});
    frame.insertLabel('oracle.id','parent-image');frame.place(File(CONFIG.image));frame.fit(FitOptions.FILL_PROPORTIONALLY);
    var text=page.textFrames.add({geometricBounds:[120,30,200,270]});
    text.insertLabel('oracle.id','parent-text');text.contents='Parent text';text.parentStory.appliedFont='Arial';text.parentStory.fontStyle='Regular';text.parentStory.pointSize=14;text.parentStory.fillColor=doc.swatches.itemByName('Black');
    var guide=page.guides.add({orientation:HorizontalOrVertical.HORIZONTAL,location:230,fitToPage:true});
    guide.insertLabel('oracle.id','parent-guide');
    doc.pages[0].appliedMaster=master;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["parent-image","parent-text","parent-guide"]}';
  } catch(e) {return '{"ok":false,"error":"'+String(e).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
  finally {if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=ui;sp.measurementUnit=unit;}
})();
