(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, saved = {ui:sp.userInteractionLevel, unit:sp.measurementUnit, redraw:sp.enableRedraw};
  var doc = null, made = [];
  function label(it, name) { it.insertLabel('oracle.id', name); made.push(name); return it; }
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit=MeasurementUnits.POINTS; sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:true,pageWidth:300,pageHeight:400,pagesPerDocument:3};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    var layer=doc.layers[0].getElements()[0], hidden=doc.layers.add({name:'Hidden guides',showGuides:false});
    var a=doc.masterSpreads[0], b=doc.masterSpreads.add({namePrefix:'B',baseName:'Child'}), c=doc.masterSpreads.add({namePrefix:'C',baseName:'Grandchild'});
    var parents=[a,b,c];
    for(var m=0;m<3;m++) while(parents[m].pageItems.length) parents[m].pageItems[0].remove();
    for(var m=0;m<3;m++) for(var p=0;p<2;p++) {
      var pg=parents[m].pages[p], r=pg.bounds;
      label(pg.rectangles.add(layer,{geometricBounds:[r[0]+30+m*45,r[1]+35+p*20,r[0]+55+m*45,r[1]+105+p*20],fillColor:doc.swatches.itemByName('Black'),fillTint:30+m*30,strokeColor:doc.swatches.itemByName('None')}),'parent-'+m+'-'+p);
      label(pg.guides.add(layer,{orientation:HorizontalOrVertical.HORIZONTAL,location:75+m*50,fitToPage:true}),'guide-'+m+'-'+p).itemLayer=layer;
      label(pg.guides.add(hidden,{orientation:HorizontalOrVertical.VERTICAL,location:r[1]+85,fitToPage:true}),'hidden-guide-'+m+'-'+p).itemLayer=hidden;
    }
    b.appliedMaster=a; c.appliedMaster=b;
    for(var i=0;i<doc.pages.length;i++) doc.pages[i].appliedMaster=c;
    label(doc.pages[0].guides.add(layer,{orientation:HorizontalOrVertical.HORIZONTAL,location:275,fitToPage:true}),'document-guide').itemLayer=layer;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  } catch(e) { return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}'; }
  finally { if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw; }
})();
