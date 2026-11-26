(function(){
  var CONFIG=CONFIG_PLACEHOLDER,doc=null,errors=[],made=[],verticalError='';
  if(app.documents.length)return '{"ok":false,"error":"documents open"}';
  var sp=app.scriptPreferences,saved={ui:sp.userInteractionLevel,unit:sp.measurementUnit,redraw:sp.enableRedraw};
  try{
    sp.userInteractionLevel=UserInteractionLevels.NEVER_INTERACT;sp.measurementUnit=MeasurementUnits.POINTS;sp.enableRedraw=false;
    doc=app.documents.add(false);doc.documentPreferences.properties={facingPages:false,pageWidth:420,pageHeight:595,pagesPerDocument:5};
    doc.paragraphStyles[1].properties={appliedFont:app.fonts.itemByName('Arial\tRegular'),fontStyle:'Regular',pointSize:14,leading:21,hyphenation:false};
    var texts=[
      ['ja','Yu Gothic\tRegular','日本語の組版を確認します。句読点、括弧（テスト）、数字123と英字ABCを含みます。文字の間隔と改行の位置を比べます。印刷物の読みやすさを大切にします。'],
      ['zh','Microsoft YaHei\tRegular','我们比较中文简体的排版效果。这里包含标点符号、括号（测试）、数字123和英文ABC。检查文字间距、换行位置以及页面上的实际印刷结果。'],
      ['ar','Arial\tRegular','نختبر تنسيق النص العربي في الصفحة المطبوعة. هذه جملة تحتوي على أرقام 123 وكلمات ABC وعلامات الترقيم. نراجع اتصال الحروف وترتيب الكلمات والمسافات بين السطور.'],
      ['he','Arial\tRegular','אנחנו בודקים את סידור הטקסט בעברית בעמוד המודפס. המשפט כולל מספרים 123 ומילים ABC וסימני פיסוק. נשווה את סדר האותיות ואת המרווחים בין השורות.'],
      ['ja-vertical','Yu Gothic\tRegular','日本語の縦書きを確認します。句読点、括弧（テスト）、数字と英字を含みます。文字の向きと改行位置を比べます。']
    ];
    for(var i=0;i<texts.length;i++){
      var row=texts[i],f=doc.pages[i].textFrames.add({geometricBounds:[40,40,550,380]});
      f.insertLabel('oracle.id',row[0]);f.contents=row[2]+'\r'+row[2];
      var p=f.parentStory.paragraphs.everyItem();
      p.properties={appliedFont:app.fonts.itemByName(row[1]),fontStyle:'Regular',pointSize:14,leading:21,hyphenation:false,
        composer:i===2||i===3?'$ID/HL Composer Optyca':'$ID/HL Composer J'};
      if(i===2||i===3)p.properties={paragraphDirection:ParagraphDirectionOptions.RIGHT_TO_LEFT_DIRECTION,justification:Justification.RIGHT_ALIGN};
      if(i===4) {
        try {f.parentStory.storyPreferences.storyOrientation=StoryHorizontalOrVertical.VERTICAL;}
        catch(verticalUnavailable) {verticalError=verticalUnavailable.message;doc.pages[i].remove();break;}
      }
      made.push(row[0]);
    }
    doc.exportFile(ExportFormat.INDESIGN_MARKUP,File(CONFIG.out));
  }catch(e){errors.push(e.message+' @'+e.line);}
  finally{try{if(doc)doc.close(SaveOptions.NO);}catch(e2){errors.push(e2.message);}
    sp.userInteractionLevel=saved.ui;sp.measurementUnit=saved.unit;sp.enableRedraw=saved.redraw;}
  return '{"ok":'+(errors.length===0)+',"made":["'+made.join('","')+'"],"verticalError":"'+verticalError.replace(/"/g,"'")+'","errors":"'+errors.join('; ').replace(/"/g,"'")+'"}';
})();
