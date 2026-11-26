// C3 text oracle. System fonts only; hidden document, preferences restored.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [], errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var font = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: font, fontStyle: 'Regular' };
    var body = doc.paragraphStyles.add({ name: 'Text Body', appliedFont: font, fontStyle: 'Regular', pointSize: 12, leading: 16, hyphenation: false });
    var second = doc.paragraphStyles.add({ name: 'Text Second', basedOn: body, leftIndent: 18, spaceBefore: 7 });
    var korean = doc.paragraphStyles.add({ name: 'Text Korean', basedOn: body, appliedFont: app.fonts.itemByName('Malgun Gothic\tRegular') });
    var emphasis = doc.characterStyles.add({ name: 'Text Emphasis', fontStyle: 'Bold' });
    function frame(id, bounds, text, style) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id); made.push(id);
      f.contents = text;
      f.parentStory.paragraphs.everyItem().appliedParagraphStyle = style || body;
      return f;
    }
    // Exact indices: ABCD[0..4), efgh[4..8), IJKL[8..12), CR[12], second[13..19).
    var mixed = frame('mixed', [36, 36, 180, 559], 'ABCDefghIJKL\rsecond paragraph\rthird', body);
    mixed.parentStory.characters.itemByRange(4, 7).appliedCharacterStyle = emphasis;
    mixed.parentStory.characters.itemByRange(8, 11).pointSize = 15;
    mixed.parentStory.paragraphs[1].appliedParagraphStyle = second;
    mixed.parentStory.paragraphs[2].appliedParagraphStyle = body;
    // Distinguish the return's character format from the next paragraph's first letter.
    mixed.parentStory.characters[13].pointSize = 17;
    frame('korean', [190, 36, 260, 559], '한글 조판입니다.\r둘째 문단입니다.', korean);
    var flow = frame('flow-1', [280, 36, 312, 280], 'One two three four five six seven eight nine ten. One two three four five six seven eight nine ten.', body);
    var flow2 = frame('flow-2', [350, 36, 382, 280], '', body);
    flow.nextTextFrame = flow2;
    frame('overset', [420, 36, 452, 280], 'One two three four five six seven eight nine ten. One two three four five six seven eight nine ten. Hidden text at the end.', body);
    // Native characters/insertionPoints are measured, including astral text, table and anchor.
    var special = frame('special', [480, 36, 700, 559], 'A\uD83D\uDE00B\tC\nD\rE', body);
    special.parentStory.characters[1].appliedFont = app.fonts.itemByName('Segoe UI Symbol\tRegular');
    var table = special.parentStory.insertionPoints[-1].tables.add({ bodyRowCount: 1, columnCount: 1 });
    table.cells[0].contents = 'cell';
    table.cells[0].texts[0].appliedFont = font;
    table.width = 100;
    special.parentStory.insertionPoints[-1].contents = 'F';
    var anchor = special.parentStory.insertionPoints[-1].rectangles.add({ geometricBounds: [0, 0, 8, 8] });
    anchor.fillColor = doc.swatches.itemByName('Black');
    special.parentStory.insertionPoints[-1].contents = 'G';
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) {
    errors.push(String(e.message) + ' @' + e.line);
  } finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  function q(s) { return '"' + String(s).split('\\').join('/').split('"').join("'") + '"'; }
  var a = [], b = [];
  for (var i = 0; i < made.length; i++) a.push(q(made[i]));
  for (var j = 0; j < errors.length; j++) b.push(q(errors[j]));
  return '{"ok":' + (errors.length == 0) + ',"made":[' + a.join(',') + '],"errors":[' + b.join(',') + ']}';
})();
