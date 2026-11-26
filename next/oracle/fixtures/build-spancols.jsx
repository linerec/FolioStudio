// C3 formatting: system fonts, inherited styles, direct overrides, plain-text clipboard.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 2 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular' };
    var body = doc.paragraphStyles.add({ name: 'Format Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 16, hyphenation: false });
    var heading = doc.paragraphStyles.add({ name: 'Format Heading', basedOn: body, pointSize: 18, leading: 24, spaceBefore: 8, leftIndent: 12 });
    var other = doc.paragraphStyles.add({ name: 'Format Other', basedOn: body, justification: Justification.RIGHT_ALIGN, spaceAfter: 6 });
    var bold = doc.characterStyles.add({ name: 'Format Bold', fontStyle: 'Bold' });
    doc.characterStyles.add({ name: 'Format Italic', fontStyle: 'Italic' });
    doc.characterStyles.add({ name: 'Format Large', basedOn: bold, pointSize: 16, tracking: 40 });
    doc.characterStyles.add({ name: 'Format Serif', appliedFont: app.fonts.itemByName('Times New Roman\tBold'), fontStyle: 'Bold' });
    function frame(id, bounds, text, style) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id); f.contents = text;
      f.parentStory.paragraphs.everyItem().applyParagraphStyle(style || body, true);
      return f;
    }
    var f=frame('keep', [36,36,136,276], 'lead1\nlead2\nlead3\nlead4\nlead5\rbody1\nbody2\nbody3\nbody4\rafter', body);
    f.textFramePreferences.properties={textColumnCount:3,textColumnGutter:12};
    var split=frame('split', [180,36,380,396], f.contents, body);
    split.textFramePreferences.properties={textColumnCount:3,textColumnGutter:12};
    body.leading=14;
    var kept=doc.paragraphStyles.add({name:'Kept',basedOn:body});
    f.parentStory.paragraphs[1].applyParagraphStyle(kept,true);
    split.parentStory.paragraphs[1].applyParagraphStyle(kept,true);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["format","destination","korean-format"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
