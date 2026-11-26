// A system-font story with room for optical alignment at both edges.
(function () {
  var config = CONFIG_PLACEHOLDER;
  var sp = app.scriptPreferences;
  var saved = [sp.userInteractionLevel, sp.measurementUnit, sp.enableRedraw];
  var doc;
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {
      facingPages: false, pageWidth: 360, pageHeight: 260, pagesPerDocument: 1
    };
    doc.viewPreferences.properties = {
      horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS
    };
    var frame = doc.pages[0].textFrames.add({ geometricBounds: [36, 36, 224, 324] });
    frame.insertLabel('oracle.id', 'optmargin');
    frame.contents = 'Reference line';
    frame.parentStory.paragraphs[0].properties = {
      appliedFont: app.fonts.itemByName('Arial\tRegular'), fontStyle: 'Regular',
      pointSize: 12, leading: 18, hyphenation: false
    };
    frame.parentStory.storyPreferences.properties = {
      opticalMarginAlignment: false, opticalMarginSize: 12
    };
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
    return '{"ok":true,"made":["optmargin"]}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = saved[0];
    sp.measurementUnit = saved[1];
    sp.enableRedraw = saved[2];
  }
})();
