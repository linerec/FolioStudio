(function(){
  var CONFIG=CONFIG_PLACEHOLDER,sp=app.scriptPreferences;
  var saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw},doc=null,error='';
  try{
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:300,pageHeight:200};
    var names=['none','edges','alpha','photoshop'],types=[ClippingPathType.NONE,ClippingPathType.DETECT_EDGES,ClippingPathType.ALPHA_CHANNEL,ClippingPathType.PHOTOSHOP_PATH];
    for(var n=0;n<4;n++){
      var x=20+(n%2)*140,y=20+Math.floor(n/2)*85;
      var f=doc.pages[0].rectangles.add({geometricBounds:[y,x,y+64,x+64],strokeWeight:0});f.insertLabel('oracle.id',names[n]);
      f.place(File(File(CONFIG.image).parent.fsName+'/objopts-paths.psd'));
      var cp=f.allGraphics[0].clippingPath;
      var p={clippingType:types[n],insetFrame:0};
      if(n===2)p.appliedPathName=cp.alphaChannelPathNames[0];
      if(n===3)p.appliedPathName=cp.photoshopPathNames[0];
      cp.properties=p;
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){error=String(e).replace(/"/g,"'");}finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
  return '{"ok":'+(!error)+',"made":["none","edges","alpha","photoshop"],"error":"'+error+'"}';
}());
