// Existing index markers in a footnote and in a table in one threaded story.
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
    doc.documentPreferences.properties = {facingPages:false, pageWidth:612, pageHeight:540, pagesPerDocument:3};
    var font = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = {appliedFont:font, fontStyle:'Regular', pointSize:12, leading:Leading.AUTO};
    var frames = [];
    for (var i = 0; i < doc.pages.length; i++) {
      var frame = doc.pages[i].textFrames.add({geometricBounds:[36,36,330,340]});
      frame.insertLabel('oracle.id', 'source-' + (i + 1));
      if (i > 0) frame.previousTextFrame = frames[i - 1];
      frames.push(frame);
    }
    var story = frames[0].parentStory;
    story.contents = 'First page of the story.\rFootnote on this page.\rTable on this page.';
    story.paragraphs[1].startParagraph = StartParagraph.NEXT_PAGE;
    story.paragraphs[2].startParagraph = StartParagraph.NEXT_PAGE;
    var note = story.paragraphs[1].insertionPoints[8].footnotes.add();
    note.texts[0].contents = 'Note target';
    var table = story.paragraphs[2].insertionPoints[-2].tables.add({bodyRowCount:2, columnCount:2});
    for (var c = 0; c < table.cells.length; c++) table.cells[c].contents = 'Cell ' + (c + 1);
    var cell = table.cells[3];
    var index = doc.indexes.length ? doc.indexes[0] : doc.indexes.add();
    index.topics.add('Footnote target').pageReferences.add(note.texts[0].insertionPoints[1], PageReferenceType.CURRENT_PAGE);
    index.topics.add('Table target').pageReferences.add(cell.texts[0].insertionPoints[1], PageReferenceType.CURRENT_PAGE);
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
  } catch (e) {
    error = String(e.message) + ' @' + e.line;
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    prefs.userInteractionLevel = old.ui;
    prefs.measurementUnit = old.unit;
    prefs.enableRedraw = old.redraw;
  }
  return '{"ok":' + (!error) + ',"made":["source-1","source-2","source-3"],"error":"' + error.replace(/"/g, "'") + '"}';
})();
