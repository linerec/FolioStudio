// Text-variable fixture: four facing pages (US Letter). The parent's header frames (left and right) hold, in order: a Running Header by paragraph style
// (Head), a Running Header by character style (Accent), the page number, the Last Page Number, the Chapter Number and a custom text.
// The pages hold the Head paragraphs the headers read: page 1 "Alpha Heading", page 2 "Beta" and "Gamma Second" (the first on the page
// counts), page 3 none (the header of an earlier page carries on), page 4 "Delta Ending"; the Accent character style marks "Zulu" on
// page 1, "Yankee" on page 2 and nothing on pages 3 and 4. File Name and the dates depend on the file and the day, so they are left out.
// Hidden document, system font only, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null;
  function on(page, b) { var p = page.bounds; return [p[0]+b[0],p[1]+b[1],p[0]+b[2],p[1]+b[3]]; }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    doc.marginPreferences.properties = { top:36, bottom:36, left:36, right:36 };
    doc.masterSpreads[0].pageItems.everyItem().remove();
    doc.pages[0].pageItems.everyItem().remove();
    doc.documentPreferences.properties = { facingPages:true, pagesPerDocument:4 };
    var font = app.fonts.itemByName('Malgun Gothic\tRegular'), bold = app.fonts.itemByName('Malgun Gothic\tBold');
    var head = doc.paragraphStyles.add({ name:'Head', appliedFont:font, pointSize:20, leading:24 });
    var body = doc.paragraphStyles.add({ name:'Body', appliedFont:font, pointSize:12, leading:16 });
    var accent = doc.characterStyles.add({ name:'Accent', appliedFont:bold });
    // the variables
    var byHead = doc.textVariables.add({ name:'By Head', variableType:VariableTypes.MATCH_PARAGRAPH_STYLE_TYPE });
    byHead.variableOptions.appliedParagraphStyle = head;
    var byAccent = doc.textVariables.add({ name:'By Accent', variableType:VariableTypes.MATCH_CHARACTER_STYLE_TYPE });
    byAccent.variableOptions.appliedCharacterStyle = accent;
    var custom = doc.textVariables.add({ name:'Custom Note', variableType:VariableTypes.CUSTOM_TEXT_TYPE });
    custom.variableOptions.contents = 'Custom note';
    var last = doc.textVariables.itemByName('Last Page Number'), chapter = doc.textVariables.itemByName('Chapter Number');
    // the parent's header, on both parent pages
    var master = doc.masterSpreads[0], tag = ['L', 'R'];
    for (var q = 0; q < master.pages.length; q++) {
      var mp = master.pages[q];
      var hf = mp.textFrames.add({ geometricBounds:on(mp,[10,36,34,576]) });
      hf.insertLabel('oracle.id', 'header-' + tag[q]);
      var story = hf.parentStory;
      var text = function (t) { story.insertionPoints[-1].contents = t; };
      var variable = function (v) { story.textVariableInstances.add(LocationOptions.AT_END, story.insertionPoints[-1], { associatedTextVariable:v }); };
      text('H:'); variable(byHead); text(' A:'); variable(byAccent); text(' P'); story.insertionPoints[-1].contents = SpecialCharacters.AUTO_PAGE_NUMBER;
      text('/'); variable(last); text(' C'); variable(chapter); text(' '); variable(custom);
      story.texts[0].properties = { appliedFont:font, pointSize:9, leading:12 };
    }
    // the pages
    var heads = [['Alpha Heading', 'Zulu'], ['Beta', 'Yankee'], [null, null], ['Delta Ending', null]];
    for (var i = 0; i < doc.pages.length; i++) {
      var page = doc.pages[i];
      var tf = page.textFrames.add({ geometricBounds:on(page,[60,36,400,576]) });
      tf.insertLabel('oracle.id', 'body-' + i);
      var lines = [];
      if (heads[i][0]) lines.push(heads[i][0]);
      if (i == 1) lines.push('Gamma Second');
      lines.push('Body paragraph of page ' + (i + 1) + ' with a word.');
      tf.contents = lines.join('\r');
      var ps = tf.parentStory.paragraphs;
      for (var k = 0; k < ps.length; k++) ps[k].appliedParagraphStyle = (ps[k].contents.replace(/\r/g, '') == 'Body paragraph of page ' + (i + 1) + ' with a word.') ? body : head;
      if (heads[i][1]) {
        var found = tf.parentStory.paragraphs[-1].words[-3];
        found.contents = heads[i][1];
        found.appliedCharacterStyle = accent;
      }
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true}';
  } catch(e) {
    return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';
  } finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
})();
