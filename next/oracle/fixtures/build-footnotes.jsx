// Three single pages, one threaded story, no footnotes. System fonts only.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = {ui:sp.userInteractionLevel, unit:sp.measurementUnit, redraw:sp.enableRedraw};
  var doc = null;
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {facingPages:false, pageWidth:360, pageHeight:480, pagesPerDocument:3};
    doc.viewPreferences.properties = {horizontalMeasurementUnits:MeasurementUnits.POINTS,
      verticalMeasurementUnits:MeasurementUnits.POINTS, rulerOrigin:RulerOrigin.SPREAD_ORIGIN};
    doc.marginPreferences.properties = {top:36,bottom:36,left:36,right:36};
    var font = app.fonts.itemByName('Arial\tRegular');
    doc.textDefaults.properties = {appliedFont:font,pointSize:12,leading:16};
    doc.paragraphStyles[1].properties = {appliedFont:font,pointSize:12,leading:16,hyphenation:false};
    var body = doc.paragraphStyles.add({name:'Body',appliedFont:font,pointSize:12,leading:16,hyphenation:false});
    var previous = null, first = null;
    for (var i=0;i<3;i++) {
      var page = doc.pages[i], b = page.bounds;
      var frame = page.textFrames.add({geometricBounds:[b[0]+36,b[1]+36,b[0]+444,b[1]+324]});
      frame.insertLabel('oracle.id','body-'+i);
      if (previous) previous.nextTextFrame = frame; else first = frame;
      previous = frame;
    }
    first.parentStory.contents = 'First paragraph for a footnote.\rSecond paragraph for another note.\rThird paragraph ends here.';
    first.parentStory.texts[0].applyParagraphStyle(body,true);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["body-0","body-1","body-2"]}';
  } catch(e) {
    return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';
  } finally {
    if(doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel=saved.ui; sp.measurementUnit=saved.unit; sp.enableRedraw=saved.redraw;
  }
})();
