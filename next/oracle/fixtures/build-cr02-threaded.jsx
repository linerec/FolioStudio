// Independent text stories with open, curved and closed paths between frames.
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
      facingPages: false, pageWidth: 620, pageHeight: 480, pagesPerDocument: 2
    };
    doc.viewPreferences.properties = {
      horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS,
      rulerOrigin: RulerOrigin.PAGE_ORIGIN
    };
    var chains = [
      ['line', 'curve', 'closed', 'line'],
      ['frame', 'closed', 'line', 'curve', 'frame'],
      ['curve', 'frame', 'line', 'frame', 'closed'],
      ['frame', 'line', 'frame', 'closed', 'curve'],
      ['closed', 'curve', 'frame']
    ];
    var made = [];
    for (var row = 0; row < chains.length; row++) {
      var nodes = [];
      for (var col = 0; col < chains[row].length; col++) {
        var kind = chains[row][col];
        var x = 24 + 116 * col, y = 32 + 136 * (row % 3);
        var page = doc.pages[row < 3 || (row === 3 && col < 2) ? 0 : 1];
        var p = page.bounds, l = p[1] + x, t = p[0] + y;
        var label = 'link-' + (row + 1) + '-' + (col + 1);
        var item;
        if (kind === 'frame') {
          item = page.textFrames.add({geometricBounds: [t, l, t + 44, l + 102]});
          nodes.push(item);
        } else {
          if (kind === 'closed') {
            item = page.ovals.add({geometricBounds: [t, l, t + 38, l + 100]});
          } else {
            item = page.graphicLines.add();
            item.paths[0].entirePath = kind === 'line'
              ? [[l, t + 26], [l + 102, t + 26]]
              : [[l, t + 27], [[l + 32, t - 8], [l + 49, t + 4], [l + 70, t + 15]], [l + 102, t + 24]];
          }
          item.strokeColor = doc.swatches.itemByName('Black');
          item.strokeWeight = 0.5;
          nodes.push(item.textPaths.add());
        }
        item.insertLabel('oracle.id', label);
        made.push(label);
      }
      for (var i = 0; i + 1 < nodes.length; i++) nodes[i].nextTextFrame = nodes[i + 1];
      var story = nodes[0].parentStory;
      var text = 'Story ' + (row + 1) + '. ';
      var repeats = [9, 4, 7, 14, 1][row];
      for (var r = 0; r < repeats; r++) text += 'One two three four five six. ';
      story.contents = text;
      story.texts[0].appliedFont = app.fonts.itemByName('Arial');
      story.texts[0].fontStyle = 'Regular';
      story.texts[0].pointSize = 10;
    }
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
    return '{"ok":true,"made":["' + made.join('","') + '"]}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = saved[0];
    sp.measurementUnit = saved[1];
    sp.enableRedraw = saved[2];
  }
})();
