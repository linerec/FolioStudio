// Vertical text fixture (vtext-c1): four stories in Malgun Gothic with the Japanese composer, their text
// hidden (fill None) so a step can show it. This English InDesign has no DOM for the story orientation, so
// artifacts/vtext-c1-20261023/tools/make-fixture.py builds this, writes StoryOrientation="Vertical" into the
// IDML and lets InDesign re-export it (ids stable). (Not named build-*.jsx: `capture.py build` would write it horizontal.)
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [], errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    if (!font.isValid) throw Error('Malgun Gothic Regular is not installed');
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 1 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS,
      rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    doc.textPreferences.typographersQuotes = false;
    function add(id, bounds, text, columns) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id);
      f.textFramePreferences.properties = { insetSpacing: 0, firstBaselineOffset: FirstBaseline.ASCENT_OFFSET,
        textColumnCount: columns || 1, textColumnGutter: 12 };
      f.contents = text;
      f.parentStory.texts[0].properties = { appliedFont: font, fontStyle: 'Regular', pointSize: 12, leading: 20,
        appliedLanguage: 'English: USA', composer: '$ID/HL Composer J', hyphenation: false, tracking: 0,
        justification: Justification.LEFT_ALIGN, kerningMethod: 'Metrics', ligatures: false,
        fillColor: doc.swatches.itemByName('None') };
      made.push(id);
    }
    add('vt-mixed', [60, 330, 330, 560], '세로쓰기 시험입니다\r漢字와 한글을 섞어\rEnglish 글자는 눕힘\r숫자 12와 2026년\r「인용」과 (괄호)도');
    add('vt-cols', [380, 330, 760, 560], '첫째 단의 첫 줄\r첫째 단의 둘째 줄\r셋째 줄입니다\r넷째 줄입니다\r다섯째 줄\r여섯째 줄\r일곱째 줄\r여덟째 줄\r아홉째 줄\r열째 줄\r열한째 줄\r열두째 줄\r열셋째 줄\r열넷째 줄', 2);
    add('vt-tcy', [60, 40, 330, 270], '2026년 11월 21일\r오후 12시 30분\r제 3장 25절');
    add('vt-rot', [380, 40, 760, 270], '글자 회전 시험\rABC 회전 글자\r가나다라마바사');
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  var a = [];
  for (var i = 0; i < made.length; i++) a.push('"' + made[i] + '"');
  return '{"ok":' + (errors.length == 0) + ',"made":[' + a.join(',') + '],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
