// Page-panel fixture: distinct page-owned objects, an image, and current-page markers.
// Hidden document, system font only, no persistent application preference changes.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences, saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, made = [];
  function label(it, name) { it.insertLabel('oracle.id', name); made.push(name); return it; }
  function on(page, b) { var p = page.bounds; return [p[0]+b[0],p[1]+b[1],p[0]+b[2],p[1]+b[3]]; }
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS,
      verticalMeasurementUnits: MeasurementUnits.POINTS, rulerOrigin: RulerOrigin.SPREAD_ORIGIN };
    doc.documentPreferences.properties = { facingPages:true, pageWidth:300, pageHeight:400, pagesPerDocument:5,
      allowPageShuffle:true, preserveLayoutWhenShuffling:true };
    doc.marginPreferences.properties = { top:24, bottom:24, left:24, right:24 };
    var black = doc.swatches.itemByName('Black'), none = doc.swatches.itemByName('None');
    var font = app.fonts.itemByName('Malgun Gothic\tRegular');
    function text(page, name, b, content) {
      var f = label(page.textFrames.add({geometricBounds:on(page,b)}),name);
      f.contents = content;
      f.parentStory.texts[0].properties = {appliedFont:font, pointSize:12, leading:16};
      return f;
    }
    for (var m=0; m<2; m++) {
      var master = m ? doc.masterSpreads.add({namePrefix:'B',baseName:'Alternate'}) : doc.masterSpreads[0];
      for(var q=0;q<master.pages.length;q++) {
        var mp=master.pages[q];
        var bar=mp.rectangles.add({geometricBounds:on(mp,[15,24,19+m*4,80]),fillColor:black,strokeColor:none});
        var tf=text(mp,'master-'+m+'-'+q,[365,24,390,120],'');
        tf.insertionPoints[0].contents=SpecialCharacters.AUTO_PAGE_NUMBER;
      }
    }
    for(var i=0;i<doc.pages.length;i++) {
      var page=doc.pages[i];
      label(page.rectangles.add({geometricBounds:on(page,[50+i*12,30,80+i*12,90+i*10]),
        fillColor:black,fillTint:20+i*15,strokeColor:none}),'box-'+i);
      text(page,'text-'+i,[200,30,250,250],'Page '+(i+1)+' / ');
      doc.textFrames.item(0); // creation order is not used to locate objects
      var frames=page.textFrames;
      frames[0].insertionPoints[-1].contents=SpecialCharacters.AUTO_PAGE_NUMBER;
    }
    var image=label(doc.pages[2].rectangles.add({geometricBounds:on(doc.pages[2],[100,150,170,220])}),'image');
    image.place(File(CONFIG.image)); image.fit(FitOptions.PROPORTIONALLY);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":'+made.toSource().replace(/'/g,'"')+'}';
  } catch(e) {
    return '{"ok":false,"error":"'+String(e.message).replace(/["\\\r\n]/g,' ')+' @'+e.line+'"}';
  } finally {
    if(doc)doc.close(SaveOptions.NO);
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;
  }
})();
