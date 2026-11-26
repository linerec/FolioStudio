(function(){
  var CONFIG=CONFIG_PLACEHOLDER,sp=app.scriptPreferences;
  var saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw},doc=null,error='';
  try{
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:300,pageHeight:240};
    var names=['rotated-alpha','oval-photoshop','inverse-edges','sheared-alpha'];
    for(var n=0;n<4;n++){
      var x=35+(n%2)*140,y=35+Math.floor(n/2)*100;
      var collection=n===1?doc.pages[0].ovals:doc.pages[0].rectangles;
      var f=collection.add({geometricBounds:[y,x,y+64,x+64],strokeWeight:0});f.insertLabel('oracle.id',names[n]);
      f.place(File(File(CONFIG.image).parent.fsName+'/objopts-paths.psd'));
      var cp=f.allGraphics[0].clippingPath;
      var p={clippingType:n===1?ClippingPathType.PHOTOSHOP_PATH:n===2?ClippingPathType.DETECT_EDGES:ClippingPathType.ALPHA_CHANNEL,insetFrame:0};
      if(n===1)p.appliedPathName=cp.photoshopPathNames[0];
      if(n===0||n===3)p.appliedPathName=cp.alphaChannelPathNames[0];
      if(n===2)p.invertPath=true;
      cp.properties=p;
      if(n===0)f.rotationAngle=17;
      if(n===3)f.shearAngle=12;
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){error=String(e).replace(/"/g,"'");}finally{if(doc)doc.close(SaveOptions.NO);sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
  return '{"ok":'+(!error)+',"made":["rotated-alpha","oval-photoshop","inverse-edges","sheared-alpha"],"error":"'+error+'"}';
}());
