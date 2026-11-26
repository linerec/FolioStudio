// Numbered headings and siblings with children on separate source pages.
(function () {
  var config = CONFIG_PLACEHOLDER;
  var prefs = app.scriptPreferences;
  var old = {ui:prefs.userInteractionLevel, unit:prefs.measurementUnit, redraw:prefs.enableRedraw};
  var doc = null, error = '';
  try {
    prefs.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    prefs.measurementUnit = MeasurementUnits.POINTS;
    prefs.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {facingPages:false, pageWidth:360, pageHeight:480, pagesPerDocument:3};
    var font = app.fonts.itemByName('Arial	Regular');
    doc.textDefaults.properties = {appliedFont:font, pointSize:12, leading:16};
    var base = doc.paragraphStyles[1];
    base.properties = {appliedFont:font, pointSize:12, leading:16, hyphenation:false};
    for (var i = 1; i <= 3; i++) {
      doc.paragraphStyles.add({name:'H'+i, basedOn:base, pointSize:18-i});
      doc.paragraphStyles.add({name:'TOC'+i, basedOn:base, leftIndent:12*(i-1)});
    }
    doc.paragraphStyles.add({name:'TOC Title', basedOn:base, pointSize:20});
    var headings = [
      ['Zebra chapter', 'Zulu section', 'Zulu detail', 'Alpha section', 'Alpha detail'],
      ['Beta chapter', 'Delta section', 'Delta detail', 'Bravo section', 'Bravo detail']
    ];
    for (var p = 0; p < headings.length; p++) {
      var frame = doc.pages[p+1].textFrames.add({geometricBounds:[36,36,420,324]});
      frame.insertLabel('oracle.id', 'head-'+(p+1));
      frame.contents = headings[p].join('\r');
      var levels = [1,2,3,2,3];
      for (var h = 0; h < levels.length; h++) {
        frame.paragraphs[h].applyParagraphStyle(doc.paragraphStyles.itemByName('H'+levels[h]), true);
      }
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
  } catch (e) {
    error = String(e.message) + ' @' + e.line;
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    prefs.userInteractionLevel = old.ui;
    prefs.measurementUnit = old.unit;
    prefs.enableRedraw = old.redraw;
  }
  return '{"ok":' + (!error) + ',"made":["head-1","head-2"],"error":"' + error.replace(/["\\\r\n]/g,' ') + '"}';
})();
