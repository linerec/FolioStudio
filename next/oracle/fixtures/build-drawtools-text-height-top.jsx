(function(){
var c=CONFIG_PLACEHOLDER,d=null,sp=app.scriptPreferences,old={unit:sp.measurementUnit,ui:sp.userInteractionLevel,redraw:sp.enableRedraw};
try{sp.measurementUnit=MeasurementUnits.POINTS;sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.enableRedraw=false;
d=app.open(File(File(c.out).parent.fsName+'/basic-create.idml'),false);
d.textFramePreferences.properties={autoSizingType:AutoSizingTypeEnum.HEIGHT_ONLY, autoSizingReferencePoint:AutoSizingReferenceEnum.TOP_LEFT_POINT};
d.exportFile(ExportFormat.INDESIGN_MARKUP,File(c.out));return '{"ok":true,"made":[]}';
}catch(e){return '{"ok":false,"error":"'+String(e).replace(/"/g,"'")+'"}';}
finally{if(d)d.close(SaveOptions.NO);sp.measurementUnit=old.unit;sp.userInteractionLevel=old.ui;sp.enableRedraw=old.redraw;}
})();