(function () {
  var cfg=CONFIG_PLACEHOLDER, doc=null, sp=app.scriptPreferences;
  if(app.documents.length)return '{"ok":false,"error":"documents open"}';
  var ui=sp.userInteractionLevel,unit=sp.measurementUnit;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:true,pageWidth:240,pageHeight:300,pagesPerDocument:5};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    doc.sections[0].properties={continueNumbering:false,pageNumberStart:10,sectionPrefix:'A-',includeSectionPrefix:true};
    var made=[];
    for(var i=0;i<doc.pages.length;i++)if(i!==2){
      var p=doc.pages[i],b=p.bounds;
      var r=p.rectangles.add({geometricBounds:[b[0]+20,b[1]+20,b[0]+50+i*15,b[1]+80],fillColor:doc.swatches.itemByName('Black'),strokeColor:doc.swatches.itemByName('None'),nonprinting:i===3});
      r.insertLabel('oracle.id','printing-box-'+i);made.push('printing-box-'+i);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(cfg.out));
    return '{"ok":true,"made":["box0","box1","box3","box4"]}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=ui;sp.measurementUnit=unit;}
})();
