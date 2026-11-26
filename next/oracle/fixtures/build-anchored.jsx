(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw}, doc=null, made=[];
  function label(it,name){it.insertLabel('oracle.id',name);made.push(name);return it;}
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:360,pageHeight:480};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS,rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    var tf=label(doc.pages[0].textFrames.add({geometricBounds:[36,36,430,324]}),'text');
    tf.contents='Alpha beta gamma delta.\rSecond paragraph for anchors.\rThird paragraph for anchors.';
    tf.parentStory.texts[0].properties={appliedFont:'Arial',fontStyle:'Regular',pointSize:12,leading:16};
    var r=label(doc.pages[0].rectangles.add({geometricBounds:[50,50,68,74],fillColor:doc.swatches.itemByName('Black'),strokeWeight:0}),'inline');
    r.anchoredObjectSettings.insertAnchoredObject(tf.parentStory.insertionPoints[6],AnchorPosition.INLINE_POSITION);
    var a=label(doc.pages[0].rectangles.add({geometricBounds:[80,50,92,80],fillColor:doc.swatches.itemByName('Black'),strokeWeight:0}),'above');
    a.anchoredObjectSettings.insertAnchoredObject(tf.parentStory.insertionPoints[30],AnchorPosition.ABOVE_LINE);
    var c=label(doc.pages[0].rectangles.add({geometricBounds:[100,50,112,68],fillColor:doc.swatches.itemByName('Black'),strokeWeight:0}),'custom');
    c.anchoredObjectSettings.insertAnchoredObject(tf.parentStory.insertionPoints[60],AnchorPosition.ANCHORED);
    c.anchoredObjectSettings.properties={anchorPoint:AnchorPoint.TOP_LEFT_ANCHOR,horizontalReferencePoint:AnchoredRelativeTo.TEXT_FRAME,horizontalAlignment:HorizontalAlignment.LEFT_ALIGN,verticalReferencePoint:VerticallyRelativeTo.LINE_BASELINE,anchorXoffset:-40,anchorYoffset:5};
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  }catch(e){return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';}
  finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
