(function () {
  var config = CONFIG_PLACEHOLDER;
  var prefs = app.scriptPreferences;
  var saved = [prefs.userInteractionLevel, prefs.measurementUnit, prefs.enableRedraw];
  var doc, labels = [];
  try {
    prefs.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    prefs.measurementUnit = MeasurementUnits.POINTS;
    prefs.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {
      facingPages: true, pageWidth: 360, pageHeight: 480, pagesPerDocument: 3
    };
    doc.viewPreferences.rulerOrigin = RulerOrigin.PAGE_ORIGIN;
    var names = ['column', 'margins', 'page'];
    for (var p = 1; p <= 2; p++) {
      var page = doc.pages[p], side = p === 1 ? 'left' : 'right';
      page.marginPreferences.properties = {top: 49, bottom: 67, left: 56, right: 42};
      var frame = page.textFrames.add({geometricBounds: [105, 91, 382, 287]});
      frame.textFramePreferences.properties = {textColumnCount: 2, textColumnGutter: 16, insetSpacing: 6};
      frame.contents = 'One two three.';
      frame.parentStory.texts[0].properties = {appliedFont: 'Arial', fontStyle: 'Regular', pointSize: 12, leading: 16};
      for (var i = 0; i < names.length; i++) {
        var rect = page.rectangles.add({
          geometricBounds: [70, 60, 78 + i * 2, 76], fillColor: doc.swatches.itemByName('Black'), strokeWeight: 0
        });
        var label = side + '-' + names[i];
        rect.insertLabel('oracle.id', label);
        labels.push(label);
        rect.anchoredObjectSettings.insertAnchoredObject(frame.parentStory.insertionPoints[i * 4], AnchorPosition.ANCHORED);
        rect.anchoredObjectSettings.properties = {
          anchorPoint: AnchorPoint.TOP_LEFT_ANCHOR, horizontalReferencePoint: AnchoredRelativeTo.TEXT_FRAME,
          horizontalAlignment: HorizontalAlignment.LEFT_ALIGN, verticalReferencePoint: VerticallyRelativeTo.LINE_BASELINE,
          anchorXoffset: -8 - i * 22, anchorYoffset: 14
        };
      }
    }
    doc.recompose();
    doc.viewPreferences.rulerOrigin = RulerOrigin.SPREAD_ORIGIN;
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
    return '{"ok":true,"made":' + labels.toSource().replace(/'/g, '"') + '}';
  } catch (error) {
    return '{"ok":false,"error":"' + String(error.message).replace(/["\\\r\n]/g, ' ') + ' @' + error.line + '"}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    prefs.userInteractionLevel = saved[0];
    prefs.measurementUnit = saved[1];
    prefs.enableRedraw = saved[2];
  }
})();
