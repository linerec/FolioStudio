(function () {
  var CONFIG = CONFIG_PLACEHOLDER, doc = null, errors = [];
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:595,pageHeight:842,pagesPerDocument:1};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular',pointSize:12,leading:16,hyphenation:false};
    var f=doc.pages[0].textFrames.add({geometricBounds:[36,36,800,559]});
    f.insertLabel('oracle.id','flow');f.textFramePreferences.properties={textColumnCount:2,textColumnGutter:20};
    var text='One bright morning, the editor arranged words on a printed page. ';
    for(var i=0;i<75;i++)f.parentStory.insertionPoints[-1].contents=text;
    var r=doc.pages[0].rectangles.add({geometricBounds:[150,100,250,175],fillColor:doc.swatches.itemByName('Black'),strokeWeight:0});
    r.insertLabel('oracle.id','box');
    var o=doc.pages[0].ovals.add({geometricBounds:[330,100,430,195],fillColor:doc.swatches.itemByName('Black'),strokeWeight:0});
    o.insertLabel('oracle.id','oval');
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {errors.push(e.message+' @'+e.line);}
  finally {try{if(doc)doc.close(SaveOptions.NO);}catch(e2){errors.push(e2.message);}
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
  return '{"ok":'+(errors.length===0)+',"made":["flow","box","oval"],"errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
