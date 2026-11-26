// Derived object-style fixture. Run capture.py build basic-styles; basic.idml must exist.
// Object styles: "Oracle Box" (fill, stroke, corners, opacity), "Oracle Box Blue" based on it
// (fill only), "Oracle Text" (text frame columns and inset). r-fill has "Oracle Box" with a
// local stroke weight override; the other labelled objects keep their styles.
(function () {
  var CONFIG=CONFIG_PLACEHOLDER;
  if(app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences, saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  var d=null;
  try {
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    d=app.open(File(File(CONFIG.out).parent.fsName+'/basic.idml'),false);
    var box=d.objectStyles.add({name:'Oracle Box'});
    box.enableFill=true; box.fillColor=d.swatches.itemByName('Oracle Orange');
    box.enableStroke=true; box.strokeColor=d.swatches.itemByName('Black'); box.strokeWeight=2;
    box.enableStrokeAndCornerOptions=true;
    box.topLeftCornerOption=box.topRightCornerOption=box.bottomLeftCornerOption=box.bottomRightCornerOption=CornerOptions.ROUNDED_CORNER;
    box.topLeftCornerRadius=box.topRightCornerRadius=box.bottomLeftCornerRadius=box.bottomRightCornerRadius=8;
    box.objectEffectsEnablingSettings.enableTransparency=true; box.transparencySettings.blendingSettings.opacity=70;
    var blue=d.objectStyles.add({name:'Oracle Box Blue', basedOn:box});
    blue.enableFill=true; blue.fillColor=d.swatches.itemByName('Oracle Blue');
    var text=d.objectStyles.add({name:'Oracle Text'});
    text.enableTextFrameGeneralOptions=true;
    text.textFramePreferences.textColumnCount=2; text.textFramePreferences.insetSpacing=[6,6,6,6];
    var all=d.allPageItems, find=function(l){for(var i=0;i<all.length;i++) if(all[i].extractLabel('oracle.id')==l) return all[i];};
    var rf=find('r-fill'); rf.applyObjectStyle(box, true); rf.strokeWeight=4;
    d.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
    return '{"ok":true,"made":["Oracle Box","Oracle Box Blue","Oracle Text"]}';
  } catch(e) {return '{"ok":false,"error":"'+String(e).replace(/"/g,"'")+'"}';}
  finally {if(d)d.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
})();
