// Plain shapes for corner, noise, and QR measurements.
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
      facingPages: false, pageWidth: 360, pageHeight: 354, pagesPerDocument: 1
    };
    doc.viewPreferences.properties = {
      horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS
    };
    var page = doc.pages[0], blue = doc.colors.add({
      name: 'Oracle Blue', model: ColorModel.PROCESS, space: ColorSpace.CMYK,
      colorValue: [72, 38, 0, 12]
    });
    var made = [];
    function rectangle(label, bounds, fill, angle) {
      var item = page.rectangles.add({
        geometricBounds: bounds, fillColor: fill,
        strokeColor: doc.swatches.itemByName('Black'), strokeWeight: 1.5,
        rotationAngle: angle
      });
      item.insertLabel('oracle.id', label);
      made.push(label);
    }
    rectangle('r-plain', [32, 30, 100, 150], doc.swatches.itemByName('None'), 0);
    rectangle('r-fill', [128, 34, 214, 151], blue, 0);
    rectangle('r-rot', [246, 43, 307, 148], blue, 19);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
    return '{"ok":true,"made":["' + made.join('","') + '"]}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = saved[0];
    sp.measurementUnit = saved[1];
    sp.enableRedraw = saved[2];
  }
})();
