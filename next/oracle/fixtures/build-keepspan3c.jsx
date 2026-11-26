// Keep options meeting a split or spanning paragraph in frames that break inside it (keepspan-c3, batch c).
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences, old = {ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var doc = null, error = "";
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);
    doc.documentPreferences.properties={facingPages:false,pageWidth:460,pageHeight:300,pagesPerDocument:6};
    doc.viewPreferences.properties={horizontalMeasurementUnits:MeasurementUnits.POINTS,verticalMeasurementUnits:MeasurementUnits.POINTS};
    var font=app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties={appliedFont:font,fontStyle:'Regular',pointSize:12,leading:14,hyphenation:false};
    function frame(page,id,bounds,cols){var f=doc.pages[page].textFrames.add({geometricBounds:bounds});f.insertLabel("oracle.id",id);f.textFramePreferences.properties={textColumnCount:cols,textColumnGutter:12};return f;}
    var f0=frame(0,"p1",[36,36,116,426],3);
    var g0=frame(0,"p1-next",[150,36,270,426],3);
    f0.contents="a1\na2\rm1\nm2\nm3\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8";
    f0.nextTextFrame=g0;
    var f1=frame(1,"p2",[36,36,116,426],3);
    var g1=frame(1,"p2-next",[150,36,270,426],3);
    f1.contents="a1\na2\na3\rm1\nm2\nm3\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8";
    f1.nextTextFrame=g1;
    var f2=frame(2,"p3",[36,36,100,426],2);
    var g2=frame(2,"p3-next",[150,36,270,426],2);
    f2.contents="a1\na2\na3\na4\rm1\nm2\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8";
    f2.nextTextFrame=g2;
    var f3=frame(3,"p4",[36,36,116,426],2);
    var g3=frame(3,"p4-next",[150,36,270,426],2);
    f3.contents="a1\na2\na3\na4\na5\rm1\nm2\nm3\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8";
    f3.nextTextFrame=g3;
    var f4=frame(4,"p5",[36,36,116,426],3);
    var g4=frame(4,"p5-next",[150,36,270,426],3);
    f4.contents="a1\rm1\nm2\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8";
    f4.nextTextFrame=g4;
    var f5=frame(5,"p6",[36,36,84,426],3);
    var g5=frame(5,"p6-next",[150,36,270,426],3);
    f5.contents="a1\na2\na3\rm1\nm2\rb1\nb2\nb3\nb4\nb5\nb6\nb7\nb8";
    f5.nextTextFrame=g5;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  } catch(e) {error=String(e.message)+" @"+e.line;}
  finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=old.ui;sp.measurementUnit=old.unit;sp.enableRedraw=old.redraw;
  }
  return '{"ok":'+(!error)+',"made":["p1", "p1-next", "p2", "p2-next", "p3", "p3-next", "p4", "p4-next", "p5", "p5-next", "p6", "p6-next"],"error":"'+error.replace(/"/g,"'")+'"}';
})();
