// Hidden, reproducible CJK composition specimens. Run: capture.py build cjk.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var old = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [], errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS;
    sp.enableRedraw = false;
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    if (!font.isValid) throw Error('Malgun Gothic Regular is not installed');
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 14 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    var texts = [
      ['syllables', '가나다라마바사아자차카타파하거너더러머버서어저처커터퍼허고노도로모보소오조초코토포호구누두루무부수우주추쿠투푸후'],
      ['words', '우리 잡지는 사람과 도시의 이야기를 전합니다. 오래된 골목을 따라 걸으며 새로운 풍경을 만났습니다. 다람쥐 헌 쳇바퀴에 타고파. 함께 읽고 생각하는 시간을 만듭니다.'],
      ['mixed', '한글ABC와 Studio2026의 새 문서를 12월 25일에 만듭니다. 가격은 12,345원이며 10.5% 할인됩니다. A4 용지 210mm와 297mm에 한국어 English 123을 나란히 놓습니다.'],
      ['punct', '그는 “안녕하세요.”라고 말했습니다. (서울의 오늘)과 [내일의 도시]를 읽고, 「작은 이야기」를 썼습니다. 질문은 무엇인가요? 답은 여기 있습니다! …그리고 ‘다음’으로 갑니다.'],
      ['han', '大韓民國出版文化産業振興計劃과 서울特別市의 文化藝術政策을 함께 살펴봅니다. 漢字文章句讀點、括弧「言語文化」와 한글을 섞어서 읽습니다. 東西南北春夏秋冬山川草木日月星辰'],
      ['latin', '한국어와 internationalization typography representation extraordinary publication을 나란히 적습니다. antidisestablishmentarianism supercalifragilisticexpialidocious 단어도 여러 줄에 놓습니다.']
    ];
    var widths = [96, 173, 241];
    var basic = doc.paragraphStyles.itemByName('$ID/NormalParagraphStyle');
    basic.properties = { appliedFont: font, pointSize: 11, leading: 15, spaceBefore: 0, spaceAfter: 0,
      leftIndent: 0, rightIndent: 0, firstLineIndent: 0, tracking: 0,
      minimumWordSpacing: 80, desiredWordSpacing: 100, maximumWordSpacing: 133,
      minimumLetterSpacing: 0, desiredLetterSpacing: 0, maximumLetterSpacing: 0,
      minimumGlyphScaling: 100, desiredGlyphScaling: 100, maximumGlyphScaling: 100 };
    function add(kind, text, width, justify, hyphen, composer) {
      var n = made.length, x = 30 + (n % 2) * 285, y = 30 + Math.floor((n % 6) / 2) * 265;
      var id = [composer == '$ID/HL Composer' ? 'roman' : 'japanese', kind, width,
        justify ? 'justify' : 'left', hyphen ? 'hyphen' : 'word'].join('-');
      var f = doc.pages[Math.floor(n / 6)].textFrames.add({ geometricBounds: [y, x, y + 240, x + width] });
      f.insertLabel('oracle.id', id);
      f.textFramePreferences.properties = { insetSpacing: 0, firstBaselineOffset: FirstBaseline.ASCENT_OFFSET };
      f.contents = text;
      f.parentStory.texts[0].properties = { appliedFont: font, fontStyle: 'Regular', pointSize: 11,
        appliedLanguage: 'English: USA', leading: 15, composer: composer, hyphenation: hyphen,
        justification: justify ? Justification.LEFT_JUSTIFIED : Justification.LEFT_ALIGN,
        hyphenateWordsLongerThan: 5, hyphenateAfterFirst: 2, hyphenateBeforeLast: 2,
        hyphenateCapitalizedWords: true, hyphenateLastWord: true, hyphenateLadderLimit: 3,
        kerningMethod: 'Metrics', ligatures: false };
      made.push(id);
    }
    for (var t = 0; t < texts.length; t++)
      for (var w = 0; w < widths.length; w++)
        for (var a = 0; a < 2; a++)
          for (var h = 0; h < 2; h++) add(texts[t][0], texts[t][1], widths[w], a, h, '$ID/HL Composer');
    for (var j = 0; j < texts.length; j++)
      for (var q = 0; q < 2; q++) add(texts[j][0], texts[j][1], 173, q, true, '$ID/HL Composer J');
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) {
    errors.push(String(e.message) + ' @' + e.line);
  } finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = old.ui; sp.measurementUnit = old.unit; sp.enableRedraw = old.redraw;
  }
  function quote(s) { return '"' + String(s).split('\\').join('/').split('"').join("'") + '"'; }
  var a = [], b = [];
  for (var i = 0; i < made.length; i++) a.push(quote(made[i]));
  for (var k = 0; k < errors.length; k++) b.push(quote(errors[k]));
  return '{"ok":' + (errors.length == 0) + ',"made":[' + a.join(',') + '],"errors":[' + b.join(',') + ']}';
})();
