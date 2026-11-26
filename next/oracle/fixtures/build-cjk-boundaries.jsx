// Hidden, controlled Korean line-break specimens. Run capture.py build cjk-boundaries.
(function () {
  var config = CONFIG_PLACEHOLDER;
  var prefs = app.scriptPreferences;
  var old = {ui: prefs.userInteractionLevel, unit: prefs.measurementUnit, redraw: prefs.enableRedraw};
  var doc = null, made = [], error;
  try {
    prefs.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    prefs.measurementUnit = MeasurementUnits.POINTS;
    prefs.enableRedraw = false;
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    if (!font.isValid) throw Error('Malgun Gothic Regular is not installed');
    doc = app.documents.add(false);
    doc.documentPreferences.properties = {facingPages: false, pageWidth: 500, pageHeight: 600};
    doc.viewPreferences.properties = {horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN};

    function add(name, text, width, options) {
      var slot = made.length % 8;
      var page = slot === 0 && made.length > 0 ? doc.pages.add() : doc.pages.lastItem();
      var x = 20 + (slot % 2) * 240, y = 20 + Math.floor(slot / 2) * 145;
      var frame = page.textFrames.add({geometricBounds: [y, x, y + 136, x + width]});
      frame.insertLabel('oracle.id', name);
      frame.textFramePreferences.properties = {insetSpacing: 0, firstBaselineOffset: FirstBaseline.ASCENT_OFFSET};
      frame.contents = text;
      frame.parentStory.texts[0].properties = {
        appliedFont: font, fontStyle: 'Regular', appliedLanguage: 'English: USA', pointSize: 11,
        leading: 15, composer: options.japanese ? '$ID/HL Composer J' : '$ID/HL Composer',
        justification: options.justify ? Justification.LEFT_JUSTIFIED : Justification.LEFT_ALIGN,
        hyphenation: Boolean(options.hyphen), hyphenateWordsLongerThan: 5,
        hyphenateAfterFirst: 2, hyphenateBeforeLast: 2,
        hyphenateCapitalizedWords: true, hyphenateLastWord: true, hyphenateLadderLimit: 3,
        kerningMethod: 'Metrics', ligatures: false, tracking: 0,
        minimumWordSpacing: 80, desiredWordSpacing: 100, maximumWordSpacing: 133,
        minimumLetterSpacing: 0, desiredLetterSpacing: 0, maximumLetterSpacing: 0,
        minimumGlyphScaling: 100, desiredGlyphScaling: 100, maximumGlyphScaling: 100
      };
      made.push(name);
    }

    add('corner-closing', '「작은 이야기」를 읽고 왔습니다.', 78, {});
    add('double-corner-closing', '『작은 이야기』를 읽고 왔습니다.', 78, {});
    add('fullwidth-paren-closing', '（작은 이야기）를 읽고 왔습니다.', 78, {});
    add('corner-unspaced', '오늘 그는「작은 이야기」를 읽고 말했습니다.', 64, {});
    add('corner-opening-left', '오늘 그는 「작은 이야기를」 읽고 말했습니다.', 64, {});
    add('corner-opening-hyphen', '오늘 그는 「작은 이야기를」 읽고 말했습니다.', 64, {hyphen: true});
    add('corner-opening-japanese', '오늘 그는 「작은 이야기를」 읽고 말했습니다.', 64, {japanese: true});
    add('corner-closing-japanese', '「작은 이야기」를 읽고 왔습니다.', 78, {japanese: true});

    var englishFirst = 'supercalifragilisticexpialidocious 단어도 여러 줄에 놓습니다.';
    add('latin-first-roman', englishFirst, 173, {hyphen: true});
    add('latin-first-japanese', englishFirst, 173, {hyphen: true, japanese: true});
    add('latin-first-japanese-narrow', englishFirst, 172, {hyphen: true, japanese: true});
    add('latin-first-japanese-wide', englishFirst, 174, {hyphen: true, japanese: true});
    add('latin-first-japanese-wider', englishFirst, 177, {hyphen: true, japanese: true});
    add('latin-first-japanese-justify', englishFirst, 173, {hyphen: true, justify: true, japanese: true});
    doc.recompose();
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(config.out));
  } catch (e) {
    error = String(e.message) + ' at ' + e.line;
  } finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { error = String(e2.message); }
    prefs.userInteractionLevel = old.ui;
    prefs.measurementUnit = old.unit;
    prefs.enableRedraw = old.redraw;
  }
  if (error) return '{"ok":false,"error":"' + error.split('"').join("'") + '"}';
  return '{"ok":true,"made":["' + made.join('","') + '"]}';
})();
