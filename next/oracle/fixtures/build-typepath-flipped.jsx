(function () {
  var config = CONFIG_PLACEHOLDER;
  var prefs = app.scriptPreferences;
  var saved = [prefs.userInteractionLevel, prefs.measurementUnit, prefs.enableRedraw];
  var doc;
  try {
    prefs.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    prefs.measurementUnit = MeasurementUnits.POINTS;
    prefs.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {facingPages: false, pageWidth: 440, pageHeight: 620};
    doc.viewPreferences.rulerOrigin = RulerOrigin.PAGE_ORIGIN;
    var page = doc.pages[0];
    var rectangle = page.rectangles.add({geometricBounds: [85, 45, 230, 190]});
    var oval = page.ovals.add({geometricBounds: [90, 245, 235, 390]});
    function polygon(points) {
      var shape = page.polygons.add();
      shape.paths[0].entirePath = points;
      shape.paths[0].pathType = PathType.CLOSED_PATH;
      return shape;
    }
    var clockwise = polygon([[45, 430], [145, 385], [190, 485], [115, 530]]);
    var counterclockwise = polygon([[245, 430], [315, 530], [390, 485], [345, 385]]);
    function addText(shape, label, text) {
      shape.strokeWeight = 0;
      shape.insertLabel('oracle.id', label);
      var path = shape.textPaths.add();
      path.contents = text;
      path.parentStory.texts[0].properties = {appliedFont: 'Arial', fontStyle: 'Regular', pointSize: 18};
      path.flipPathEffect = FlipValues.NOT_FLIPPED;
    }
    addText(rectangle, 'rectangle', 'Rectangle text');
    addText(oval, 'oval', 'Oval text');
    addText(clockwise, 'clockwise', 'Clockwise polygon');
    addText(counterclockwise, 'counterclockwise', 'Counterclockwise polygon');
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
    return '{"ok":true,"made":["rectangle","oval","clockwise","counterclockwise"]}';
  } catch (error) {
    return '{"ok":false,"error":"' + String(error.message).replace(/["\\\r\n]/g, ' ') + ' @' + error.line + '"}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    prefs.userInteractionLevel = saved[0];
    prefs.measurementUnit = saved[1];
    prefs.enableRedraw = saved[2];
  }
})();
