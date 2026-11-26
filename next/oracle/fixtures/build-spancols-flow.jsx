(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp=app.scriptPreferences, old={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, doc=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:432,pageHeight:450,pagesPerDocument:2};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,pointSize:12,leading:14,hyphenation:false};
    function lines(prefix,n){var a=[];for(var i=1;i<=n;i++)a.push(prefix+i);return a.join('\n');}
    function frame(page,id,bounds){var f=page.textFrames.add({geometricBounds:bounds});f.insertLabel('oracle.id',id);f.textFramePreferences.properties={textColumnCount:3,textColumnGutter:12};return f;}
    var f=frame(doc.pages[0],'flow',[36,36,136,396]);
    var f2=frame(doc.pages[1],'flow-next',[36,36,136,396]);f.nextTextFrame=f2;
    f.contents=lines('lead',5)+'\r'+lines('b',12)+'\r'+lines('after',8);
    var consecutive=frame(doc.pages[0],'consecutive',[180,36,420,396]);
    consecutive.contents=lines('lead',5)+'\r'+lines('body',4)+'\r'+lines('after',4);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["flow","flow-next","consecutive"]}';
  } catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/"/g,"'")+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;}
})();
