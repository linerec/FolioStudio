// hyphdict-c1: Character panel Language -> the hyphenation dictionary, in natural text (British English, German, French, Spanish).
// Eight justified frames (220 pt wide) in Arial 12/15, language [No Language] (no hyphenation: Folio and InDesign start alike); the scenarios set the story's language.
(function () {
  var CONFIG = CONFIG_PLACEHOLDER;
  if (app.documents.length) return '{"ok":false,"error":"documents open"}';
  var sp = app.scriptPreferences;
  var saved = { ui: sp.userInteractionLevel, unit: sp.measurementUnit, redraw: sp.enableRedraw };
  var doc = null, errors = [];
  try {
    sp.userInteractionLevel = UserInteractionLevels.NEVER_INTERACT;
    sp.measurementUnit = MeasurementUnits.POINTS; sp.enableRedraw = false;
    doc = app.documents.add(false);
    doc.documentPreferences.properties = { facingPages: false, pageWidth: 595, pageHeight: 842, pagesPerDocument: 2 };
    doc.viewPreferences.properties = { horizontalMeasurementUnits: MeasurementUnits.POINTS, verticalMeasurementUnits: MeasurementUnits.POINTS };
    var arial = app.fonts.itemByName('Arial\tRegular');
    doc.paragraphStyles[1].properties = { appliedFont: arial, fontStyle: 'Regular' };
    var body = doc.paragraphStyles.add({ name: 'Hd Body', appliedFont: arial, fontStyle: 'Regular', pointSize: 12, leading: 15, justification: Justification.LEFT_ALIGN, hyphenation: true, hyphenationZone: 0, hyphenateWordsLongerThan: 5, hyphenateAfterFirst: 2, hyphenateBeforeLast: 2, appliedLanguage: '[No Language]' });
    function frame(id, bounds, text, page) {
      var f = doc.pages[page].textFrames.add({ geometricBounds: bounds });
      f.insertLabel('oracle.id', id); f.contents = text;
      f.parentStory.paragraphs.everyItem().applyParagraphStyle(body, true);
      return f;
    }
    frame("uk1", [36, 36, 216, 256], "The municipal administration announced yesterday several significant transformations concerning the reorganisation of public transportation. Particularly the modernisation of railway infrastructure, the enlargement of cycling networks and the development of environmentally responsible equipment represent fundamental priorities. Furthermore, the government considers comprehensive restructuring.", 0);
    frame("uk2", [36, 296, 216, 516], "Throughout the summer holidays we encountered innumerable hikers who thoroughly enjoyed the extraordinarily picturesque mountain scenery. Late afternoon we reached the refuge, where the cheerful innkeeper presented delicious traditional vegetable soup accompanied by freshly prepared bread. Subsequently we undertook a considerable expedition.", 0);
    frame("de1", [226, 36, 406, 256], "Die Bundesregierung beschloss gestern umfangreiche Maßnahmen zur Verbesserung der Verkehrsinfrastruktur. Besonders die Sanierung der Autobahnbrücken, die Erweiterung des Schienennetzes und die Förderung umweltfreundlicher Fahrzeugtechnologien stehen im Mittelpunkt. Verkehrsminister Hoffmann kündigte außerdem eine Überprüfung sämtlicher Genehmigungsverfahren an.", 0);
    frame("de2", [226, 296, 406, 516], "Unterwegs begegneten wir zahlreichen Wanderern, die sich über das ungewöhnlich schöne Herbstwetter freuten. Nachmittags erreichten wir die Berghütte, wo uns der freundliche Wirt hausgemachte Kartoffelsuppe und selbstgebackenes Brot servierte. Anschließend unternahmen wir einen ausgedehnten Spaziergang durch die Kastanienallee.", 0);
    frame("fr1", [416, 36, 596, 256], "L'administration municipale a annoncé hier d'importantes transformations concernant l'aménagement des transports publics. Notamment la modernisation des infrastructures ferroviaires, l'élargissement du réseau cyclable et le développement d'équipements écologiquement responsables constituent les priorités principales. Le gouvernement envisage également une réorganisation complète.", 0);
    frame("fr2", [416, 296, 596, 516], "Pendant les vacances d'été, nous avons rencontré énormément de randonneurs qui profitaient pleinement du magnifique paysage montagneux. L'après-midi, nous avons atteint le refuge où l'aimable aubergiste nous a présenté une délicieuse soupe traditionnelle accompagnée d'un pain fraîchement préparé. Ensuite nous avons entrepris une longue promenade.", 0);
    frame("es1", [606, 36, 786, 256], "El ayuntamiento anunció ayer importantes transformaciones relacionadas con la reorganización del transporte público. Especialmente la modernización de las infraestructuras ferroviarias, la ampliación de la red ciclista y el desarrollo de instalaciones medioambientalmente responsables constituyen las prioridades fundamentales. Además, el gobierno contempla una reestructuración completa.", 0);
    frame("es2", [606, 296, 786, 516], "Durante las vacaciones de verano encontramos innumerables excursionistas que disfrutaban plenamente del extraordinario paisaje montañoso. Por la tarde llegamos al refugio, donde el amable posadero nos presentó una deliciosa sopa tradicional acompañada de pan recién horneado. Posteriormente emprendimos una larga caminata.", 0);
    frame("uk3", [36, 36, 216, 256], "The committee published a comprehensive assessment of undergraduate programmes at neighbouring universities. Notwithstanding considerable disagreements, representatives acknowledged that international collaboration substantially strengthens research opportunities, particularly regarding environmental conservation and sustainable agriculture.", 1);
    frame("uk4", [36, 296, 216, 516], "Yesterday evening the headmaster organised an extraordinary celebration for the departing teachers. Unfortunately, the weather forecast predicted thunderstorms, therefore everybody gathered indoors, enjoying traditional refreshments, entertaining conversations and wonderful photographs of previous generations.", 1);
    frame("de3", [226, 36, 406, 256], "Die Landesregierung veröffentlichte gestern eine ausführliche Untersuchung über Möglichkeiten zur Reduzierung des Energieverbrauchs. Insbesondere die Gebäudesanierung, die Förderung erneuerbarer Energiequellen und die Einführung verbindlicher Effizienzstandards sollen langfristig wirtschaftliche Vorteile bringen.", 1);
    frame("de4", [226, 296, 406, 516], "Während der Sommerferien besuchten meine Großeltern zahlreiche Sehenswürdigkeiten in Norddeutschland. Besonders beeindruckend fanden sie die mittelalterlichen Backsteinkirchen, die Hafenanlagen sowie das ausgezeichnete Fischrestaurant unmittelbar neben dem historischen Marktplatz.", 1);
    doc.exportFile(ExportFormat.INDESIGN_MARKUP, File(CONFIG.out));
  } catch (e) { errors.push(String(e.message) + ' @' + e.line); }
  finally {
    try { if (doc) doc.close(SaveOptions.NO); } catch (e2) { errors.push(e2.message); }
    sp.userInteractionLevel = saved.ui; sp.measurementUnit = saved.unit; sp.enableRedraw = saved.redraw;
  }
  return '{"ok":' + (errors.length == 0) + ',"made":["uk1", "uk2", "de1", "de2", "fr1", "fr2", "es1", "es2", "uk3", "uk4", "de3", "de4"],"errors":"' + errors.join('; ').replace(/"/g, "'") + '"}';
})();
