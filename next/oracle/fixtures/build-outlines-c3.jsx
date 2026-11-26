// Create Outlines on a text range, second part (outlines-c3): stroked characters (the polygon is put by the outer edge of
// its stroke), and the pair kerning InDesign gives the object character (set as a word space) with the letters around it.
// Arial 36 pt frames without insets; hidden document, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function frame(name, b, text, font) {
    var f = doc.pages[0].textFrames.add({ geometricBounds: b });
    f.textFramePreferences.insetSpacing = [0, 0, 0, 0];
    f.contents = text;
    f.insertLabel('oracle.id', name); made.push(name);
    var t = f.texts[0];
    t.appliedFont = app.fonts.itemByName(font || 'Arial\tRegular'); t.pointSize = 36; t.kerningMethod = 'Metrics';
    return f;
  }
  function stroke(range, weight, fillName) {
    range.strokeColor = doc.swatches.itemByName('Black'); range.strokeWeight = weight;
    if (fillName) range.fillColor = doc.swatches.itemByName(fillName);
  }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.PAGE_ORIGIN };
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 612, pageHeight: 792, pagesPerDocument: 1 };
    doc.marginPreferences.properties = { top: 24, bottom: 24, left: 24, right: 24 };
    doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle').appliedFont = app.fonts.itemByName('Arial\tRegular');
    doc.textDefaults.appliedFont = app.fonts.itemByName('Arial\tRegular');
    var s1 = frame('stroke', [20, 20, 70, 300], 'AV Oag'); stroke(s1.texts[0], 2, 'Paper');
    var s2 = frame('strokemix', [80, 20, 130, 300], 'AV Oag'); stroke(s2.characters.itemByRange(0, 2), 3);
    var s3 = frame('stroke8', [140, 20, 200, 300], 'AV Oag'); stroke(s3.texts[0], 8, 'Paper');
    var s4 = frame('strokeblack', [210, 20, 260, 300], 'AV Oag'); stroke(s4.texts[0], 0.5);
    var s5 = frame('strokelead', [270, 20, 330, 300], 'Oag Bog'); s5.texts[0].leading = 20; stroke(s5.texts[0], 4, 'Paper');
    frame('kerntimes', [340, 20, 390, 300], 'TVWAx', 'Times New Roman\tRegular');
    frame('kernafter', [400, 20, 450, 300], 'xVAx');
    frame('kernbefore', [460, 20, 510, 300], 'ATLx');
    var l1 = frame('leadlive', [640, 20, 700, 300], 'Oag Bog'); l1.texts[0].leading = 20;
    var l2 = frame('strokelive', [710, 20, 770, 300], 'Oag Bog'); stroke(l2.texts[0], 4, 'Paper');
    var u1 = frame('under', [20, 320, 70, 600], 'AV Oag'); u1.texts[0].underline = true;
    var u2 = frame('strike', [80, 320, 130, 600], 'AV Oag'); u2.texts[0].strikeThru = true;
    var u3 = frame('rule', [140, 320, 200, 600], 'AV Oag'); u3.paragraphs[0].ruleBelow = true; u3.paragraphs[0].ruleBelowLineWeight = 2; u3.paragraphs[0].ruleBelowOffset = 6;
    frame('kernverdana', [520, 20, 570, 300], 'AVAx', 'Verdana\tRegular');
    frame('kerntwo', [580, 20, 630, 300], 'TVAYx');
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
    return '{"ok":true,"made":' + made.toSource().replace(/'/g, '"') + '}';
  } catch (e) {
    return '{"ok":false,"error":"' + String(e.message).replace(/["\\\r\n]/g, ' ') + ' @' + e.line + '"}';
  } finally {
    if (doc) doc.close(SaveOptions.NO);
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
})();
