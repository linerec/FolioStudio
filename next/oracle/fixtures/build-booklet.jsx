(function () {
  // Booklet printing fixture: 8 facing pages of 240 x 300 pt. Each page has a black box (its height tells the page),
  // a coloured box and Arial text; some objects reach past the page edge or across the spine (what the booklet clips).
  var cfg=CONFIG_PLACEHOLDER, doc=null, sp=app.scriptPreferences;
  if(app.documents.length)return '{"ok":false,"error":"documents open"}';
  var ui=sp.userInteractionLevel,unit=sp.measurementUnit;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:true,pageWidth:240,pageHeight:300,pagesPerDocument:8};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    var black=doc.swatches.itemByName('Black'),none=doc.swatches.itemByName('None');
    var inks=[];
    var tints=[[100,0,0,0],[0,100,0,0],[0,0,100,0],[60,0,0,40],[0,60,60,0],[40,40,0,0],[0,0,60,30],[80,20,0,0]];
    for(var c=0;c<tints.length;c++)inks.push(doc.colors.add({name:'booklet-'+c,model:ColorModel.PROCESS,space:ColorSpace.CMYK,colorValue:tints[c]}));
    var made=[];
    for(var i=0;i<doc.pages.length;i++){
      var p=doc.pages[i],b=p.bounds;
      var r=p.rectangles.add({geometricBounds:[b[0]+20,b[1]+20,b[0]+50+i*15,b[1]+80],fillColor:black,strokeColor:none});
      r.insertLabel('oracle.id','booklet-box-'+i);made.push('booklet-box-'+i);
      var q=p.rectangles.add({geometricBounds:[b[0]+120,b[1]+100,b[0]+170,b[1]+200],fillColor:inks[i],strokeColor:none});
      q.insertLabel('oracle.id','booklet-ink-'+i);
      var t=p.textFrames.add({geometricBounds:[b[0]+220,b[1]+20,b[0]+280,b[1]+220],contents:'Page '+(i+1)+'\rBooklet'});
      t.texts[0].appliedFont=app.fonts.itemByName('Arial\tRegular');t.texts[0].pointSize=24;
      t.insertLabel('oracle.id','booklet-text-'+i);
    }
    // objects that leave their page: over the outside edge of the page (pages 8 and 1: the pasteboard), across the spine (spread of pages 4 and 5)
    var p8=doc.pages[7],b8=p8.bounds;
    p8.rectangles.add({geometricBounds:[b8[0]+60,b8[1]-30,b8[0]+90,b8[1]+50],fillColor:inks[2],strokeColor:none}).insertLabel('oracle.id','booklet-over-8');
    var p1=doc.pages[0],b1=p1.bounds;
    p1.rectangles.add({geometricBounds:[b1[0]+60,b1[1]-30,b1[0]+90,b1[1]+50],fillColor:inks[1],strokeColor:none}).insertLabel('oracle.id','booklet-over-1');
    var p4=doc.pages[3],b4=p4.bounds;
    p4.rectangles.add({geometricBounds:[b4[0]+200,b4[3]-30,b4[0]+230,b4[3]+30],fillColor:inks[0],strokeColor:black,strokeWeight:1}).insertLabel('oracle.id','booklet-spine-4');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(cfg.out));
    return '{"ok":true,"made":["'+made.join('","')+'"]}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=ui;sp.measurementUnit=unit;}
})();
