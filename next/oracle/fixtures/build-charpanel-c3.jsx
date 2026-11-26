// charpanel-c3: Character panel scale / skew / position / language / kerning and the Paragraph panel's last-line indent and Hyphenate box.
// System fonts only (Arial, Times New Roman, Malgun Gothic). Narrow frames so the text wraps and line heights show.
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
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular' };
    var body = doc.paragraphStyles.add({ name: 'Cp Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 16, hyphenation: false });
    var auto = doc.paragraphStyles.add({ name: 'Cp Auto', basedOn: body, leading: Leading.AUTO });
    var hy = doc.paragraphStyles.add({ name: 'Cp Hyphen', basedOn: body, hyphenation: true, justification: Justification.LEFT_JUSTIFIED });
    function frame(id, bounds, text, style) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id); f.contents = text;
      f.parentStory.paragraphs.everyItem().applyParagraphStyle(style || body, true);
      return f;
    }
    var f = frame('scale', [36, 36, 300, 250], 'Hamburgefonstiv AV To office. The quick brown fox jumps over the lazy dog.\rSecond paragraph with auto leading, 0123456789 wide narrow text keeps going on.\rThird line.', body);
    f.parentStory.paragraphs[1].applyParagraphStyle(auto, true);
    var g = frame('serif', [320, 36, 500, 250], 'Times serif text: AV To Wa office fi fl. The quick brown fox jumps over the lazy dog again and again.\rSecond serif paragraph.', body);
    g.parentStory.texts[0].appliedFont = app.fonts.itemByName('Times New Roman\tRegular');
    frame('korean', [520, 36, 640, 250], '한글 문단의 서식입니다. 가로 비율과 기울기를 시험합니다.\r둘째 문단입니다.', body)
      .parentStory.texts[0].appliedFont = app.fonts.itemByName('Malgun Gothic\tRegular');
    frame('hyph', [36, 280, 336, 380], 'Internationalization characteristically misunderstanding extraordinarily communication interpretation.\rAnother paragraph with antidisestablishmentarianism and representatives.', hy);
    frame('plain', [36, 400, 300, 559], 'Plain destination frame.\rSecond paragraph.', body);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["scale","serif","korean","hyph","plain"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
