// Vertical text fixture 2 (vtext-c2): the frame features a vertical story meets — a shaped (oval) frame, another item's
// text wrap, the baseline grid, underline / strikethrough, auto tate-chu-yoko and CJK punctuation at line edges. Malgun
// Gothic, Japanese composer, text hidden (fill None) so a step can show one frame at a time. This English InDesign has no
// DOM for the story orientation, so artifacts/vtext-c2-20261023/tools/make-fixture.py builds this, writes
// StoryOrientation="Vertical" into the IDML and lets InDesign re-export it (ids stable). (Not named build-*.jsx: `capture.py
// build` would write it horizontal.)
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
    doc.gridPreferences.properties = { baselineStart: 0, baselineDivision: 24, baselineGridShown: false,
      baselineGridRelativeOption: BaselineGridRelativeOption.TOP_OF_PAGE_OF_BASELINE_GRID_RELATIVE_OPTION };
    doc.textPreferences.typographersQuotes = false;
    function add(id, bounds, text, shape) {
      var f = doc.pages[0].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id);
      f.textFramePreferences.properties = { insetSpacing: 0, firstBaselineOffset: FirstBaseline.ASCENT_OFFSET, textColumnCount: 1 };
      if (shape) f.convertShape(shape);
      f.contents = text;
      f.parentStory.texts[0].properties = { appliedFont: font, fontStyle: 'Regular', pointSize: 12, leading: 20,
        appliedLanguage: 'English: USA', composer: '$ID/HL Composer J', hyphenation: false, tracking: 0,
        justification: Justification.LEFT_ALIGN, kerningMethod: 'Metrics', ligatures: false,
        fillColor: doc.swatches.itemByName('None') };
      made.push(id);
      return f;
    }
    var round = '세로 글이 둥근 틀 안에 들어갑니다. ';
    add('vt-oval', [40, 330, 290, 560], round + round + round + round + round + round + round + round + round + round, ConvertShapeOptions.CONVERT_TO_OVAL);
    var wrap = '글 감싸기 시험입니다. 세로 글이 사각형을 피해 갑니다. ';
    add('vt-wrap', [40, 40, 290, 270], wrap + wrap + wrap + wrap + wrap + wrap + wrap);
    var box = doc.pages[0].rectangles.add({ geometricBounds: [130, 110, 200, 200] });
    box.insertLabel('oracle.id', 'vt-wrap-box');
    box.properties = { fillColor: doc.swatches.itemByName('None'), strokeColor: doc.swatches.itemByName('Black'), strokeWeight: 0.5 };
    // (bounding box wrap, offsets 0: the fixture as built has TextWrapOffset 0 on every side)
    box.textWrapPreferences.properties = { textWrapMode: TextWrapModes.BOUNDING_BOX_TEXT_WRAP };
    made.push('vt-wrap-box');
    add('vt-grid', [310, 330, 560, 560], '기준선 격자 첫째 단락\r둘째 단락입니다\r셋째 단락\r넷째 단락입니다');
    add('vt-under', [310, 40, 560, 270], '밑줄 시험 English 글\r취소선 시험 漢字 글\r둘 다 있는 줄입니다');
    add('vt-autotcy', [580, 330, 820, 560], '2026년 11월 21일\r제 3장 25절 100쪽\rAB 12 CDE 345');
    add('vt-punct', [580, 40, 820, 270], '「세로쓰기」에서는 문장 부호가 바뀝니다、쉼표와 마침표。그리고 『겹낫표』와 （괄호）도 세로로 섭니다！ 정말일까요？ 「그렇습니다」。');
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
