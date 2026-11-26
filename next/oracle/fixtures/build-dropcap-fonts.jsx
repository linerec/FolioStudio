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
    doc.documentPreferences.properties = {facingPages: false, pageWidth: 450, pageHeight: 580};
    doc.viewPreferences.rulerOrigin = RulerOrigin.PAGE_ORIGIN;
    doc.characterStyles.add({name: 'Drop Serif', appliedFont: 'Times New Roman', fontStyle: 'Bold'});
    doc.characterStyles.add({name: 'Drop Italic', fontStyle: 'Italic'});
    function sample(page, label, font, top, text) {
      var style = doc.paragraphStyles.itemByName('Drop ' + font);
      if (!style.isValid) style = doc.paragraphStyles.add({
        name: 'Drop ' + font, appliedFont: font, fontStyle: 'Regular', pointSize: 13, leading: 18, hyphenation: false
      });
      var frame = page.textFrames.add({geometricBounds: [top, 60, top + 120, 385]});
      frame.insertLabel('oracle.id', label);
      frame.contents = text + ' is the craft of arranging letters so that language becomes legible and appealing to the reader. Good type carries meaning without drawing attention to itself.';
      frame.parentStory.paragraphs[0].applyParagraphStyle(style, true);
    }
    sample(doc.pages[0], 'georgia', 'Georgia', 52, 'Typography');
    sample(doc.pages[0], 'calibri', 'Calibri', 286, 'AV spacing');
    var second = doc.pages.add();
    sample(second, 'single-space', 'Arial', 52, 'T Spacing');
    sample(second, 'many-spaces', 'Arial', 211, 'AV   Spacing');
    sample(second, 'nonbreaking-space', 'Arial', 370, 'AV\u00a0Spacing');
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
    return '{"ok":true,"made":["georgia","calibri","single-space","many-spaces","nonbreaking-space"]}';
  } catch (error) {
    return '{"ok":false,"error":"' + String(error.message).replace(/["\\\r\n]/g, ' ') + ' @' + error.line + '"}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    prefs.userInteractionLevel = saved[0];
    prefs.measurementUnit = saved[1];
    prefs.enableRedraw = saved[2];
  }
})();
