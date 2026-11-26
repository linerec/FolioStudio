(function () {
  // bookx-c4: a page with two text frames and the objects a library item is made of (Library.store, then placeAsset on text).
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, doc=null, made=[];
  function label(it,name){it.insertLabel('oracle.id',name);made.push(name);return it;}
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    var page=doc.pages[0], black=doc.swatches.itemByName('Black'), reg=doc.swatches.itemByName('Registration');
    var tf=label(page.textFrames.add({geometricBounds:[36,36,150,324]}),'text');
    tf.contents='hello world';
    tf.parentStory.texts[0].properties={appliedFont:'Arial',fontStyle:'Regular',pointSize:12,leading:16};
    var t2=label(page.textFrames.add({geometricBounds:[160,36,260,324]}),'second');
    t2.contents='abcdef\rsecond paragraph';
    t2.parentStory.texts[0].properties={appliedFont:'Arial',fontStyle:'Regular',pointSize:12,leading:16};
    label(page.rectangles.add({geometricBounds:[300,36,330,76],fillColor:black,strokeColor:reg,strokeWeight:3}),'rect');
    label(page.ovals.add({geometricBounds:[300,90,330,120],fillColor:black,strokeWeight:0}),'oval');
    var rot=label(page.rectangles.add({geometricBounds:[300,140,330,190],fillColor:black,strokeColor:reg,strokeWeight:2}),'rot');
    rot.rotationAngle=30;
    var g1=page.rectangles.add({geometricBounds:[400,36,420,56],fillColor:black,strokeWeight:0});
    var g2=page.ovals.add({geometricBounds:[400,70,420,90],fillColor:black,strokeWeight:0});
    label(page.groups.add([g1,g2]),'group');
    label(page.textFrames.add({geometricBounds:[340,140,380,220],contents:'abc'}),'tframe');
    var pic=label(page.rectangles.add({geometricBounds:[340,36,390,86]}),'pic');
    pic.place(File(CONFIG.image));
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
