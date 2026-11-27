// Folio Studio menu definitions. Edit menu structure and shortcuts here.
// Edit bilingual labels in scripts/menu-labels.json and run scripts/gen-menutree.py.
import type { MenuDef, MenuEntry } from './menudef';
import { t } from '../i18n';

const c = (cmd: string): MenuEntry => ({ cmd });
const S: MenuEntry = { sep: true };
const m = (key: string, sub: MenuEntry[]): MenuEntry => ({ get label() { return t(key); }, sub });

/** Label of File ▸ Open Recent; buildMenus() fills it with the recent projects. */
export const RECENT_SUBMENU = 'menu.recent';

/** Menu commands with translated labels and default shortcuts. */
export const MENU_SPECS: Record<string, { label: string; keys?: string[] }> = {
  'id.file.new.book': { get label() { return t('command.id.file.new.book'); } }, // File › New › Book...
  'id.file.new.library': { get label() { return t('command.id.file.new.library'); } }, // File › New › Library...
  'id.file.importPdfComments': { get label() { return t('command.id.file.importPdfComments'); } }, // File › Import PDF Comments...
  'id.file.pdfPresets.define': { get label() { return t('command.id.file.pdfPresets.define'); } }, // File › PDF Presets › Define...
  'id.file.pdfPresets.highQualityPrint': { get label() { return t('command.id.file.pdfPresets.highQualityPrint'); } }, // File › PDF Presets › [High Quality Print]...
  'id.file.pdfPresets.pdfX1a2001': { get label() { return t('command.id.file.pdfPresets.pdfX1a2001'); } }, // File › PDF Presets › [PDF/X-1a:2001]...
  'id.file.pdfPresets.pdfX32002': { get label() { return t('command.id.file.pdfPresets.pdfX32002'); } }, // File › PDF Presets › [PDF/X-3:2002]...
  'id.file.pdfPresets.pdfX42008': { get label() { return t('command.id.file.pdfPresets.pdfX42008'); } }, // File › PDF Presets › [PDF/X-4:2008]...
  'id.file.pdfPresets.pressQuality': { get label() { return t('command.id.file.pdfPresets.pressQuality'); } }, // File › PDF Presets › [Press Quality]...
  'id.file.pdfPresets.smallestFileSize': { get label() { return t('command.id.file.pdfPresets.smallestFileSize'); } }, // File › PDF Presets › [Smallest File Size]...
  'id.file.documentPresets.define': { get label() { return t('command.id.file.documentPresets.define'); } }, // File › Document Presets › Define...
  'id.file.documentPresets.default': { get label() { return t('command.id.file.documentPresets.default'); } }, // File › Document Presets › [Default]...
  'id.file.user': { get label() { return t('command.id.file.user'); } }, // File › User...
  'id.file.userSettings.importUserSettings': { get label() { return t('command.id.file.userSettings.importUserSettings'); } }, // File › User Settings › Import User Settings...
  'id.file.userSettings.exportUserSettings': { get label() { return t('command.id.file.userSettings.exportUserSettings'); } }, // File › User Settings › Export User Settings...
  'id.file.printPresets.define': { get label() { return t('command.id.file.printPresets.define'); } }, // File › Print Presets › Define...
  'id.file.printPresets.default': { get label() { return t('command.id.file.printPresets.default'); } }, // File › Print Presets › [Default]...
  'id.file.printBooklet': { get label() { return t('command.id.file.printBooklet'); } }, // File › Print Booklet...
  'id.edit.placeAndLink': { get label() { return t('command.id.edit.placeAndLink'); } }, // Edit › Place and Link
  'id.edit.editOriginal': { get label() { return t('command.id.edit.editOriginal'); } }, // Edit › Edit Original
  'id.edit.editWith.other': { get label() { return t('command.id.edit.editWith.other'); } }, // Edit › Edit With › Other...
  'id.list.editWith': { get label() { return t('command.id.list.editWith'); } }, // (동적 목록 자리)
  'id.edit.goToSource': { get label() { return t('command.id.edit.goToSource'); } }, // Edit › Go To Source
  'id.edit.spelling.dynamicSpelling': { get label() { return t('command.id.edit.spelling.dynamicSpelling'); } }, // Edit › Spelling › Dynamic Spelling
  'id.edit.spelling.autocorrect': { get label() { return t('command.id.edit.spelling.autocorrect'); } }, // Edit › Spelling › Autocorrect
  'id.edit.spelling.userDictionary': { get label() { return t('command.id.edit.spelling.userDictionary'); } }, // Edit › Spelling › User Dictionary...
  'id.edit.transparencyBlendSpace.documentRgb': { get label() { return t('command.id.edit.transparencyBlendSpace.documentRgb'); } }, // Edit › Transparency Blend Space › Document RGB
  'id.edit.transparencyBlendSpace.documentCmyk': { get label() { return t('command.id.edit.transparencyBlendSpace.documentCmyk'); } }, // Edit › Transparency Blend Space › Document CMYK
  'id.edit.transparencyFlattenerPresets': { get label() { return t('command.id.edit.transparencyFlattenerPresets'); } }, // Edit › Transparency Flattener Presets...
  'id.edit.assignProfiles': { get label() { return t('command.id.edit.assignProfiles'); } }, // Edit › Assign Profiles...
  'id.edit.convertToProfile': { get label() { return t('command.id.edit.convertToProfile'); } }, // Edit › Convert to Profile...
  'id.edit.preferences.interface': { get label() { return t('command.id.edit.preferences.interface'); } }, // Edit › Preferences › Interface...
  'id.edit.preferences.userInterfaceScaling': { get label() { return t('command.id.edit.preferences.userInterfaceScaling'); } }, // Edit › Preferences › User Interface Scaling...
  'id.edit.preferences.type': { get label() { return t('command.id.edit.preferences.type'); } }, // Edit › Preferences › Type...
  'id.edit.preferences.advancedType': { get label() { return t('command.id.edit.preferences.advancedType'); } }, // Edit › Preferences › Advanced Type...
  'id.edit.preferences.composition': { get label() { return t('command.id.edit.preferences.composition'); } }, // Edit › Preferences › Composition...
  'id.edit.preferences.unitsAndIncrements': { get label() { return t('command.id.edit.preferences.unitsAndIncrements'); } }, // Edit › Preferences › Units & Increments...
  'id.edit.preferences.grids': { get label() { return t('command.id.edit.preferences.grids'); } }, // Edit › Preferences › Grids...
  'id.edit.preferences.guidesAndPasteboard': { get label() { return t('command.id.edit.preferences.guidesAndPasteboard'); } }, // Edit › Preferences › Guides & Pasteboard...
  'id.edit.preferences.dictionary': { get label() { return t('command.id.edit.preferences.dictionary'); } }, // Edit › Preferences › Dictionary...
  'id.edit.preferences.spelling': { get label() { return t('command.id.edit.preferences.spelling'); } }, // Edit › Preferences › Spelling...
  'id.edit.preferences.autocorrect': { get label() { return t('command.id.edit.preferences.autocorrect'); } }, // Edit › Preferences › Autocorrect...
  'id.edit.preferences.notes': { get label() { return t('command.id.edit.preferences.notes'); } }, // Edit › Preferences › Notes...
  'id.edit.preferences.trackChanges': { get label() { return t('command.id.edit.preferences.trackChanges'); } }, // Edit › Preferences › Track Changes...
  'id.edit.preferences.storyEditorDisplay': { get label() { return t('command.id.edit.preferences.storyEditorDisplay'); } }, // Edit › Preferences › Story Editor Display...
  'id.edit.preferences.displayPerformance': { get label() { return t('command.id.edit.preferences.displayPerformance'); } }, // Edit › Preferences › Display Performance...
  'id.edit.preferences.appearanceOfBlack': { get label() { return t('command.id.edit.preferences.appearanceOfBlack'); } }, // Edit › Preferences › Appearance of Black...
  'id.edit.preferences.fileHandling': { get label() { return t('command.id.edit.preferences.fileHandling'); } }, // Edit › Preferences › File Handling...
  'id.edit.preferences.clipboardHandling': { get label() { return t('command.id.edit.preferences.clipboardHandling'); } }, // Edit › Preferences › Clipboard Handling...
  'id.layout.pages.duplicateSpread': { get label() { return t('command.id.layout.pages.duplicateSpread'); } }, // Layout › Pages › Duplicate Spread
  'id.layout.pages.hideSpread': { get label() { return t('command.id.layout.pages.hideSpread'); } }, // Layout › Pages › Hide Spread
  'id.layout.pages.printPages': { get label() { return t('command.id.layout.pages.printPages'); } }, // Layout › Pages › Print Pages...
  'id.layout.pages.applyParentToPages': { get label() { return t('command.id.layout.pages.applyParentToPages'); } }, // Layout › Pages › Apply Parent to Pages...
  'id.layout.updateTableOfContents': { get label() { return t('command.id.layout.updateTableOfContents'); } }, // Layout › Update Table of Contents
  'id.layout.tableOfContentsStyles': { get label() { return t('command.id.layout.tableOfContentsStyles'); } }, // Layout › Table of Contents Styles...
  'id.list.fonts': { get label() { return t('command.id.list.fonts'); } }, // (동적 목록 자리)
  'id.type.size.other': { get label() { return t('command.id.type.size.other'); } }, // Type › Size › Other...
  'id.type.size.6Pt': { get label() { return t('command.id.type.size.6Pt'); } }, // Type › Size › 6 pt
  'id.type.size.8Pt': { get label() { return t('command.id.type.size.8Pt'); } }, // Type › Size › 8 pt
  'id.type.size.9Pt': { get label() { return t('command.id.type.size.9Pt'); } }, // Type › Size › 9 pt
  'id.type.size.10Pt': { get label() { return t('command.id.type.size.10Pt'); } }, // Type › Size › 10 pt
  'id.type.size.11Pt': { get label() { return t('command.id.type.size.11Pt'); } }, // Type › Size › 11 pt
  'id.type.size.12Pt': { get label() { return t('command.id.type.size.12Pt'); } }, // Type › Size › 12 pt
  'id.type.size.14Pt': { get label() { return t('command.id.type.size.14Pt'); } }, // Type › Size › 14 pt
  'id.type.size.18Pt': { get label() { return t('command.id.type.size.18Pt'); } }, // Type › Size › 18 pt
  'id.type.size.24Pt': { get label() { return t('command.id.type.size.24Pt'); } }, // Type › Size › 24 pt
  'id.type.size.30Pt': { get label() { return t('command.id.type.size.30Pt'); } }, // Type › Size › 30 pt
  'id.type.size.36Pt': { get label() { return t('command.id.type.size.36Pt'); } }, // Type › Size › 36 pt
  'id.type.size.48Pt': { get label() { return t('command.id.type.size.48Pt'); } }, // Type › Size › 48 pt
  'id.type.size.60Pt': { get label() { return t('command.id.type.size.60Pt'); } }, // Type › Size › 60 pt
  'id.type.size.72Pt': { get label() { return t('command.id.type.size.72Pt'); } }, // Type › Size › 72 pt
  'id.type.changeCase.uppercase': { get label() { return t('command.id.type.changeCase.uppercase'); } }, // Type › Change Case › UPPERCASE
  'id.type.changeCase.lowercase': { get label() { return t('command.id.type.changeCase.lowercase'); } }, // Type › Change Case › lowercase
  'id.type.changeCase.titleCase': { get label() { return t('command.id.type.changeCase.titleCase'); } }, // Type › Change Case › Title Case
  'id.type.changeCase.sentenceCase': { get label() { return t('command.id.type.changeCase.sentenceCase'); } }, // Type › Change Case › Sentence case
  'id.type.typeOnAPath.options': { get label() { return t('command.id.type.typeOnAPath.options'); } }, // Type › Type on a Path › Options...
  'id.type.typeOnAPath.deleteTypeFromPath': { get label() { return t('command.id.type.typeOnAPath.deleteTypeFromPath'); } }, // Type › Type on a Path › Delete Type from Path
  'id.type.notes.notesMode': { get label() { return t('command.id.type.notes.notesMode'); } }, // Type › Notes › Notes Mode
  'id.type.notes.newNote': { get label() { return t('command.id.type.notes.newNote'); } }, // Type › Notes › New Note
  'id.type.notes.openNote': { get label() { return t('command.id.type.notes.openNote'); } }, // Type › Notes › Open Note
  'id.type.notes.deleteNote': { get label() { return t('command.id.type.notes.deleteNote'); } }, // Type › Notes › Delete Note
  'id.type.notes.previousNote': { get label() { return t('command.id.type.notes.previousNote'); } }, // Type › Notes › Previous Note
  'id.type.notes.nextNote': { get label() { return t('command.id.type.notes.nextNote'); } }, // Type › Notes › Next Note
  'id.type.notes.convertToNote': { get label() { return t('command.id.type.notes.convertToNote'); } }, // Type › Notes › Convert to Note
  'id.type.notes.convertToText': { get label() { return t('command.id.type.notes.convertToText'); } }, // Type › Notes › Convert to Text
  'id.type.notes.splitNote': { get label() { return t('command.id.type.notes.splitNote'); } }, // Type › Notes › Split Note
  'id.type.notes.expandCollapseNotesInStory': { get label() { return t('command.id.type.notes.expandCollapseNotesInStory'); } }, // Type › Notes › Expand/Collapse Notes in Story
  'id.type.notes.removeNotesFromStory': { get label() { return t('command.id.type.notes.removeNotesFromStory'); } }, // Type › Notes › Remove Notes from Story
  'id.type.notes.removeAllNotes': { get label() { return t('command.id.type.notes.removeAllNotes'); } }, // Type › Notes › Remove All Notes
  'id.type.trackChanges.trackChangesInCurrentStory': { get label() { return t('command.id.type.trackChanges.trackChangesInCurrentStory'); } }, // Type › Track Changes › Track Changes in Current Story
  'id.type.trackChanges.enableTrackingInAllStories': { get label() { return t('command.id.type.trackChanges.enableTrackingInAllStories'); } }, // Type › Track Changes › Enable Tracking in All Stories
  'id.type.trackChanges.disableTrackingInAllStories': { get label() { return t('command.id.type.trackChanges.disableTrackingInAllStories'); } }, // Type › Track Changes › Disable Tracking in All Stories
  'id.type.trackChanges.previousChange': { get label() { return t('command.id.type.trackChanges.previousChange'); } }, // Type › Track Changes › Previous Change
  'id.type.trackChanges.nextChange': { get label() { return t('command.id.type.trackChanges.nextChange'); } }, // Type › Track Changes › Next Change
  'id.type.trackChanges.acceptChange': { get label() { return t('command.id.type.trackChanges.acceptChange'); } }, // Type › Track Changes › Accept Change
  'id.type.trackChanges.rejectChange': { get label() { return t('command.id.type.trackChanges.rejectChange'); } }, // Type › Track Changes › Reject Change
  'id.type.trackChanges.acceptChangeFindNext': { get label() { return t('command.id.type.trackChanges.acceptChangeFindNext'); } }, // Type › Track Changes › Accept Change, Find Next
  'id.type.trackChanges.rejectChangeFindNext': { get label() { return t('command.id.type.trackChanges.rejectChangeFindNext'); } }, // Type › Track Changes › Reject Change, Find Next
  'id.type.trackChanges.acceptAllChanges.inThisStory': { get label() { return t('command.id.type.trackChanges.acceptAllChanges.inThisStory'); } }, // Type › Track Changes › Accept All Changes › In This Story
  'id.type.trackChanges.acceptAllChanges.inThisDocument': { get label() { return t('command.id.type.trackChanges.acceptAllChanges.inThisDocument'); } }, // Type › Track Changes › Accept All Changes › In This Document
  'id.type.trackChanges.rejectAllChanges.inThisStory': { get label() { return t('command.id.type.trackChanges.rejectAllChanges.inThisStory'); } }, // Type › Track Changes › Reject All Changes › In This Story
  'id.type.trackChanges.rejectAllChanges.inThisDocument': { get label() { return t('command.id.type.trackChanges.rejectAllChanges.inThisDocument'); } }, // Type › Track Changes › Reject All Changes › In This Document
  'id.type.trackChanges.acceptAllChangesByThisUser.inThisStory': { get label() { return t('command.id.type.trackChanges.acceptAllChangesByThisUser.inThisStory'); } }, // Type › Track Changes › Accept All Changes by This User › In This Story
  'id.type.trackChanges.acceptAllChangesByThisUser.inThisDocument': { get label() { return t('command.id.type.trackChanges.acceptAllChangesByThisUser.inThisDocument'); } }, // Type › Track Changes › Accept All Changes by This User › In This Document
  'id.type.trackChanges.rejectAllChangesByThisUser.inThisStory': { get label() { return t('command.id.type.trackChanges.rejectAllChangesByThisUser.inThisStory'); } }, // Type › Track Changes › Reject All Changes by This User › In This Story
  'id.type.trackChanges.rejectAllChangesByThisUser.inThisDocument': { get label() { return t('command.id.type.trackChanges.rejectAllChangesByThisUser.inThisDocument'); } }, // Type › Track Changes › Reject All Changes by This User › In This Document
  'id.type.insertFootnote': { get label() { return t('command.id.type.insertFootnote'); } }, // Type › Insert Footnote
  'id.type.documentFootnoteOptions': { get label() { return t('command.id.type.documentFootnoteOptions'); } }, // Type › Document Footnote Options...
  'id.type.insertEndnote': { get label() { return t('command.id.type.insertEndnote'); } }, // Type › Insert Endnote
  'id.type.documentEndnoteOptions': { get label() { return t('command.id.type.documentEndnoteOptions'); } }, // Type › Document Endnote Options...
  'id.type.convertFootnotesAndEndnotes': { get label() { return t('command.id.type.convertFootnotesAndEndnotes'); } }, // Type › Convert Footnotes and Endnotes...
  'id.type.hyperlinksAndCrossReferences.newHyperlink': { get label() { return t('command.id.type.hyperlinksAndCrossReferences.newHyperlink'); } }, // Type › Hyperlinks & Cross-References › New Hyperlink...
  'id.type.hyperlinksAndCrossReferences.editHyperlink': { get label() { return t('command.id.type.hyperlinksAndCrossReferences.editHyperlink'); } }, // Type › Hyperlinks & Cross-References › Edit Hyperlink...
  'id.type.hyperlinksAndCrossReferences.insertCrossReference': { get label() { return t('command.id.type.hyperlinksAndCrossReferences.insertCrossReference'); } }, // Type › Hyperlinks & Cross-References › Insert Cross-Reference...
  'id.type.hyperlinksAndCrossReferences.crossReferenceOptions': { get label() { return t('command.id.type.hyperlinksAndCrossReferences.crossReferenceOptions'); } }, // Type › Hyperlinks & Cross-References › Cross-Reference Options...
  'id.type.hyperlinksAndCrossReferences.updateCrossReference': { get label() { return t('command.id.type.hyperlinksAndCrossReferences.updateCrossReference'); } }, // Type › Hyperlinks & Cross-References › Update Cross-Reference
  'id.type.textVariables.define': { get label() { return t('command.id.type.textVariables.define'); } }, // Type › Text Variables › Define...
  'id.type.textVariables.insertVariable.chapterNumber': { get label() { return t('command.id.type.textVariables.insertVariable.chapterNumber'); } }, // Type › Text Variables › Insert Variable › Chapter Number
  'id.type.textVariables.insertVariable.creationDate': { get label() { return t('command.id.type.textVariables.insertVariable.creationDate'); } }, // Type › Text Variables › Insert Variable › Creation Date
  'id.type.textVariables.insertVariable.fileName': { get label() { return t('command.id.type.textVariables.insertVariable.fileName'); } }, // Type › Text Variables › Insert Variable › File Name
  'id.type.textVariables.insertVariable.imageName': { get label() { return t('command.id.type.textVariables.insertVariable.imageName'); } }, // Type › Text Variables › Insert Variable › Image Name
  'id.type.textVariables.insertVariable.lastPageNumber': { get label() { return t('command.id.type.textVariables.insertVariable.lastPageNumber'); } }, // Type › Text Variables › Insert Variable › Last Page Number
  'id.type.textVariables.insertVariable.modificationDate': { get label() { return t('command.id.type.textVariables.insertVariable.modificationDate'); } }, // Type › Text Variables › Insert Variable › Modification Date
  'id.type.textVariables.insertVariable.outputDate': { get label() { return t('command.id.type.textVariables.insertVariable.outputDate'); } }, // Type › Text Variables › Insert Variable › Output Date
  'id.type.textVariables.insertVariable.runningHeader': { get label() { return t('command.id.type.textVariables.insertVariable.runningHeader'); } }, // Type › Text Variables › Insert Variable › Running Header
  'id.type.textVariables.convertVariableToText': { get label() { return t('command.id.type.textVariables.convertVariableToText'); } }, // Type › Text Variables › Convert Variable to Text
  'id.type.bulletedAndNumberedLists.applyRemoveBullets': { get label() { return t('command.id.type.bulletedAndNumberedLists.applyRemoveBullets'); } }, // Type › Bulleted & Numbered Lists › Apply/Remove Bullets
  'id.type.bulletedAndNumberedLists.applyRemoveNumbers': { get label() { return t('command.id.type.bulletedAndNumberedLists.applyRemoveNumbers'); } }, // Type › Bulleted & Numbered Lists › Apply/Remove Numbers
  'id.type.bulletedAndNumberedLists.restartContinueNumbering': { get label() { return t('command.id.type.bulletedAndNumberedLists.restartContinueNumbering'); } }, // Type › Bulleted & Numbered Lists › Restart/Continue Numbering
  'id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText': { get label() { return t('command.id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText'); } }, // Type › Bulleted & Numbered Lists › Convert Bullets and Numbering to Text
  'id.type.bulletedAndNumberedLists.defineLists': { get label() { return t('command.id.type.bulletedAndNumberedLists.defineLists'); } }, // Type › Bulleted & Numbered Lists › Define Lists...
  'id.type.insertSpecialCharacter.symbols.bulletCharacter': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.bulletCharacter'); } }, // Type › Insert Special Character › Symbols › Bullet Character
  'id.type.insertSpecialCharacter.symbols.copyrightSymbol': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.copyrightSymbol'); } }, // Type › Insert Special Character › Symbols › Copyright Symbol
  'id.type.insertSpecialCharacter.symbols.ellipsis': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.ellipsis'); } }, // Type › Insert Special Character › Symbols › Ellipsis
  'id.type.insertSpecialCharacter.symbols.paragraphSymbol': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.paragraphSymbol'); } }, // Type › Insert Special Character › Symbols › Paragraph Symbol
  'id.type.insertSpecialCharacter.symbols.registeredTrademarkSymbol': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.registeredTrademarkSymbol'); } }, // Type › Insert Special Character › Symbols › Registered Trademark Symbol
  'id.type.insertSpecialCharacter.symbols.sectionSymbol': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.sectionSymbol'); } }, // Type › Insert Special Character › Symbols › Section Symbol
  'id.type.insertSpecialCharacter.symbols.trademarkSymbol': { get label() { return t('command.id.type.insertSpecialCharacter.symbols.trademarkSymbol'); } }, // Type › Insert Special Character › Symbols › Trademark Symbol
  'id.type.insertSpecialCharacter.markers.currentPageNumber': { get label() { return t('command.id.type.insertSpecialCharacter.markers.currentPageNumber'); } }, // Type › Insert Special Character › Markers › Current Page Number
  'id.type.insertSpecialCharacter.markers.nextPageNumber': { get label() { return t('command.id.type.insertSpecialCharacter.markers.nextPageNumber'); } }, // Type › Insert Special Character › Markers › Next Page Number
  'id.type.insertSpecialCharacter.markers.previousPageNumber': { get label() { return t('command.id.type.insertSpecialCharacter.markers.previousPageNumber'); } }, // Type › Insert Special Character › Markers › Previous Page Number
  'id.type.insertSpecialCharacter.markers.sectionMarker': { get label() { return t('command.id.type.insertSpecialCharacter.markers.sectionMarker'); } }, // Type › Insert Special Character › Markers › Section Marker
  'id.type.insertSpecialCharacter.markers.footnoteNumber': { get label() { return t('command.id.type.insertSpecialCharacter.markers.footnoteNumber'); } }, // Type › Insert Special Character › Markers › Footnote Number
  'id.type.insertSpecialCharacter.hyphensAndDashes.emDash': { get label() { return t('command.id.type.insertSpecialCharacter.hyphensAndDashes.emDash'); } }, // Type › Insert Special Character › Hyphens and Dashes › Em Dash
  'id.type.insertSpecialCharacter.hyphensAndDashes.enDash': { get label() { return t('command.id.type.insertSpecialCharacter.hyphensAndDashes.enDash'); } }, // Type › Insert Special Character › Hyphens and Dashes › En Dash
  'id.type.insertSpecialCharacter.hyphensAndDashes.discretionaryHyphen': { get label() { return t('command.id.type.insertSpecialCharacter.hyphensAndDashes.discretionaryHyphen'); } }, // Type › Insert Special Character › Hyphens and Dashes › Discretionary Hyphen
  'id.type.insertSpecialCharacter.hyphensAndDashes.nonbreakingHyphen': { get label() { return t('command.id.type.insertSpecialCharacter.hyphensAndDashes.nonbreakingHyphen'); } }, // Type › Insert Special Character › Hyphens and Dashes › Nonbreaking Hyphen
  'id.type.insertSpecialCharacter.quotationMarks.doubleLeftQuotationMarks': { get label() { return t('command.id.type.insertSpecialCharacter.quotationMarks.doubleLeftQuotationMarks'); } }, // Type › Insert Special Character › Quotation Marks › Double Left Quotation Marks
  'id.type.insertSpecialCharacter.quotationMarks.doubleRightQuotationMarks': { get label() { return t('command.id.type.insertSpecialCharacter.quotationMarks.doubleRightQuotationMarks'); } }, // Type › Insert Special Character › Quotation Marks › Double Right Quotation Marks
  'id.type.insertSpecialCharacter.quotationMarks.singleLeftQuotationMark': { get label() { return t('command.id.type.insertSpecialCharacter.quotationMarks.singleLeftQuotationMark'); } }, // Type › Insert Special Character › Quotation Marks › Single Left Quotation Mark
  'id.type.insertSpecialCharacter.quotationMarks.singleRightQuotationMark': { get label() { return t('command.id.type.insertSpecialCharacter.quotationMarks.singleRightQuotationMark'); } }, // Type › Insert Special Character › Quotation Marks › Single Right Quotation Mark
  'id.type.insertSpecialCharacter.quotationMarks.straightDoubleQuotationMarks': { get label() { return t('command.id.type.insertSpecialCharacter.quotationMarks.straightDoubleQuotationMarks'); } }, // Type › Insert Special Character › Quotation Marks › Straight Double Quotation Marks
  'id.type.insertSpecialCharacter.quotationMarks.straightSingleQuotationMarkApostrophe': { get label() { return t('command.id.type.insertSpecialCharacter.quotationMarks.straightSingleQuotationMarkApostrophe'); } }, // Type › Insert Special Character › Quotation Marks › Straight Single Quotation Mark (Apostrophe)
  'id.type.insertSpecialCharacter.other.tab': { get label() { return t('command.id.type.insertSpecialCharacter.other.tab'); } }, // Type › Insert Special Character › Other › Tab
  'id.type.insertSpecialCharacter.other.rightIndentTab': { get label() { return t('command.id.type.insertSpecialCharacter.other.rightIndentTab'); } }, // Type › Insert Special Character › Other › Right Indent Tab
  'id.type.insertSpecialCharacter.other.indentToHere': { get label() { return t('command.id.type.insertSpecialCharacter.other.indentToHere'); } }, // Type › Insert Special Character › Other › Indent to Here
  'id.type.insertSpecialCharacter.other.endNestedStyleHere': { get label() { return t('command.id.type.insertSpecialCharacter.other.endNestedStyleHere'); } }, // Type › Insert Special Character › Other › End Nested Style Here
  'id.type.insertSpecialCharacter.other.nonJoiner': { get label() { return t('command.id.type.insertSpecialCharacter.other.nonJoiner'); } }, // Type › Insert Special Character › Other › Non-joiner
  'id.type.insertWhiteSpace.emSpace': { get label() { return t('command.id.type.insertWhiteSpace.emSpace'); } }, // Type › Insert White Space › Em Space
  'id.type.insertWhiteSpace.enSpace': { get label() { return t('command.id.type.insertWhiteSpace.enSpace'); } }, // Type › Insert White Space › En Space
  'id.type.insertWhiteSpace.nonbreakingSpace': { get label() { return t('command.id.type.insertWhiteSpace.nonbreakingSpace'); } }, // Type › Insert White Space › Nonbreaking Space
  'id.type.insertWhiteSpace.nonbreakingSpaceFixedWidth': { get label() { return t('command.id.type.insertWhiteSpace.nonbreakingSpaceFixedWidth'); } }, // Type › Insert White Space › Nonbreaking Space (Fixed Width)
  'id.type.insertWhiteSpace.hairSpace': { get label() { return t('command.id.type.insertWhiteSpace.hairSpace'); } }, // Type › Insert White Space › Hair Space
  'id.type.insertWhiteSpace.sixthSpace': { get label() { return t('command.id.type.insertWhiteSpace.sixthSpace'); } }, // Type › Insert White Space › Sixth Space
  'id.type.insertWhiteSpace.thinSpace': { get label() { return t('command.id.type.insertWhiteSpace.thinSpace'); } }, // Type › Insert White Space › Thin Space
  'id.type.insertWhiteSpace.quarterSpace': { get label() { return t('command.id.type.insertWhiteSpace.quarterSpace'); } }, // Type › Insert White Space › Quarter Space
  'id.type.insertWhiteSpace.thirdSpace': { get label() { return t('command.id.type.insertWhiteSpace.thirdSpace'); } }, // Type › Insert White Space › Third Space
  'id.type.insertWhiteSpace.punctuationSpace': { get label() { return t('command.id.type.insertWhiteSpace.punctuationSpace'); } }, // Type › Insert White Space › Punctuation Space
  'id.type.insertWhiteSpace.figureSpace': { get label() { return t('command.id.type.insertWhiteSpace.figureSpace'); } }, // Type › Insert White Space › Figure Space
  'id.type.insertWhiteSpace.flushSpace': { get label() { return t('command.id.type.insertWhiteSpace.flushSpace'); } }, // Type › Insert White Space › Flush Space
  'id.type.insertBreakCharacter.columnBreak': { get label() { return t('command.id.type.insertBreakCharacter.columnBreak'); } }, // Type › Insert Break Character › Column Break
  'id.type.insertBreakCharacter.frameBreak': { get label() { return t('command.id.type.insertBreakCharacter.frameBreak'); } }, // Type › Insert Break Character › Frame Break
  'id.type.insertBreakCharacter.pageBreak': { get label() { return t('command.id.type.insertBreakCharacter.pageBreak'); } }, // Type › Insert Break Character › Page Break
  'id.type.insertBreakCharacter.oddPageBreak': { get label() { return t('command.id.type.insertBreakCharacter.oddPageBreak'); } }, // Type › Insert Break Character › Odd Page Break
  'id.type.insertBreakCharacter.evenPageBreak': { get label() { return t('command.id.type.insertBreakCharacter.evenPageBreak'); } }, // Type › Insert Break Character › Even Page Break
  'id.type.insertBreakCharacter.paragraphReturn': { get label() { return t('command.id.type.insertBreakCharacter.paragraphReturn'); } }, // Type › Insert Break Character › Paragraph Return
  'id.type.insertBreakCharacter.forcedLineBreak': { get label() { return t('command.id.type.insertBreakCharacter.forcedLineBreak'); } }, // Type › Insert Break Character › Forced Line Break
  'id.type.insertBreakCharacter.discretionaryLineBreak': { get label() { return t('command.id.type.insertBreakCharacter.discretionaryLineBreak'); } }, // Type › Insert Break Character › Discretionary Line Break
  'id.object.transform.scale': { get label() { return t('command.id.object.transform.scale'); } }, // Object › Transform › Scale...
  'id.object.transform.rotate': { get label() { return t('command.id.object.transform.rotate'); } }, // Object › Transform › Rotate...
  'id.object.transform.shear': { get label() { return t('command.id.object.transform.shear'); } }, // Object › Transform › Shear...
  'id.object.transform.rotate90DegCw': { get label() { return t('command.id.object.transform.rotate90DegCw'); } }, // Object › Transform › Rotate 90° CW
  'id.object.transform.rotate90DegCcw': { get label() { return t('command.id.object.transform.rotate90DegCcw'); } }, // Object › Transform › Rotate 90° CCW
  'id.object.transform.rotate180Deg': { get label() { return t('command.id.object.transform.rotate180Deg'); } }, // Object › Transform › Rotate 180°
  'id.object.transform.flipHorizontal': { get label() { return t('command.id.object.transform.flipHorizontal'); } }, // Object › Transform › Flip Horizontal
  'id.object.transform.flipVertical': { get label() { return t('command.id.object.transform.flipVertical'); } }, // Object › Transform › Flip Vertical
  'id.object.transform.clearTransformations': { get label() { return t('command.id.object.transform.clearTransformations'); } }, // Object › Transform › Clear Transformations
  'id.object.transformAgain.transformAgainIndividually': { get label() { return t('command.id.object.transformAgain.transformAgainIndividually'); } }, // Object › Transform Again › Transform Again Individually
  'id.object.transformAgain.transformSequenceAgainIndividually': { get label() { return t('command.id.object.transformAgain.transformSequenceAgainIndividually'); } }, // Object › Transform Again › Transform Sequence Again Individually
  'id.object.insertMathml': { get label() { return t('command.id.object.insertMathml'); } }, // Object › Insert MathML...
  'id.object.generateQrCode': { get label() { return t('command.id.object.generateQrCode'); } }, // Object › Generate QR Code...
  'id.object.anchoredObject.options': { get label() { return t('command.id.object.anchoredObject.options'); } }, // Object › Anchored Object › Options...
  'id.object.anchoredObject.insert': { get label() { return t('command.id.object.anchoredObject.insert'); } }, // Object › Anchored Object › Insert...
  'id.object.anchoredObject.release': { get label() { return t('command.id.object.anchoredObject.release'); } }, // Object › Anchored Object › Release
  'id.object.fitting.clearFrameFittingOptions': { get label() { return t('command.id.object.fitting.clearFrameFittingOptions'); } }, // Object › Fitting › Clear Frame Fitting Options
  'id.object.content.graphic': { get label() { return t('command.id.object.content.graphic'); } }, // Object › Content › Graphic
  'id.object.content.text': { get label() { return t('command.id.object.content.text'); } }, // Object › Content › Text
  'id.object.content.unassigned': { get label() { return t('command.id.object.content.unassigned'); } }, // Object › Content › Unassigned
  'id.object.effects.transparency': { get label() { return t('command.id.object.effects.transparency'); } }, // Object › Effects › Transparency...
  'id.object.effects.innerShadow': { get label() { return t('command.id.object.effects.innerShadow'); } }, // Object › Effects › Inner Shadow...
  'id.object.effects.outerGlow': { get label() { return t('command.id.object.effects.outerGlow'); } }, // Object › Effects › Outer Glow...
  'id.object.effects.innerGlow': { get label() { return t('command.id.object.effects.innerGlow'); } }, // Object › Effects › Inner Glow...
  'id.object.effects.bevelAndEmboss': { get label() { return t('command.id.object.effects.bevelAndEmboss'); } }, // Object › Effects › Bevel and Emboss...
  'id.object.effects.satin': { get label() { return t('command.id.object.effects.satin'); } }, // Object › Effects › Satin...
  'id.object.effects.basicFeather': { get label() { return t('command.id.object.effects.basicFeather'); } }, // Object › Effects › Basic Feather...
  'id.object.effects.directionalFeather': { get label() { return t('command.id.object.effects.directionalFeather'); } }, // Object › Effects › Directional Feather...
  'id.object.effects.gradientFeather': { get label() { return t('command.id.object.effects.gradientFeather'); } }, // Object › Effects › Gradient Feather...
  'id.object.effects.clearEffects': { get label() { return t('command.id.object.effects.clearEffects'); } }, // Object › Effects › Clear Effects
  'id.object.effects.clearAllTransparency': { get label() { return t('command.id.object.effects.clearAllTransparency'); } }, // Object › Effects › Clear All Transparency
  'id.object.effects.globalLight': { get label() { return t('command.id.object.effects.globalLight'); } }, // Object › Effects › Global Light...
  'id.object.captions.captionSetup': { get label() { return t('command.id.object.captions.captionSetup'); } }, // Object › Captions › Caption Setup...
  'id.object.captions.generateLiveCaption': { get label() { return t('command.id.object.captions.generateLiveCaption'); } }, // Object › Captions › Generate Live Caption
  'id.object.captions.generateStaticCaption': { get label() { return t('command.id.object.captions.generateStaticCaption'); } }, // Object › Captions › Generate Static Caption
  'id.object.captions.convertToStaticCaption': { get label() { return t('command.id.object.captions.convertToStaticCaption'); } }, // Object › Captions › Convert to Static Caption
  'id.object.clippingPath.options': { get label() { return t('command.id.object.clippingPath.options'); }, keys: ['Ctrl+Alt+Shift+K'] }, // Object › Clipping Path › Options...
  'id.object.clippingPath.convertClippingPathToFrame': { get label() { return t('command.id.object.clippingPath.convertClippingPathToFrame'); } }, // Object › Clipping Path › Convert Clipping Path to Frame
  'id.object.imageColorSettings': { get label() { return t('command.id.object.imageColorSettings'); } }, // Object › Image Color Settings...
  'id.object.paths.join': { get label() { return t('command.id.object.paths.join'); } }, // Object › Paths › Join
  'id.object.paths.openPath': { get label() { return t('command.id.object.paths.openPath'); } }, // Object › Paths › Open Path
  'id.object.paths.closePath': { get label() { return t('command.id.object.paths.closePath'); } }, // Object › Paths › Close Path
  'id.object.paths.reversePath': { get label() { return t('command.id.object.paths.reversePath'); } }, // Object › Paths › Reverse Path
  'id.object.paths.releaseCompoundPath': { get label() { return t('command.id.object.paths.releaseCompoundPath'); }, keys: ['Ctrl+Alt+Shift+8'] }, // Object › Paths › Release Compound Path
  'id.object.pathfinder.add': { get label() { return t('command.id.object.pathfinder.add'); } }, // Object › Pathfinder › Add
  'id.object.pathfinder.subtract': { get label() { return t('command.id.object.pathfinder.subtract'); } }, // Object › Pathfinder › Subtract
  'id.object.pathfinder.intersect': { get label() { return t('command.id.object.pathfinder.intersect'); } }, // Object › Pathfinder › Intersect
  'id.object.pathfinder.excludeOverlap': { get label() { return t('command.id.object.pathfinder.excludeOverlap'); } }, // Object › Pathfinder › Exclude Overlap
  'id.object.pathfinder.minusBack': { get label() { return t('command.id.object.pathfinder.minusBack'); } }, // Object › Pathfinder › Minus Back
  'id.object.convertShape.rectangle': { get label() { return t('command.id.object.convertShape.rectangle'); } }, // Object › Convert Shape › Rectangle
  'id.object.convertShape.roundedRectangle': { get label() { return t('command.id.object.convertShape.roundedRectangle'); } }, // Object › Convert Shape › Rounded Rectangle
  'id.object.convertShape.beveledRectangle': { get label() { return t('command.id.object.convertShape.beveledRectangle'); } }, // Object › Convert Shape › Beveled Rectangle
  'id.object.convertShape.inverseRoundedRectangle': { get label() { return t('command.id.object.convertShape.inverseRoundedRectangle'); } }, // Object › Convert Shape › Inverse Rounded Rectangle
  'id.object.convertShape.ellipse': { get label() { return t('command.id.object.convertShape.ellipse'); } }, // Object › Convert Shape › Ellipse
  'id.object.convertShape.triangle': { get label() { return t('command.id.object.convertShape.triangle'); } }, // Object › Convert Shape › Triangle
  'id.object.convertShape.polygon': { get label() { return t('command.id.object.convertShape.polygon'); } }, // Object › Convert Shape › Polygon
  'id.object.convertShape.line': { get label() { return t('command.id.object.convertShape.line'); } }, // Object › Convert Shape › Line
  'id.object.convertShape.orthogonalLine': { get label() { return t('command.id.object.convertShape.orthogonalLine'); } }, // Object › Convert Shape › Orthogonal Line
  'id.object.convertPoint.plain': { get label() { return t('command.id.object.convertPoint.plain'); } }, // Object › Convert Point › Plain
  'id.object.convertPoint.corner': { get label() { return t('command.id.object.convertPoint.corner'); } }, // Object › Convert Point › Corner
  'id.object.convertPoint.smooth': { get label() { return t('command.id.object.convertPoint.smooth'); } }, // Object › Convert Point › Smooth
  'id.object.convertPoint.symmetrical': { get label() { return t('command.id.object.convertPoint.symmetrical'); } }, // Object › Convert Point › Symmetrical
  'id.object.displayPerformance.fastDisplay': { get label() { return t('command.id.object.displayPerformance.fastDisplay'); } }, // Object › Display Performance › Fast Display
  'id.object.displayPerformance.typicalDisplay': { get label() { return t('command.id.object.displayPerformance.typicalDisplay'); } }, // Object › Display Performance › Typical Display
  'id.object.displayPerformance.highQualityDisplay': { get label() { return t('command.id.object.displayPerformance.highQualityDisplay'); } }, // Object › Display Performance › High Quality Display
  'id.object.displayPerformance.useViewSetting': { get label() { return t('command.id.object.displayPerformance.useViewSetting'); } }, // Object › Display Performance › Use View Setting
  'id.table.convertTableToText': { get label() { return t('command.id.table.convertTableToText'); } }, // Table › Convert Table to Text...
  'id.table.tableOptions.tableSetup': { get label() { return t('command.id.table.tableOptions.tableSetup'); }, keys: ['Ctrl+Alt+Shift+B'] }, // Table › Table Options › Table Setup...
  'id.table.tableOptions.alternatingRowStrokes': { get label() { return t('command.id.table.tableOptions.alternatingRowStrokes'); } }, // Table › Table Options › Alternating Row Strokes...
  'id.table.tableOptions.alternatingColumnStrokes': { get label() { return t('command.id.table.tableOptions.alternatingColumnStrokes'); } }, // Table › Table Options › Alternating Column Strokes...
  'id.table.tableOptions.alternatingFills': { get label() { return t('command.id.table.tableOptions.alternatingFills'); } }, // Table › Table Options › Alternating Fills...
  'id.table.tableOptions.headersAndFooters': { get label() { return t('command.id.table.tableOptions.headersAndFooters'); } }, // Table › Table Options › Headers and Footers...
  'id.table.cellOptions.text': { get label() { return t('command.id.table.cellOptions.text'); } }, // Table › Cell Options › Text...
  'id.table.cellOptions.graphic': { get label() { return t('command.id.table.cellOptions.graphic'); } }, // Table › Cell Options › Graphic...
  'id.table.cellOptions.strokesAndFills': { get label() { return t('command.id.table.cellOptions.strokesAndFills'); } }, // Table › Cell Options › Strokes and Fills...
  'id.table.cellOptions.rowsAndColumns': { get label() { return t('command.id.table.cellOptions.rowsAndColumns'); } }, // Table › Cell Options › Rows and Columns...
  'id.table.cellOptions.diagonalLines': { get label() { return t('command.id.table.cellOptions.diagonalLines'); } }, // Table › Cell Options › Diagonal Lines...
  'id.table.insert.row': { get label() { return t('command.id.table.insert.row'); } }, // Table › Insert › Row...
  'id.table.insert.column': { get label() { return t('command.id.table.insert.column'); } }, // Table › Insert › Column...
  'id.table.delete.row': { get label() { return t('command.id.table.delete.row'); } }, // Table › Delete › Row
  'id.table.delete.column': { get label() { return t('command.id.table.delete.column'); } }, // Table › Delete › Column
  'id.table.delete.table': { get label() { return t('command.id.table.delete.table'); } }, // Table › Delete › Table
  'id.table.select.cell': { get label() { return t('command.id.table.select.cell'); } }, // Table › Select › Cell
  'id.table.select.row': { get label() { return t('command.id.table.select.row'); } }, // Table › Select › Row
  'id.table.select.column': { get label() { return t('command.id.table.select.column'); } }, // Table › Select › Column
  'id.table.select.table': { get label() { return t('command.id.table.select.table'); } }, // Table › Select › Table
  'id.table.select.headerRows': { get label() { return t('command.id.table.select.headerRows'); } }, // Table › Select › Header Rows
  'id.table.select.bodyRows': { get label() { return t('command.id.table.select.bodyRows'); } }, // Table › Select › Body Rows
  'id.table.select.footerRows': { get label() { return t('command.id.table.select.footerRows'); } }, // Table › Select › Footer Rows
  'id.table.unmergeCells': { get label() { return t('command.id.table.unmergeCells'); } }, // Table › Unmerge Cells
  'id.table.splitCellHorizontally': { get label() { return t('command.id.table.splitCellHorizontally'); } }, // Table › Split Cell Horizontally
  'id.table.splitCellVertically': { get label() { return t('command.id.table.splitCellVertically'); } }, // Table › Split Cell Vertically
  'id.table.pasteBefore': { get label() { return t('command.id.table.pasteBefore'); } }, // Table › Paste Before
  'id.table.pasteAfter': { get label() { return t('command.id.table.pasteAfter'); } }, // Table › Paste After
  'id.table.convertCellToGraphicCell': { get label() { return t('command.id.table.convertCellToGraphicCell'); } }, // Table › Convert Cell to Graphic Cell
  'id.table.convertCellToTextCell': { get label() { return t('command.id.table.convertCellToTextCell'); } }, // Table › Convert Cell to Text Cell
  'id.table.convertRows.toHeader': { get label() { return t('command.id.table.convertRows.toHeader'); } }, // Table › Convert Rows › To Header
  'id.table.convertRows.toBody': { get label() { return t('command.id.table.convertRows.toBody'); } }, // Table › Convert Rows › To Body
  'id.table.convertRows.toFooter': { get label() { return t('command.id.table.convertRows.toFooter'); } }, // Table › Convert Rows › To Footer
  'id.table.distributeRowsEvenly': { get label() { return t('command.id.table.distributeRowsEvenly'); } }, // Table › Distribute Rows Evenly
  'id.table.distributeColumnsEvenly': { get label() { return t('command.id.table.distributeColumnsEvenly'); } }, // Table › Distribute Columns Evenly
  'id.table.goToRow': { get label() { return t('command.id.table.goToRow'); } }, // Table › Go to Row...
  'id.table.editHeader': { get label() { return t('command.id.table.editHeader'); } }, // Table › Edit Header
  'id.table.editFooter': { get label() { return t('command.id.table.editFooter'); } }, // Table › Edit Footer
  'id.view.proofSetup.custom': { get label() { return t('command.id.view.proofSetup.custom'); } }, // View › Proof Setup › Custom...
  'id.view.proofSetup.documentCmyk': { get label() { return t('command.id.view.proofSetup.documentCmyk'); } }, // View › Proof Setup › Document CMYK
  'id.view.proofSetup.workingCmyk': { get label() { return t('command.id.view.proofSetup.workingCmyk'); } }, // View › Proof Setup › Working CMYK
  'id.view.matchPasteboardToThemeColor': { get label() { return t('command.id.view.matchPasteboardToThemeColor'); } }, // View › Match Pasteboard to Theme Color
  'id.view.rotateSpread.90DegCw': { get label() { return t('command.id.view.rotateSpread.90DegCw'); } }, // View › Rotate Spread › 90° CW
  'id.view.rotateSpread.90DegCcw': { get label() { return t('command.id.view.rotateSpread.90DegCcw'); } }, // View › Rotate Spread › 90° CCW
  'id.view.rotateSpread.180Deg': { get label() { return t('command.id.view.rotateSpread.180Deg'); } }, // View › Rotate Spread › 180°
  'id.view.rotateSpread.clearRotation': { get label() { return t('command.id.view.rotateSpread.clearRotation'); } }, // View › Rotate Spread › Clear Rotation
  'id.view.displayPerformance.fastDisplay': { get label() { return t('command.id.view.displayPerformance.fastDisplay'); }, keys: ['Ctrl+Alt+Shift+Z'] }, // View › Display Performance › Fast Display
  'id.view.displayPerformance.typicalDisplay': { get label() { return t('command.id.view.displayPerformance.typicalDisplay'); }, keys: ['Ctrl+Alt+Z'] }, // View › Display Performance › Typical Display
  'id.view.displayPerformance.highQualityDisplay': { get label() { return t('command.id.view.displayPerformance.highQualityDisplay'); }, keys: ['Ctrl+Alt+H'] }, // View › Display Performance › High Quality Display
  'id.view.displayPerformance.allowObjectLevelDisplaySettings': { get label() { return t('command.id.view.displayPerformance.allowObjectLevelDisplaySettings'); } }, // View › Display Performance › Allow Object-Level Display Settings
  'id.view.displayPerformance.clearObjectLevelDisplaySettings': { get label() { return t('command.id.view.displayPerformance.clearObjectLevelDisplaySettings'); } }, // View › Display Performance › Clear Object-Level Display Settings
  'id.view.showHiddenSpreads': { get label() { return t('command.id.view.showHiddenSpreads'); } }, // View › Show Hidden Spreads
  'id.view.extras.hideNotes': { get label() { return t('command.id.view.extras.hideNotes'); } }, // View › Extras › Hide Notes
  'id.view.extras.showLinkBadge': { get label() { return t('command.id.view.extras.showLinkBadge'); } }, // View › Extras › Show Link Badge
  'id.view.extras.hideLiveCorners': { get label() { return t('command.id.view.extras.hideLiveCorners'); } }, // View › Extras › Hide Live Corners
  'id.view.extras.hideAnchoredObjectControl': { get label() { return t('command.id.view.extras.hideAnchoredObjectControl'); } }, // View › Extras › Hide Anchored Object Control
  'id.view.extras.hideConveyor': { get label() { return t('command.id.view.extras.hideConveyor'); }, keys: ['Alt+B'] }, // View › Extras › Hide Conveyor
  'id.view.gridsAndGuides.deleteAllGuidesOnSpread': { get label() { return t('command.id.view.gridsAndGuides.deleteAllGuidesOnSpread'); } }, // View › Grids & Guides › Delete All Guides on Spread
  'id.view.gridsAndGuides.snapToDocumentGrid': { get label() { return t('command.id.view.gridsAndGuides.snapToDocumentGrid'); }, keys: ["Ctrl+Shift+'"] }, // View › Grids & Guides › Snap to Document Grid
  'id.view.storyEditor.showChanges': { get label() { return t('command.id.view.storyEditor.showChanges'); } }, // View › Story Editor › Show Changes
  'id.view.storyEditor.showStyleNameColumn': { get label() { return t('command.id.view.storyEditor.showStyleNameColumn'); } }, // View › Story Editor › Show Style Name Column
  'id.view.storyEditor.showDepthRuler': { get label() { return t('command.id.view.storyEditor.showDepthRuler'); } }, // View › Story Editor › Show Depth Ruler
  'id.view.storyEditor.showParagraphBreakMarks': { get label() { return t('command.id.view.storyEditor.showParagraphBreakMarks'); } }, // View › Story Editor › Show Paragraph Break Marks
  'id.view.storyEditor.collapseAllFootnotes': { get label() { return t('command.id.view.storyEditor.collapseAllFootnotes'); } }, // View › Story Editor › Collapse All Footnotes
  'id.window.arrange.tileAllVertically': { get label() { return t('command.id.window.arrange.tileAllVertically'); } }, // Window › Arrange › Tile All Vertically
  'id.window.arrange.tileAllHorizontally': { get label() { return t('command.id.window.arrange.tileAllHorizontally'); } }, // Window › Arrange › Tile All Horizontally
  'id.window.arrange.2UpHorizontal': { get label() { return t('command.id.window.arrange.2UpHorizontal'); } }, // Window › Arrange › 2-up Horizontal
  'id.window.arrange.2UpVertical': { get label() { return t('command.id.window.arrange.2UpVertical'); } }, // Window › Arrange › 2-up Vertical
  'id.window.arrange.3UpHorizontal': { get label() { return t('command.id.window.arrange.3UpHorizontal'); } }, // Window › Arrange › 3-up Horizontal
  'id.window.arrange.3UpVertical': { get label() { return t('command.id.window.arrange.3UpVertical'); } }, // Window › Arrange › 3-up Vertical
  'id.window.arrange.3UpStacked': { get label() { return t('command.id.window.arrange.3UpStacked'); } }, // Window › Arrange › 3-up Stacked
  'id.window.arrange.4Up': { get label() { return t('command.id.window.arrange.4Up'); } }, // Window › Arrange › 4-up
  'id.window.arrange.4UpVertical': { get label() { return t('command.id.window.arrange.4UpVertical'); } }, // Window › Arrange › 4-up Vertical
  'id.window.arrange.4UpHorizontal': { get label() { return t('command.id.window.arrange.4UpHorizontal'); } }, // Window › Arrange › 4-up Horizontal
  'id.window.arrange.4UpStacked': { get label() { return t('command.id.window.arrange.4UpStacked'); } }, // Window › Arrange › 4-up Stacked
  'id.window.arrange.5UpStacked': { get label() { return t('command.id.window.arrange.5UpStacked'); } }, // Window › Arrange › 5-up Stacked
  'id.window.arrange.6Up': { get label() { return t('command.id.window.arrange.6Up'); } }, // Window › Arrange › 6-up
  'id.window.arrange.6UpStacked': { get label() { return t('command.id.window.arrange.6UpStacked'); } }, // Window › Arrange › 6-up Stacked
  'id.window.arrange.cascade': { get label() { return t('command.id.window.arrange.cascade'); } }, // Window › Arrange › Cascade
  'id.window.arrange.tile': { get label() { return t('command.id.window.arrange.tile'); } }, // Window › Arrange › Tile
  'id.window.arrange.floatInWindow': { get label() { return t('command.id.window.arrange.floatInWindow'); } }, // Window › Arrange › Float in Window
  'id.window.arrange.floatAllInWindows': { get label() { return t('command.id.window.arrange.floatAllInWindows'); } }, // Window › Arrange › Float All in Windows
  'id.window.arrange.consolidateAllWindows': { get label() { return t('command.id.window.arrange.consolidateAllWindows'); } }, // Window › Arrange › Consolidate All Windows
  'id.window.arrange.newWindow': { get label() { return t('command.id.window.arrange.newWindow'); } }, // Window › Arrange › New Window
  'id.window.arrange.splitWindow': { get label() { return t('command.id.window.arrange.splitWindow'); } }, // Window › Arrange › Split Window
  'id.window.arrange.minimize': { get label() { return t('command.id.window.arrange.minimize'); } }, // Window › Arrange › Minimize
  'id.window.workspace.newWorkspace': { get label() { return t('command.id.window.workspace.newWorkspace'); } }, // Window › Workspace › New Workspace...
  'id.window.workspace.deleteWorkspace': { get label() { return t('command.id.window.workspace.deleteWorkspace'); } }, // Window › Workspace › Delete Workspace...
  'id.window.comments.pdfComments': { get label() { return t('command.id.window.comments.pdfComments'); } }, // Window › Comments › PDF Comments
  'id.window.editorial.notes': { get label() { return t('command.id.window.editorial.notes'); } }, // Window › Editorial › Notes
  'id.window.editorial.trackChanges': { get label() { return t('command.id.window.editorial.trackChanges'); } }, // Window › Editorial › Track Changes
  'id.window.history': { get label() { return t('command.id.window.history'); } }, // Window › History
  'id.window.interactive.hyperlinks': { get label() { return t('command.id.window.interactive.hyperlinks'); } }, // Window › Interactive › Hyperlinks
  'id.window.mathExpressions': { get label() { return t('command.id.window.mathExpressions'); } }, // Window › Math Expressions
  'id.window.objectAndLayout.pathfinder': { get label() { return t('command.id.window.objectAndLayout.pathfinder'); } }, // Window › Object & Layout › Pathfinder
  'id.window.output.attributes': { get label() { return t('command.id.window.output.attributes'); } }, // Window › Output › Attributes
  'id.window.output.flattenerPreview': { get label() { return t('command.id.window.output.flattenerPreview'); } }, // Window › Output › Flattener Preview
  'id.window.output.trapPresets': { get label() { return t('command.id.window.output.trapPresets'); } }, // Window › Output › Trap Presets
  'id.window.properties': { get label() { return t('command.id.window.properties'); } }, // Window › Properties
  'id.window.styles.cellStyles': { get label() { return t('command.id.window.styles.cellStyles'); } }, // Window › Styles › Cell Styles
  'id.window.styles.tableStyles': { get label() { return t('command.id.window.styles.tableStyles'); } }, // Window › Styles › Table Styles
  'id.window.typeAndTables.conditionalText': { get label() { return t('command.id.window.typeAndTables.conditionalText'); } }, // Window › Type & Tables › Conditional Text
  'id.window.typeAndTables.crossReferences': { get label() { return t('command.id.window.typeAndTables.crossReferences'); } }, // Window › Type & Tables › Cross-References
  'id.window.typeAndTables.index': { get label() { return t('command.id.window.typeAndTables.index'); }, keys: ['Shift+F8'] }, // Window › Type & Tables › Index
  'id.window.typeAndTables.table': { get label() { return t('command.id.window.typeAndTables.table'); }, keys: ['Shift+F9'] }, // Window › Type & Tables › Table
  'id.window.utilities.backgroundTasks': { get label() { return t('command.id.window.utilities.backgroundTasks'); } }, // Window › Utilities › Background Tasks
  'id.window.utilities.dataMerge': { get label() { return t('command.id.window.utilities.dataMerge'); } }, // Window › Utilities › Data Merge
  'id.window.utilities.scriptLabel': { get label() { return t('command.id.window.utilities.scriptLabel'); } }, // Window › Utilities › Script Label
  'id.window.utilities.toolHints': { get label() { return t('command.id.window.utilities.toolHints'); } }, // Window › Utilities › Tool Hints
  'id.panel.pages.deleteSpread': { get label() { return t('command.id.panel.pages.deleteSpread'); } }, // [Pages Panel Menu] Delete Spread
  'id.panel.pages.newParent': { get label() { return t('command.id.panel.pages.newParent'); } }, // [Pages Panel Menu] New Parent...
  'id.panel.pages.parentOptions': { get label() { return t('command.id.panel.pages.parentOptions'); } }, // [Pages Panel Menu] Parent Options...
  'id.panel.pages.overrideAllParentPageItems': { get label() { return t('command.id.panel.pages.overrideAllParentPageItems'); }, keys: ['Ctrl+Alt+Shift+L'] }, // [Pages Panel Menu] Override All Parent Page Items
  'id.panel.pages.parentPages.saveAsParent': { get label() { return t('command.id.panel.pages.parentPages.saveAsParent'); } }, // [Pages Panel Menu] Parent Pages › Save as Parent
  'id.panel.pages.parentPages.loadParentPages': { get label() { return t('command.id.panel.pages.parentPages.loadParentPages'); } }, // [Pages Panel Menu] Parent Pages › Load Parent Pages...
  'id.panel.pages.parentPages.selectUnusedParents': { get label() { return t('command.id.panel.pages.parentPages.selectUnusedParents'); } }, // [Pages Panel Menu] Parent Pages › Select Unused Parents
  'id.panel.pages.parentPages.removeAllLocalOverrides': { get label() { return t('command.id.panel.pages.parentPages.removeAllLocalOverrides'); } }, // [Pages Panel Menu] Parent Pages › Remove All Local Overrides
  'id.panel.pages.parentPages.detachAllObjectsFromParent': { get label() { return t('command.id.panel.pages.parentPages.detachAllObjectsFromParent'); } }, // [Pages Panel Menu] Parent Pages › Detach All Objects from Parent
  'id.panel.pages.parentPages.allowParentItemOverridesOnSelection': { get label() { return t('command.id.panel.pages.parentPages.allowParentItemOverridesOnSelection'); } }, // [Pages Panel Menu] Parent Pages › Allow Parent Item Overrides on Selection
  'id.panel.pages.parentPages.showParentItems': { get label() { return t('command.id.panel.pages.parentPages.showParentItems'); } }, // [Pages Panel Menu] Parent Pages › Show Parent Items
  'id.panel.pages.allowDocumentPagesToShuffle': { get label() { return t('command.id.panel.pages.allowDocumentPagesToShuffle'); } }, // [Pages Panel Menu] Allow Document Pages to Shuffle
  'id.panel.pages.allowSelectedSpreadToShuffle': { get label() { return t('command.id.panel.pages.allowSelectedSpreadToShuffle'); } }, // [Pages Panel Menu] Allow Selected Spread to Shuffle
  'id.panel.pages.pageAttributes.colorLabel.none': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.none'); } }, // [Pages Panel Menu] Page Attributes › Color Label › None
  'id.panel.pages.pageAttributes.colorLabel.useParentColor': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.useParentColor'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Use Parent Color
  'id.panel.pages.pageAttributes.colorLabel.white': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.white'); } }, // [Pages Panel Menu] Page Attributes › Color Label › White
  'id.panel.pages.pageAttributes.colorLabel.pink': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.pink'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Pink
  'id.panel.pages.pageAttributes.colorLabel.gold': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.gold'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Gold
  'id.panel.pages.pageAttributes.colorLabel.yellow': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.yellow'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Yellow
  'id.panel.pages.pageAttributes.colorLabel.green': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.green'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Green
  'id.panel.pages.pageAttributes.colorLabel.lightBlue': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.lightBlue'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Light Blue
  'id.panel.pages.pageAttributes.colorLabel.lavender': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.lavender'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Lavender
  'id.panel.pages.pageAttributes.colorLabel.gray': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.gray'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Gray
  'id.panel.pages.pageAttributes.colorLabel.red': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.red'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Red
  'id.panel.pages.pageAttributes.colorLabel.orange': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.orange'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Orange
  'id.panel.pages.pageAttributes.colorLabel.tan': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.tan'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Tan
  'id.panel.pages.pageAttributes.colorLabel.darkGreen': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.darkGreen'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Dark Green
  'id.panel.pages.pageAttributes.colorLabel.blue': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.blue'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Blue
  'id.panel.pages.pageAttributes.colorLabel.violet': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.violet'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Violet
  'id.panel.pages.pageAttributes.colorLabel.black': { get label() { return t('command.id.panel.pages.pageAttributes.colorLabel.black'); } }, // [Pages Panel Menu] Page Attributes › Color Label › Black
  'id.panel.pages.pageAttributes.spreadFlattening.default': { get label() { return t('command.id.panel.pages.pageAttributes.spreadFlattening.default'); } }, // [Pages Panel Menu] Page Attributes › Spread Flattening › Default
  'id.panel.pages.pageAttributes.spreadFlattening.noneIgnoreTransparency': { get label() { return t('command.id.panel.pages.pageAttributes.spreadFlattening.noneIgnoreTransparency'); } }, // [Pages Panel Menu] Page Attributes › Spread Flattening › None (Ignore Transparency)
  'id.panel.pages.pageAttributes.spreadFlattening.custom': { get label() { return t('command.id.panel.pages.pageAttributes.spreadFlattening.custom'); } }, // [Pages Panel Menu] Page Attributes › Spread Flattening › Custom...
  'id.panel.pages.viewPages.horizontally': { get label() { return t('command.id.panel.pages.viewPages.horizontally'); } }, // [Pages Panel Menu] View Pages › Horizontally
  'id.panel.pages.viewPages.vertically': { get label() { return t('command.id.panel.pages.viewPages.vertically'); } }, // [Pages Panel Menu] View Pages › Vertically
  'id.panel.pages.panelOptions': { get label() { return t('command.id.panel.pages.panelOptions'); } }, // [Pages Panel Menu] Panel Options...
  'id.panel.layers.newLayer': { get label() { return t('command.id.panel.layers.newLayer'); } }, // [Layers Panel Menu] New Layer
  'id.panel.layers.duplicateLayer': { get label() { return t('command.id.panel.layers.duplicateLayer'); } }, // [Layers Panel Menu] Duplicate Layer
  'id.panel.layers.deleteLayer': { get label() { return t('command.id.panel.layers.deleteLayer'); } }, // [Layers Panel Menu] Delete Layer
  'id.panel.layers.layerOptions': { get label() { return t('command.id.panel.layers.layerOptions'); } }, // [Layers Panel Menu] Layer Options...
  'id.panel.layers.showAllLayers': { get label() { return t('command.id.panel.layers.showAllLayers'); } }, // [Layers Panel Menu] Show All Layers
  'id.panel.layers.unlockAllLayers': { get label() { return t('command.id.panel.layers.unlockAllLayers'); } }, // [Layers Panel Menu] Unlock All Layers
  'id.panel.layers.unlockAll': { get label() { return t('command.id.panel.layers.unlockAll'); } }, // [Layers Panel Menu] Unlock All
  'id.panel.layers.ungroupRemembersLayers': { get label() { return t('command.id.panel.layers.ungroupRemembersLayers'); } }, // [Layers Panel Menu] Ungroup Remembers Layers
  'id.panel.layers.pasteRemembersLayers': { get label() { return t('command.id.panel.layers.pasteRemembersLayers'); } }, // [Layers Panel Menu] Paste Remembers Layers
  'id.panel.layers.mergeLayers': { get label() { return t('command.id.panel.layers.mergeLayers'); } }, // [Layers Panel Menu] Merge Layers
  'id.panel.layers.deleteUnusedLayers': { get label() { return t('command.id.panel.layers.deleteUnusedLayers'); } }, // [Layers Panel Menu] Delete Unused Layers
  'id.panel.layers.selectItemS': { get label() { return t('command.id.panel.layers.selectItemS'); } }, // [Layers Panel Menu] Select Item(s)
  'id.panel.layers.selectAndFitItem': { get label() { return t('command.id.panel.layers.selectAndFitItem'); } }, // [Layers Panel Menu] Select and Fit Item
  'id.panel.layers.smallPanelRows': { get label() { return t('command.id.panel.layers.smallPanelRows'); } }, // [Layers Panel Menu] Small Panel Rows
  'id.panel.links.relink': { get label() { return t('command.id.panel.links.relink'); } }, // [Links Panel Menu] Relink...
  'id.panel.links.relinkToFolder': { get label() { return t('command.id.panel.links.relinkToFolder'); } }, // [Links Panel Menu] Relink to Folder...
  'id.panel.links.relinkFileExtension': { get label() { return t('command.id.panel.links.relinkFileExtension'); } }, // [Links Panel Menu] Relink File Extension...
  'id.panel.links.updateLink': { get label() { return t('command.id.panel.links.updateLink'); } }, // [Links Panel Menu] Update Link
  'id.panel.links.updateAllLinks': { get label() { return t('command.id.panel.links.updateAllLinks'); } }, // [Links Panel Menu] Update All Links
  'id.panel.links.goToLink': { get label() { return t('command.id.panel.links.goToLink'); } }, // [Links Panel Menu] Go To Link
  'id.panel.links.embedLink': { get label() { return t('command.id.panel.links.embedLink'); } }, // [Links Panel Menu] Embed Link
  'id.panel.links.linkOptions': { get label() { return t('command.id.panel.links.linkOptions'); } }, // [Links Panel Menu] Link Options...
  'id.panel.links.revealInExplorer': { get label() { return t('command.id.panel.links.revealInExplorer'); } }, // [Links Panel Menu] Reveal in Explorer
  'id.panel.links.utilities.searchForMissingLinks': { get label() { return t('command.id.panel.links.utilities.searchForMissingLinks'); } }, // [Links Panel Menu] Utilities › Search for Missing Links
  'id.panel.links.utilities.copyLinkSTo': { get label() { return t('command.id.panel.links.utilities.copyLinkSTo'); } }, // [Links Panel Menu] Utilities › Copy Link(s) To...
  'id.panel.links.utilities.xmpFileInfo': { get label() { return t('command.id.panel.links.utilities.xmpFileInfo'); } }, // [Links Panel Menu] Utilities › XMP File Info...
  'id.panel.links.utilities.autoSizeLinkInformation': { get label() { return t('command.id.panel.links.utilities.autoSizeLinkInformation'); } }, // [Links Panel Menu] Utilities › Auto-Size Link Information
  'id.panel.links.utilities.showLinkInformationPane': { get label() { return t('command.id.panel.links.utilities.showLinkInformationPane'); } }, // [Links Panel Menu] Utilities › Show Link Information Pane
  'id.panel.links.utilities.versions': { get label() { return t('command.id.panel.links.utilities.versions'); } }, // [Links Panel Menu] Utilities › Versions...
  'id.panel.links.copyInfo.copyFullPath': { get label() { return t('command.id.panel.links.copyInfo.copyFullPath'); } }, // [Links Panel Menu] Copy Info › Copy Full Path
  'id.panel.links.copyInfo.copyPlatformStylePath': { get label() { return t('command.id.panel.links.copyInfo.copyPlatformStylePath'); } }, // [Links Panel Menu] Copy Info › Copy Platform Style Path
  'id.panel.links.copyInfo.copyAllLinkInformation': { get label() { return t('command.id.panel.links.copyInfo.copyAllLinkInformation'); } }, // [Links Panel Menu] Copy Info › Copy All Link Information
  'id.panel.links.copyInfo.copyInfoForSelectedLinks': { get label() { return t('command.id.panel.links.copyInfo.copyInfoForSelectedLinks'); } }, // [Links Panel Menu] Copy Info › Copy Info for Selected Links
  'id.panel.links.captions.generateLiveCaptions': { get label() { return t('command.id.panel.links.captions.generateLiveCaptions'); } }, // [Links Panel Menu] Captions › Generate Live Captions
  'id.panel.links.captions.generateStaticCaptions': { get label() { return t('command.id.panel.links.captions.generateStaticCaptions'); } }, // [Links Panel Menu] Captions › Generate Static Captions
  'id.panel.links.panelOptions': { get label() { return t('command.id.panel.links.panelOptions'); } }, // [Links Panel Menu] Panel Options...
  'id.panel.stroke.showOptions': { get label() { return t('command.id.panel.stroke.showOptions'); } }, // [Stroke Panel Menu] Show Options
  'id.panel.stroke.strokeStyles': { get label() { return t('command.id.panel.stroke.strokeStyles'); } }, // [Stroke Panel Menu] Stroke Styles...
  'id.panel.color.showOptions': { get label() { return t('command.id.panel.color.showOptions'); } }, // [Color Panel Menu] Show Options
  'id.panel.color.hsb': { get label() { return t('command.id.panel.color.hsb'); } }, // [Color Panel Menu] HSB
  'id.panel.color.lab': { get label() { return t('command.id.panel.color.lab'); } }, // [Color Panel Menu] Lab
  'id.panel.color.cmyk': { get label() { return t('command.id.panel.color.cmyk'); } }, // [Color Panel Menu] CMYK
  'id.panel.color.rgb': { get label() { return t('command.id.panel.color.rgb'); } }, // [Color Panel Menu] RGB
  'id.panel.color.addToSwatches': { get label() { return t('command.id.panel.color.addToSwatches'); } }, // [Color Panel Menu] Add to Swatches
  'id.panel.swatches.newColorSwatch': { get label() { return t('command.id.panel.swatches.newColorSwatch'); } }, // [Swatches Panel Menu] New Color Swatch...
  'id.panel.swatches.newTintSwatch': { get label() { return t('command.id.panel.swatches.newTintSwatch'); } }, // [Swatches Panel Menu] New Tint Swatch...
  'id.panel.swatches.newGradientSwatch': { get label() { return t('command.id.panel.swatches.newGradientSwatch'); } }, // [Swatches Panel Menu] New Gradient Swatch...
  'id.panel.swatches.newMixedInkSwatch': { get label() { return t('command.id.panel.swatches.newMixedInkSwatch'); } }, // [Swatches Panel Menu] New Mixed Ink Swatch...
  'id.panel.swatches.newMixedInkGroup': { get label() { return t('command.id.panel.swatches.newMixedInkGroup'); } }, // [Swatches Panel Menu] New Mixed Ink Group...
  'id.panel.swatches.newColorGroup': { get label() { return t('command.id.panel.swatches.newColorGroup'); } }, // [Swatches Panel Menu] New Color Group...
  'id.panel.swatches.duplicateSwatch': { get label() { return t('command.id.panel.swatches.duplicateSwatch'); } }, // [Swatches Panel Menu] Duplicate Swatch
  'id.panel.swatches.deleteSwatch': { get label() { return t('command.id.panel.swatches.deleteSwatch'); } }, // [Swatches Panel Menu] Delete Swatch...
  'id.panel.swatches.ungroupColorGroup': { get label() { return t('command.id.panel.swatches.ungroupColorGroup'); } }, // [Swatches Panel Menu] Ungroup Color Group
  'id.panel.swatches.swatchOptions': { get label() { return t('command.id.panel.swatches.swatchOptions'); } }, // [Swatches Panel Menu] Swatch Options...
  'id.panel.swatches.colorGroupOptions': { get label() { return t('command.id.panel.swatches.colorGroupOptions'); } }, // [Swatches Panel Menu] Color Group Options...
  'id.panel.swatches.findThisColor': { get label() { return t('command.id.panel.swatches.findThisColor'); } }, // [Swatches Panel Menu] Find this Color...
  'id.panel.swatches.loadSwatches': { get label() { return t('command.id.panel.swatches.loadSwatches'); } }, // [Swatches Panel Menu] Load Swatches...
  'id.panel.swatches.saveSwatches': { get label() { return t('command.id.panel.swatches.saveSwatches'); } }, // [Swatches Panel Menu] Save Swatches...
  'id.panel.swatches.selectAllUnused': { get label() { return t('command.id.panel.swatches.selectAllUnused'); } }, // [Swatches Panel Menu] Select All Unused
  'id.panel.swatches.addUnnamedColors': { get label() { return t('command.id.panel.swatches.addUnnamedColors'); } }, // [Swatches Panel Menu] Add Unnamed Colors
  'id.panel.swatches.largeList': { get label() { return t('command.id.panel.swatches.largeList'); } }, // [Swatches Panel Menu] Large List
  'id.panel.swatches.smallList': { get label() { return t('command.id.panel.swatches.smallList'); } }, // [Swatches Panel Menu] Small List
  'id.panel.swatches.smallThumbnail': { get label() { return t('command.id.panel.swatches.smallThumbnail'); } }, // [Swatches Panel Menu] Small Thumbnail
  'id.panel.swatches.largeThumbnail': { get label() { return t('command.id.panel.swatches.largeThumbnail'); } }, // [Swatches Panel Menu] Large Thumbnail
  'id.panel.swatches.sort.allSwatchesByName': { get label() { return t('command.id.panel.swatches.sort.allSwatchesByName'); } }, // [Swatches Panel Menu] Sort › All Swatches By Name
  'id.panel.swatches.sort.allSwatchesByColorValues': { get label() { return t('command.id.panel.swatches.sort.allSwatchesByColorValues'); } }, // [Swatches Panel Menu] Sort › All Swatches By Color Values
  'id.panel.swatches.sort.selectedSwatchesByName': { get label() { return t('command.id.panel.swatches.sort.selectedSwatchesByName'); } }, // [Swatches Panel Menu] Sort › Selected Swatches By Name
  'id.panel.swatches.sort.selectedSwatchesByColorValues': { get label() { return t('command.id.panel.swatches.sort.selectedSwatchesByColorValues'); } }, // [Swatches Panel Menu] Sort › Selected Swatches By Color Values
  'id.panel.swatches.mergeSwatches': { get label() { return t('command.id.panel.swatches.mergeSwatches'); } }, // [Swatches Panel Menu] Merge Swatches
  'id.panel.swatches.inkManager': { get label() { return t('command.id.panel.swatches.inkManager'); } }, // [Swatches Panel Menu] Ink Manager...
  'id.panel.swatches.hideOptions': { get label() { return t('command.id.panel.swatches.hideOptions'); } }, // [Swatches Panel Menu] Hide Options
  'id.panel.gradient.showOptions': { get label() { return t('command.id.panel.gradient.showOptions'); } }, // [Gradient Panel Menu] Show Options
  'id.panel.info.showOptions': { get label() { return t('command.id.panel.info.showOptions'); } }, // [Info Panel Menu] Show Options
  'id.panel.info.showContentOffset': { get label() { return t('command.id.panel.info.showContentOffset'); } }, // [Info Panel Menu] Show Content Offset
  'id.panel.character.showOptions': { get label() { return t('command.id.panel.character.showOptions'); } }, // [Character Panel Menu] Show Options
  'id.panel.character.opentype.discretionaryLigatures': { get label() { return t('command.id.panel.character.opentype.discretionaryLigatures'); } }, // [Character Panel Menu] OpenType › Discretionary Ligatures
  'id.panel.character.opentype.fractions': { get label() { return t('command.id.panel.character.opentype.fractions'); } }, // [Character Panel Menu] OpenType › Fractions
  'id.panel.character.opentype.ordinal': { get label() { return t('command.id.panel.character.opentype.ordinal'); } }, // [Character Panel Menu] OpenType › Ordinal
  'id.panel.character.opentype.swash': { get label() { return t('command.id.panel.character.opentype.swash'); } }, // [Character Panel Menu] OpenType › Swash
  'id.panel.character.opentype.titlingAlternates': { get label() { return t('command.id.panel.character.opentype.titlingAlternates'); } }, // [Character Panel Menu] OpenType › Titling Alternates
  'id.panel.character.opentype.contextualAlternates': { get label() { return t('command.id.panel.character.opentype.contextualAlternates'); } }, // [Character Panel Menu] OpenType › Contextual Alternates
  'id.panel.character.opentype.allSmallCaps': { get label() { return t('command.id.panel.character.opentype.allSmallCaps'); } }, // [Character Panel Menu] OpenType › All Small Caps
  'id.panel.character.opentype.slashedZero': { get label() { return t('command.id.panel.character.opentype.slashedZero'); } }, // [Character Panel Menu] OpenType › Slashed Zero
  'id.panel.character.opentype.stylisticSets.set1': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set1'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 1
  'id.panel.character.opentype.stylisticSets.set2': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set2'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 2
  'id.panel.character.opentype.stylisticSets.set3': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set3'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 3
  'id.panel.character.opentype.stylisticSets.set4': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set4'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 4
  'id.panel.character.opentype.stylisticSets.set5': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set5'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 5
  'id.panel.character.opentype.stylisticSets.set6': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set6'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 6
  'id.panel.character.opentype.stylisticSets.set7': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set7'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 7
  'id.panel.character.opentype.stylisticSets.set8': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set8'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 8
  'id.panel.character.opentype.stylisticSets.set9': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set9'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 9
  'id.panel.character.opentype.stylisticSets.set10': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set10'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 10
  'id.panel.character.opentype.stylisticSets.set11': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set11'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 11
  'id.panel.character.opentype.stylisticSets.set12': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set12'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 12
  'id.panel.character.opentype.stylisticSets.set13': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set13'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 13
  'id.panel.character.opentype.stylisticSets.set14': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set14'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 14
  'id.panel.character.opentype.stylisticSets.set15': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set15'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 15
  'id.panel.character.opentype.stylisticSets.set16': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set16'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 16
  'id.panel.character.opentype.stylisticSets.set17': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set17'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 17
  'id.panel.character.opentype.stylisticSets.set18': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set18'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 18
  'id.panel.character.opentype.stylisticSets.set19': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set19'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 19
  'id.panel.character.opentype.stylisticSets.set20': { get label() { return t('command.id.panel.character.opentype.stylisticSets.set20'); } }, // [Character Panel Menu] OpenType › Stylistic Sets › Set 20
  'id.panel.character.opentype.positionalForms.generalForm': { get label() { return t('command.id.panel.character.opentype.positionalForms.generalForm'); } }, // [Character Panel Menu] OpenType › Positional Forms › General Form
  'id.panel.character.opentype.positionalForms.automaticForm': { get label() { return t('command.id.panel.character.opentype.positionalForms.automaticForm'); } }, // [Character Panel Menu] OpenType › Positional Forms › Automatic Form
  'id.panel.character.opentype.positionalForms.initialForm': { get label() { return t('command.id.panel.character.opentype.positionalForms.initialForm'); } }, // [Character Panel Menu] OpenType › Positional Forms › Initial Form
  'id.panel.character.opentype.positionalForms.medialForm': { get label() { return t('command.id.panel.character.opentype.positionalForms.medialForm'); } }, // [Character Panel Menu] OpenType › Positional Forms › Medial Form
  'id.panel.character.opentype.positionalForms.finalForm': { get label() { return t('command.id.panel.character.opentype.positionalForms.finalForm'); } }, // [Character Panel Menu] OpenType › Positional Forms › Final Form
  'id.panel.character.opentype.positionalForms.isolatedForm': { get label() { return t('command.id.panel.character.opentype.positionalForms.isolatedForm'); } }, // [Character Panel Menu] OpenType › Positional Forms › Isolated Form
  'id.panel.character.opentype.superscriptSuperior': { get label() { return t('command.id.panel.character.opentype.superscriptSuperior'); } }, // [Character Panel Menu] OpenType › Superscript/Superior
  'id.panel.character.opentype.subscriptInferior': { get label() { return t('command.id.panel.character.opentype.subscriptInferior'); } }, // [Character Panel Menu] OpenType › Subscript/Inferior
  'id.panel.character.opentype.numerator': { get label() { return t('command.id.panel.character.opentype.numerator'); } }, // [Character Panel Menu] OpenType › Numerator
  'id.panel.character.opentype.denominator': { get label() { return t('command.id.panel.character.opentype.denominator'); } }, // [Character Panel Menu] OpenType › Denominator
  'id.panel.character.opentype.tabularLining': { get label() { return t('command.id.panel.character.opentype.tabularLining'); } }, // [Character Panel Menu] OpenType › Tabular Lining
  'id.panel.character.opentype.proportionalOldstyle': { get label() { return t('command.id.panel.character.opentype.proportionalOldstyle'); } }, // [Character Panel Menu] OpenType › Proportional Oldstyle
  'id.panel.character.opentype.proportionalLining': { get label() { return t('command.id.panel.character.opentype.proportionalLining'); } }, // [Character Panel Menu] OpenType › Proportional Lining
  'id.panel.character.opentype.tabularOldstyle': { get label() { return t('command.id.panel.character.opentype.tabularOldstyle'); } }, // [Character Panel Menu] OpenType › Tabular Oldstyle
  'id.panel.character.opentype.defaultFigureStyle': { get label() { return t('command.id.panel.character.opentype.defaultFigureStyle'); } }, // [Character Panel Menu] OpenType › Default Figure Style
  'id.panel.character.allCaps': { get label() { return t('command.id.panel.character.allCaps'); }, keys: ['Ctrl+Shift+K'] }, // [Character Panel Menu] All Caps
  'id.panel.character.smallCaps': { get label() { return t('command.id.panel.character.smallCaps'); }, keys: ['Ctrl+Shift+H'] }, // [Character Panel Menu] Small Caps
  'id.panel.character.lowercase': { get label() { return t('command.id.panel.character.lowercase'); } }, // [Character Panel Menu] Lowercase
  'id.panel.character.superscript': { get label() { return t('command.id.panel.character.superscript'); }, keys: ['Ctrl+Shift+='] }, // [Character Panel Menu] Superscript
  'id.panel.character.subscript': { get label() { return t('command.id.panel.character.subscript'); }, keys: ['Ctrl+Alt+Shift+='] }, // [Character Panel Menu] Subscript
  'id.panel.character.underline': { get label() { return t('command.id.panel.character.underline'); }, keys: ['Ctrl+Shift+U'] }, // [Character Panel Menu] Underline
  'id.panel.character.strikethrough': { get label() { return t('command.id.panel.character.strikethrough'); }, keys: ['Ctrl+Shift+/'] }, // [Character Panel Menu] Strikethrough
  'id.panel.character.ligatures': { get label() { return t('command.id.panel.character.ligatures'); } }, // [Character Panel Menu] Ligatures
  'id.panel.character.underlineOptions': { get label() { return t('command.id.panel.character.underlineOptions'); } }, // [Character Panel Menu] Underline Options....
  'id.panel.character.strikethroughOptions': { get label() { return t('command.id.panel.character.strikethroughOptions'); } }, // [Character Panel Menu] Strikethrough Options...
  'id.panel.character.noBreak': { get label() { return t('command.id.panel.character.noBreak'); } }, // [Character Panel Menu] No Break
  'id.panel.paragraph.showOptions': { get label() { return t('command.id.panel.paragraph.showOptions'); } }, // [Paragraph Panel Menu] Show Options
  'id.panel.paragraph.worldReadySingleLineComposer': { get label() { return t('command.id.panel.paragraph.worldReadySingleLineComposer'); } }, // [Paragraph Panel Menu] World-Ready Single-line Composer
  'id.panel.paragraph.worldReadyParagraphComposer': { get label() { return t('command.id.panel.paragraph.worldReadyParagraphComposer'); } }, // [Paragraph Panel Menu] World-Ready Paragraph Composer
  'id.panel.paragraph.paragraphComposer': { get label() { return t('command.id.panel.paragraph.paragraphComposer'); } }, // [Paragraph Panel Menu] Paragraph Composer
  'id.panel.paragraph.singleLineComposer': { get label() { return t('command.id.panel.paragraph.singleLineComposer'); } }, // [Paragraph Panel Menu] Single-line Composer
  'id.panel.paragraph.onlyAlignFirstLineToGrid': { get label() { return t('command.id.panel.paragraph.onlyAlignFirstLineToGrid'); } }, // [Paragraph Panel Menu] Only Align First Line to Grid
  'id.panel.paragraph.balanceRaggedLines': { get label() { return t('command.id.panel.paragraph.balanceRaggedLines'); } }, // [Paragraph Panel Menu] Balance Ragged Lines
  'id.panel.paragraph.ignoreOpticalMargin': { get label() { return t('command.id.panel.paragraph.ignoreOpticalMargin'); } }, // [Paragraph Panel Menu] Ignore Optical Margin
  'id.panel.paragraph.justification': { get label() { return t('command.id.panel.paragraph.justification'); }, keys: ['Ctrl+Alt+Shift+J'] }, // [Paragraph Panel Menu] Justification...
  'id.panel.paragraph.keepOptions': { get label() { return t('command.id.panel.paragraph.keepOptions'); }, keys: ['Ctrl+Alt+K'] }, // [Paragraph Panel Menu] Keep Options...
  'id.panel.paragraph.spanColumns': { get label() { return t('command.id.panel.paragraph.spanColumns'); } }, // [Paragraph Panel Menu] Span Columns...
  'id.panel.paragraph.hyphenation': { get label() { return t('command.id.panel.paragraph.hyphenation'); } }, // [Paragraph Panel Menu] Hyphenation...
  'id.panel.paragraph.dropCapsAndNestedStyles': { get label() { return t('command.id.panel.paragraph.dropCapsAndNestedStyles'); }, keys: ['Ctrl+Alt+R'] }, // [Paragraph Panel Menu] Drop Caps and Nested Styles...
  'id.panel.paragraph.grepStyles': { get label() { return t('command.id.panel.paragraph.grepStyles'); } }, // [Paragraph Panel Menu] GREP Styles...
  'id.panel.paragraph.paragraphRules': { get label() { return t('command.id.panel.paragraph.paragraphRules'); }, keys: ['Ctrl+Alt+J'] }, // [Paragraph Panel Menu] Paragraph Rules...
  'id.panel.paragraph.paragraphBordersAndShading': { get label() { return t('command.id.panel.paragraph.paragraphBordersAndShading'); } }, // [Paragraph Panel Menu] Paragraph Borders and Shading...
  'id.panel.paragraph.bulletsAndNumbering': { get label() { return t('command.id.panel.paragraph.bulletsAndNumbering'); } }, // [Paragraph Panel Menu] Bullets and Numbering...
  'id.panel.paragraphStyles.newParagraphStyle': { get label() { return t('command.id.panel.paragraphStyles.newParagraphStyle'); } }, // [Paragraph Style Panel Menu] New Paragraph Style...
  'id.panel.paragraphStyles.duplicateStyle': { get label() { return t('command.id.panel.paragraphStyles.duplicateStyle'); } }, // [Paragraph Style Panel Menu] Duplicate Style...
  'id.panel.paragraphStyles.deleteStyles': { get label() { return t('command.id.panel.paragraphStyles.deleteStyles'); } }, // [Paragraph Style Panel Menu] Delete Styles
  'id.panel.paragraphStyles.redefineStyle': { get label() { return t('command.id.panel.paragraphStyles.redefineStyle'); } }, // [Paragraph Style Panel Menu] Redefine Style
  'id.panel.paragraphStyles.styleOptions': { get label() { return t('command.id.panel.paragraphStyles.styleOptions'); } }, // [Paragraph Style Panel Menu] Style Options...
  'id.panel.paragraphStyles.clearOverrides': { get label() { return t('command.id.panel.paragraphStyles.clearOverrides'); } }, // [Paragraph Style Panel Menu] Clear Overrides
  'id.panel.paragraphStyles.toggleStyleOverrideHighlighter': { get label() { return t('command.id.panel.paragraphStyles.toggleStyleOverrideHighlighter'); } }, // [Paragraph Style Panel Menu] Toggle Style Override Highlighter
  'id.panel.paragraphStyles.convertStyleSBulletsAndNumberingToText': { get label() { return t('command.id.panel.paragraphStyles.convertStyleSBulletsAndNumberingToText'); } }, // [Paragraph Style Panel Menu] Convert Style's Bullets and Numbering to Text
  'id.panel.paragraphStyles.breakLinkToStyle': { get label() { return t('command.id.panel.paragraphStyles.breakLinkToStyle'); } }, // [Paragraph Style Panel Menu] Break Link to Style
  'id.panel.paragraphStyles.loadParagraphStyles': { get label() { return t('command.id.panel.paragraphStyles.loadParagraphStyles'); } }, // [Paragraph Style Panel Menu] Load Paragraph Styles...
  'id.panel.paragraphStyles.loadAllTextStyles': { get label() { return t('command.id.panel.paragraphStyles.loadAllTextStyles'); } }, // [Paragraph Style Panel Menu] Load All Text Styles...
  'id.panel.paragraphStyles.selectAllUnused': { get label() { return t('command.id.panel.paragraphStyles.selectAllUnused'); } }, // [Paragraph Style Panel Menu] Select All Unused
  'id.panel.paragraphStyles.newStyleGroup': { get label() { return t('command.id.panel.paragraphStyles.newStyleGroup'); } }, // [Paragraph Style Panel Menu] New Style Group...
  'id.panel.paragraphStyles.openAllStyleGroups': { get label() { return t('command.id.panel.paragraphStyles.openAllStyleGroups'); } }, // [Paragraph Style Panel Menu] Open All Style Groups
  'id.panel.paragraphStyles.closeAllStyleGroups': { get label() { return t('command.id.panel.paragraphStyles.closeAllStyleGroups'); } }, // [Paragraph Style Panel Menu] Close All Style Groups
  'id.panel.paragraphStyles.copyToGroup': { get label() { return t('command.id.panel.paragraphStyles.copyToGroup'); } }, // [Paragraph Style Panel Menu] Copy to Group...
  'id.panel.paragraphStyles.newGroupFromStyles': { get label() { return t('command.id.panel.paragraphStyles.newGroupFromStyles'); } }, // [Paragraph Style Panel Menu] New Group from Styles...
  'id.panel.paragraphStyles.sortByName': { get label() { return t('command.id.panel.paragraphStyles.sortByName'); } }, // [Paragraph Style Panel Menu] Sort by Name
  'id.panel.paragraphStyles.smallPanelRows': { get label() { return t('command.id.panel.paragraphStyles.smallPanelRows'); } }, // [Paragraph Style Panel Menu] Small Panel Rows
  'id.panel.characterStyles.newCharacterStyle': { get label() { return t('command.id.panel.characterStyles.newCharacterStyle'); } }, // [Character Style Panel Menu] New Character Style...
  'id.panel.characterStyles.duplicateStyle': { get label() { return t('command.id.panel.characterStyles.duplicateStyle'); } }, // [Character Style Panel Menu] Duplicate Style...
  'id.panel.characterStyles.deleteStyle': { get label() { return t('command.id.panel.characterStyles.deleteStyle'); } }, // [Character Style Panel Menu] Delete Style
  'id.panel.characterStyles.redefineStyle': { get label() { return t('command.id.panel.characterStyles.redefineStyle'); } }, // [Character Style Panel Menu] Redefine Style
  'id.panel.characterStyles.toggleStyleOverrideHighlighter': { get label() { return t('command.id.panel.characterStyles.toggleStyleOverrideHighlighter'); } }, // [Character Style Panel Menu] Toggle Style Override Highlighter
  'id.panel.characterStyles.styleOptions': { get label() { return t('command.id.panel.characterStyles.styleOptions'); } }, // [Character Style Panel Menu] Style Options...
  'id.panel.characterStyles.breakLinkToStyle': { get label() { return t('command.id.panel.characterStyles.breakLinkToStyle'); } }, // [Character Style Panel Menu] Break Link to Style
  'id.panel.characterStyles.loadCharacterStyles': { get label() { return t('command.id.panel.characterStyles.loadCharacterStyles'); } }, // [Character Style Panel Menu] Load Character Styles...
  'id.panel.characterStyles.selectAllUnused': { get label() { return t('command.id.panel.characterStyles.selectAllUnused'); } }, // [Character Style Panel Menu] Select All Unused
  'id.panel.characterStyles.newStyleGroup': { get label() { return t('command.id.panel.characterStyles.newStyleGroup'); } }, // [Character Style Panel Menu] New Style Group...
  'id.panel.characterStyles.openAllStyleGroups': { get label() { return t('command.id.panel.characterStyles.openAllStyleGroups'); } }, // [Character Style Panel Menu] Open All Style Groups
  'id.panel.characterStyles.closeAllStyleGroups': { get label() { return t('command.id.panel.characterStyles.closeAllStyleGroups'); } }, // [Character Style Panel Menu] Close All Style Groups
  'id.panel.characterStyles.copyToGroup': { get label() { return t('command.id.panel.characterStyles.copyToGroup'); } }, // [Character Style Panel Menu] Copy to Group...
  'id.panel.characterStyles.newGroupFromStyles': { get label() { return t('command.id.panel.characterStyles.newGroupFromStyles'); } }, // [Character Style Panel Menu] New Group from Styles...
  'id.panel.characterStyles.sortByName': { get label() { return t('command.id.panel.characterStyles.sortByName'); } }, // [Character Style Panel Menu] Sort by Name
  'id.panel.characterStyles.smallPanelRows': { get label() { return t('command.id.panel.characterStyles.smallPanelRows'); } }, // [Character Style Panel Menu] Small Panel Rows
  'id.panel.objectStyles.newObjectStyle': { get label() { return t('command.id.panel.objectStyles.newObjectStyle'); } }, // [Object Styles Panel Menu] New Object Style...
  'id.panel.objectStyles.duplicateObjectStyle': { get label() { return t('command.id.panel.objectStyles.duplicateObjectStyle'); } }, // [Object Styles Panel Menu] Duplicate Object Style...
  'id.panel.objectStyles.deleteStyles': { get label() { return t('command.id.panel.objectStyles.deleteStyles'); } }, // [Object Styles Panel Menu] Delete Styles...
  'id.panel.objectStyles.redefineStyle': { get label() { return t('command.id.panel.objectStyles.redefineStyle'); } }, // [Object Styles Panel Menu] Redefine Style
  'id.panel.objectStyles.styleOptions': { get label() { return t('command.id.panel.objectStyles.styleOptions'); } }, // [Object Styles Panel Menu] Style Options...
  'id.panel.objectStyles.clearOverrides': { get label() { return t('command.id.panel.objectStyles.clearOverrides'); } }, // [Object Styles Panel Menu] Clear Overrides
  'id.panel.objectStyles.clearOverridesWhenApplyingStyle': { get label() { return t('command.id.panel.objectStyles.clearOverridesWhenApplyingStyle'); } }, // [Object Styles Panel Menu] Clear Overrides When Applying Style
  'id.panel.objectStyles.clearAttributesNotDefinedByStyle': { get label() { return t('command.id.panel.objectStyles.clearAttributesNotDefinedByStyle'); } }, // [Object Styles Panel Menu] Clear Attributes Not Defined by Style
  'id.panel.objectStyles.breakLinkToStyle': { get label() { return t('command.id.panel.objectStyles.breakLinkToStyle'); } }, // [Object Styles Panel Menu] Break Link to Style
  'id.panel.objectStyles.defaultTextFrameStyle.none': { get label() { return t('command.id.panel.objectStyles.defaultTextFrameStyle.none'); } }, // [Object Styles Panel Menu] Default Text Frame Style › [None]
  'id.panel.objectStyles.defaultTextFrameStyle.basicGraphicsFrame': { get label() { return t('command.id.panel.objectStyles.defaultTextFrameStyle.basicGraphicsFrame'); } }, // [Object Styles Panel Menu] Default Text Frame Style › [Basic Graphics Frame]
  'id.panel.objectStyles.defaultTextFrameStyle.basicTextFrame': { get label() { return t('command.id.panel.objectStyles.defaultTextFrameStyle.basicTextFrame'); } }, // [Object Styles Panel Menu] Default Text Frame Style › [Basic Text Frame]
  'id.panel.objectStyles.defaultGraphicFrameStyle.none': { get label() { return t('command.id.panel.objectStyles.defaultGraphicFrameStyle.none'); } }, // [Object Styles Panel Menu] Default Graphic Frame Style › [None]
  'id.panel.objectStyles.defaultGraphicFrameStyle.basicGraphicsFrame': { get label() { return t('command.id.panel.objectStyles.defaultGraphicFrameStyle.basicGraphicsFrame'); } }, // [Object Styles Panel Menu] Default Graphic Frame Style › [Basic Graphics Frame]
  'id.panel.objectStyles.defaultGraphicFrameStyle.basicTextFrame': { get label() { return t('command.id.panel.objectStyles.defaultGraphicFrameStyle.basicTextFrame'); } }, // [Object Styles Panel Menu] Default Graphic Frame Style › [Basic Text Frame]
  'id.panel.objectStyles.loadObjectStyles': { get label() { return t('command.id.panel.objectStyles.loadObjectStyles'); } }, // [Object Styles Panel Menu] Load Object Styles...
  'id.panel.objectStyles.selectAllUnused': { get label() { return t('command.id.panel.objectStyles.selectAllUnused'); } }, // [Object Styles Panel Menu] Select All Unused
  'id.panel.objectStyles.newStyleGroup': { get label() { return t('command.id.panel.objectStyles.newStyleGroup'); } }, // [Object Styles Panel Menu] New Style Group...
  'id.panel.objectStyles.openAllGroups': { get label() { return t('command.id.panel.objectStyles.openAllGroups'); } }, // [Object Styles Panel Menu] Open All Groups
  'id.panel.objectStyles.closeAllGroups': { get label() { return t('command.id.panel.objectStyles.closeAllGroups'); } }, // [Object Styles Panel Menu] Close All Groups
  'id.panel.objectStyles.copyToGroup': { get label() { return t('command.id.panel.objectStyles.copyToGroup'); } }, // [Object Styles Panel Menu] Copy to Group...
  'id.panel.objectStyles.newGroupFromStyles': { get label() { return t('command.id.panel.objectStyles.newGroupFromStyles'); } }, // [Object Styles Panel Menu] New Group from Styles...
  'id.panel.objectStyles.sortByName': { get label() { return t('command.id.panel.objectStyles.sortByName'); } }, // [Object Styles Panel Menu] Sort by Name
  'id.panel.objectStyles.smallPanelRows': { get label() { return t('command.id.panel.objectStyles.smallPanelRows'); } }, // [Object Styles Panel Menu] Small Panel Rows
  'id.panel.align.showOptions': { get label() { return t('command.id.panel.align.showOptions'); } }, // [Align Panel Menu] Show Options
  'id.panel.align.cancelKeyObject': { get label() { return t('command.id.panel.align.cancelKeyObject'); } }, // [Align Panel Menu] Cancel Key Object
  'id.panel.transform.redefineScalingAs100': { get label() { return t('command.id.panel.transform.redefineScalingAs100'); } }, // [Transform Panel Menu] Redefine Scaling as 100%
  'id.panel.transform.dimensionsIncludeStrokeWeight': { get label() { return t('command.id.panel.transform.dimensionsIncludeStrokeWeight'); } }, // [Transform Panel Menu] Dimensions Include Stroke Weight
  'id.panel.transform.transformationsAreTotals': { get label() { return t('command.id.panel.transform.transformationsAreTotals'); } }, // [Transform Panel Menu] Transformations are Totals
  'id.panel.transform.adjustStrokeWeightWhenScaling': { get label() { return t('command.id.panel.transform.adjustStrokeWeightWhenScaling'); } }, // [Transform Panel Menu] Adjust Stroke Weight when Scaling
  'id.panel.transform.adjustEffectsWhenScaling': { get label() { return t('command.id.panel.transform.adjustEffectsWhenScaling'); } }, // [Transform Panel Menu] Adjust Effects when Scaling
  'id.panel.textWrap.showOptions': { get label() { return t('command.id.panel.textWrap.showOptions'); } }, // [Text Wrap Panel Menu] Show Options
  'id.panel.textWrap.applyToParentPageOnly': { get label() { return t('command.id.panel.textWrap.applyToParentPageOnly'); } }, // [Text Wrap Panel Menu] Apply to Parent Page Only
  'id.panel.effects.showOptions': { get label() { return t('command.id.panel.effects.showOptions'); } }, // [Effects Panel Menu] Show Options
  'id.panel.preflight.defineProfiles': { get label() { return t('command.id.panel.preflight.defineProfiles'); } }, // [Preflight Panel Menu] Define Profiles...
  'id.panel.preflight.preflightOptions': { get label() { return t('command.id.panel.preflight.preflightOptions'); } }, // [Preflight Panel Menu] Preflight Options...
  'id.panel.preflight.saveReport': { get label() { return t('command.id.panel.preflight.saveReport'); } }, // [Preflight Panel Menu] Save Report...
  'id.panel.preflight.limitNumberOfRowsPerError.25': { get label() { return t('command.id.panel.preflight.limitNumberOfRowsPerError.25'); } }, // [Preflight Panel Menu] Limit Number of Rows Per Error › 25
  'id.panel.preflight.limitNumberOfRowsPerError.50': { get label() { return t('command.id.panel.preflight.limitNumberOfRowsPerError.50'); } }, // [Preflight Panel Menu] Limit Number of Rows Per Error › 50
  'id.panel.preflight.limitNumberOfRowsPerError.100': { get label() { return t('command.id.panel.preflight.limitNumberOfRowsPerError.100'); } }, // [Preflight Panel Menu] Limit Number of Rows Per Error › 100
  'id.panel.preflight.limitNumberOfRowsPerError.noLimit': { get label() { return t('command.id.panel.preflight.limitNumberOfRowsPerError.noLimit'); } }, // [Preflight Panel Menu] Limit Number of Rows Per Error › No Limit
  'id.panel.preflight.enablePreflightForAllDocuments': { get label() { return t('command.id.panel.preflight.enablePreflightForAllDocuments'); } }, // [Preflight Panel Menu] Enable Preflight for All Documents
  'id.panel.separations.inkManager': { get label() { return t('command.id.panel.separations.inkManager'); } }, // [Separations Preview Panel Menu] Ink Manager...
  'id.panel.separations.showSinglePlatesInBlack': { get label() { return t('command.id.panel.separations.showSinglePlatesInBlack'); } }, // [Separations Preview Panel Menu] Show Single Plates in Black
  'id.panel.separations.desaturateBlack': { get label() { return t('command.id.panel.separations.desaturateBlack'); } }, // [Separations Preview Panel Menu] Desaturate Black
  'id.panel.tableStyles.newTableStyle': { get label() { return t('command.id.panel.tableStyles.newTableStyle'); } }, // [Table Styles Panel Menu] New Table Style...
  'id.panel.tableStyles.duplicateStyle': { get label() { return t('command.id.panel.tableStyles.duplicateStyle'); } }, // [Table Styles Panel Menu] Duplicate Style...
  'id.panel.tableStyles.deleteStyle': { get label() { return t('command.id.panel.tableStyles.deleteStyle'); } }, // [Table Styles Panel Menu] Delete Style
  'id.panel.tableStyles.redefineStyle': { get label() { return t('command.id.panel.tableStyles.redefineStyle'); } }, // [Table Styles Panel Menu] Redefine Style
  'id.panel.tableStyles.styleOptions': { get label() { return t('command.id.panel.tableStyles.styleOptions'); } }, // [Table Styles Panel Menu] Style Options...
  'id.panel.tableStyles.clearOverrides': { get label() { return t('command.id.panel.tableStyles.clearOverrides'); } }, // [Table Styles Panel Menu] Clear Overrides
  'id.panel.tableStyles.breakLinkToStyle': { get label() { return t('command.id.panel.tableStyles.breakLinkToStyle'); } }, // [Table Styles Panel Menu] Break Link to Style
  'id.panel.tableStyles.loadTableStyles': { get label() { return t('command.id.panel.tableStyles.loadTableStyles'); } }, // [Table Styles Panel Menu] Load Table Styles...
  'id.panel.tableStyles.loadTableAndCellStyles': { get label() { return t('command.id.panel.tableStyles.loadTableAndCellStyles'); } }, // [Table Styles Panel Menu] Load Table and Cell Styles...
  'id.panel.tableStyles.selectAllUnused': { get label() { return t('command.id.panel.tableStyles.selectAllUnused'); } }, // [Table Styles Panel Menu] Select All Unused
  'id.panel.tableStyles.newStyleGroup': { get label() { return t('command.id.panel.tableStyles.newStyleGroup'); } }, // [Table Styles Panel Menu] New Style Group...
  'id.panel.tableStyles.openAllStyleGroups': { get label() { return t('command.id.panel.tableStyles.openAllStyleGroups'); } }, // [Table Styles Panel Menu] Open All Style Groups
  'id.panel.tableStyles.closeAllStyleGroups': { get label() { return t('command.id.panel.tableStyles.closeAllStyleGroups'); } }, // [Table Styles Panel Menu] Close All Style Groups
  'id.panel.tableStyles.copyToGroup': { get label() { return t('command.id.panel.tableStyles.copyToGroup'); } }, // [Table Styles Panel Menu] Copy to Group...
  'id.panel.tableStyles.newGroupFromStyles': { get label() { return t('command.id.panel.tableStyles.newGroupFromStyles'); } }, // [Table Styles Panel Menu] New Group from Styles...
  'id.panel.tableStyles.sortByName': { get label() { return t('command.id.panel.tableStyles.sortByName'); } }, // [Table Styles Panel Menu] Sort by Name
  'id.panel.tableStyles.smallPanelRows': { get label() { return t('command.id.panel.tableStyles.smallPanelRows'); } }, // [Table Styles Panel Menu] Small Panel Rows
  'id.panel.cellStyles.newCellStyle': { get label() { return t('command.id.panel.cellStyles.newCellStyle'); } }, // [Cell Styles Panel Menu] New Cell Style...
  'id.panel.cellStyles.duplicateStyle': { get label() { return t('command.id.panel.cellStyles.duplicateStyle'); } }, // [Cell Styles Panel Menu] Duplicate Style...
  'id.panel.cellStyles.deleteStyle': { get label() { return t('command.id.panel.cellStyles.deleteStyle'); } }, // [Cell Styles Panel Menu] Delete Style
  'id.panel.cellStyles.redefineStyle': { get label() { return t('command.id.panel.cellStyles.redefineStyle'); } }, // [Cell Styles Panel Menu] Redefine Style
  'id.panel.cellStyles.styleOptions': { get label() { return t('command.id.panel.cellStyles.styleOptions'); } }, // [Cell Styles Panel Menu] Style Options...
  'id.panel.cellStyles.clearOverrides': { get label() { return t('command.id.panel.cellStyles.clearOverrides'); } }, // [Cell Styles Panel Menu] Clear Overrides
  'id.panel.cellStyles.clearAttributesNotDefinedByStyle': { get label() { return t('command.id.panel.cellStyles.clearAttributesNotDefinedByStyle'); } }, // [Cell Styles Panel Menu] Clear Attributes Not Defined by Style
  'id.panel.cellStyles.breakLinkToStyle': { get label() { return t('command.id.panel.cellStyles.breakLinkToStyle'); } }, // [Cell Styles Panel Menu] Break Link to Style
  'id.panel.cellStyles.loadCellStyles': { get label() { return t('command.id.panel.cellStyles.loadCellStyles'); } }, // [Cell Styles Panel Menu] Load Cell Styles...
  'id.panel.cellStyles.loadTableAndCellStyles': { get label() { return t('command.id.panel.cellStyles.loadTableAndCellStyles'); } }, // [Cell Styles Panel Menu] Load Table and Cell Styles...
  'id.panel.cellStyles.selectAllUnused': { get label() { return t('command.id.panel.cellStyles.selectAllUnused'); } }, // [Cell Styles Panel Menu] Select All Unused
  'id.panel.cellStyles.newStyleGroup': { get label() { return t('command.id.panel.cellStyles.newStyleGroup'); } }, // [Cell Styles Panel Menu] New Style Group...
  'id.panel.cellStyles.openAllStyleGroups': { get label() { return t('command.id.panel.cellStyles.openAllStyleGroups'); } }, // [Cell Styles Panel Menu] Open All Style Groups
  'id.panel.cellStyles.closeAllStyleGroups': { get label() { return t('command.id.panel.cellStyles.closeAllStyleGroups'); } }, // [Cell Styles Panel Menu] Close All Style Groups
  'id.panel.cellStyles.copyToGroup': { get label() { return t('command.id.panel.cellStyles.copyToGroup'); } }, // [Cell Styles Panel Menu] Copy to Group...
  'id.panel.cellStyles.newGroupFromStyles': { get label() { return t('command.id.panel.cellStyles.newGroupFromStyles'); } }, // [Cell Styles Panel Menu] New Group from Styles...
  'id.panel.cellStyles.sortByName': { get label() { return t('command.id.panel.cellStyles.sortByName'); } }, // [Cell Styles Panel Menu] Sort by Name
  'id.panel.cellStyles.smallPanelRows': { get label() { return t('command.id.panel.cellStyles.smallPanelRows'); } }, // [Cell Styles Panel Menu] Small Panel Rows
  'id.panel.hyperlinks.deleteHyperlink': { get label() { return t('command.id.panel.hyperlinks.deleteHyperlink'); } }, // [Hyperlinks Panel Menu] Delete Hyperlink
  'id.panel.hyperlinks.newHyperlinkDestination': { get label() { return t('command.id.panel.hyperlinks.newHyperlinkDestination'); } }, // [Hyperlinks Panel Menu] New Hyperlink Destination...
  'id.panel.hyperlinks.hyperlinkDestinationOptions': { get label() { return t('command.id.panel.hyperlinks.hyperlinkDestinationOptions'); } }, // [Hyperlinks Panel Menu] Hyperlink Destination Options...
  'id.panel.hyperlinks.deleteUnusedDestinations': { get label() { return t('command.id.panel.hyperlinks.deleteUnusedDestinations'); } }, // [Hyperlinks Panel Menu] Delete Unused Destinations
  'id.panel.hyperlinks.goToSource': { get label() { return t('command.id.panel.hyperlinks.goToSource'); } }, // [Hyperlinks Panel Menu] Go To Source
  'id.panel.hyperlinks.goToDestination': { get label() { return t('command.id.panel.hyperlinks.goToDestination'); } }, // [Hyperlinks Panel Menu] Go To Destination
  'id.panel.hyperlinks.renameHyperlink': { get label() { return t('command.id.panel.hyperlinks.renameHyperlink'); } }, // [Hyperlinks Panel Menu] Rename Hyperlink...
  'id.panel.hyperlinks.resetHyperlink': { get label() { return t('command.id.panel.hyperlinks.resetHyperlink'); } }, // [Hyperlinks Panel Menu] Reset Hyperlink
  'id.panel.hyperlinks.sort.manually': { get label() { return t('command.id.panel.hyperlinks.sort.manually'); } }, // [Hyperlinks Panel Menu] Sort › Manually
  'id.panel.hyperlinks.sort.byName': { get label() { return t('command.id.panel.hyperlinks.sort.byName'); } }, // [Hyperlinks Panel Menu] Sort › By Name
  'id.panel.hyperlinks.sort.byType': { get label() { return t('command.id.panel.hyperlinks.sort.byType'); } }, // [Hyperlinks Panel Menu] Sort › By Type
  'id.panel.crossReferences.deleteCrossReference': { get label() { return t('command.id.panel.crossReferences.deleteCrossReference'); } }, // [Cross-Reference Panel Menu] Delete Cross-Reference
  'id.panel.crossReferences.relinkCrossReference': { get label() { return t('command.id.panel.crossReferences.relinkCrossReference'); } }, // [Cross-Reference Panel Menu] Relink Cross-Reference...
  'id.panel.crossReferences.defineCrossReferenceFormats': { get label() { return t('command.id.panel.crossReferences.defineCrossReferenceFormats'); } }, // [Cross-Reference Panel Menu] Define Cross-Reference Formats...
  'id.panel.glyphs.newGlyphSet': { get label() { return t('command.id.panel.glyphs.newGlyphSet'); } }, // [Glyphs Panel Menu] New Glyph Set...
  'id.panel.glyphs.sortGlyphs.byCidGid': { get label() { return t('command.id.panel.glyphs.sortGlyphs.byCidGid'); } }, // [Glyphs Panel Menu] Sort Glyphs › By CID / GID
  'id.panel.glyphs.sortGlyphs.byUnicode': { get label() { return t('command.id.panel.glyphs.sortGlyphs.byUnicode'); } }, // [Glyphs Panel Menu] Sort Glyphs › By Unicode
  'id.panel.index.new': { get label() { return t('command.id.panel.index.new'); }, keys: ['Ctrl+7'] }, // [Index Panel Menu] New...
  'id.panel.index.duplicate': { get label() { return t('command.id.panel.index.duplicate'); } }, // [Index Panel Menu] Duplicate...
  'id.panel.index.delete': { get label() { return t('command.id.panel.index.delete'); } }, // [Index Panel Menu] Delete
  'id.panel.index.importTopics': { get label() { return t('command.id.panel.index.importTopics'); } }, // [Index Panel Menu] Import Topics...
  'id.panel.index.removeUnusedTopics': { get label() { return t('command.id.panel.index.removeUnusedTopics'); } }, // [Index Panel Menu] Remove Unused Topics
  'id.panel.index.goToSelectedMarker': { get label() { return t('command.id.panel.index.goToSelectedMarker'); } }, // [Index Panel Menu] Go to Selected Marker
  'id.panel.index.options': { get label() { return t('command.id.panel.index.options'); } }, // [Index Panel Menu] Options...
  'id.panel.index.capitalize': { get label() { return t('command.id.panel.index.capitalize'); } }, // [Index Panel Menu] Capitalize...
  'id.panel.index.sortOptions': { get label() { return t('command.id.panel.index.sortOptions'); } }, // [Index Panel Menu] Sort Options...
  'id.panel.index.updatePreview': { get label() { return t('command.id.panel.index.updatePreview'); } }, // [Index Panel Menu] Update Preview
  'id.panel.index.generateIndex': { get label() { return t('command.id.panel.index.generateIndex'); } }, // [Index Panel Menu] Generate Index...
  'id.panel.index.showFindField': { get label() { return t('command.id.panel.index.showFindField'); } }, // [Index Panel Menu] Show Find Field
  'id.panel.index.showUnusedTopics': { get label() { return t('command.id.panel.index.showUnusedTopics'); } }, // [Index Panel Menu] Show Unused Topics
  'id.panel.conditionalText.newCondition': { get label() { return t('command.id.panel.conditionalText.newCondition'); } }, // [Conditional Text Panel Menu] New Condition...
  'id.panel.conditionalText.deleteCondition': { get label() { return t('command.id.panel.conditionalText.deleteCondition'); } }, // [Conditional Text Panel Menu] Delete Condition...
  'id.panel.conditionalText.conditionOptions': { get label() { return t('command.id.panel.conditionalText.conditionOptions'); } }, // [Conditional Text Panel Menu] Condition Options...
  'id.panel.conditionalText.showAll': { get label() { return t('command.id.panel.conditionalText.showAll'); } }, // [Conditional Text Panel Menu] Show All
  'id.panel.conditionalText.hideAll': { get label() { return t('command.id.panel.conditionalText.hideAll'); } }, // [Conditional Text Panel Menu] Hide All
  'id.panel.conditionalText.loadConditions': { get label() { return t('command.id.panel.conditionalText.loadConditions'); } }, // [Conditional Text Panel Menu] Load Conditions...
  'id.panel.conditionalText.loadConditionsAndSets': { get label() { return t('command.id.panel.conditionalText.loadConditionsAndSets'); } }, // [Conditional Text Panel Menu] Load Conditions and Sets...
  'id.panel.conditionalText.selectAllUnused': { get label() { return t('command.id.panel.conditionalText.selectAllUnused'); } }, // [Conditional Text Panel Menu] Select All Unused
  'id.panel.dataMerge.selectDataSource': { get label() { return t('command.id.panel.dataMerge.selectDataSource'); } }, // [Data Merge Panel Menu] Select Data Source...
  'id.panel.dataMerge.exportToPdf': { get label() { return t('command.id.panel.dataMerge.exportToPdf'); } }, // [Data Merge Panel Menu] Export To PDF
  'id.panel.dataMerge.updateDataSource': { get label() { return t('command.id.panel.dataMerge.updateDataSource'); } }, // [Data Merge Panel Menu] Update Data Source
  'id.panel.dataMerge.removeDataSource': { get label() { return t('command.id.panel.dataMerge.removeDataSource'); } }, // [Data Merge Panel Menu] Remove Data Source
  'id.panel.dataMerge.preview': { get label() { return t('command.id.panel.dataMerge.preview'); } }, // [Data Merge Panel Menu] Preview
  'id.panel.dataMerge.createMergedDocument': { get label() { return t('command.id.panel.dataMerge.createMergedDocument'); } }, // [Data Merge Panel Menu] Create Merged Document...
  'id.panel.dataMerge.updateContentInDataFields': { get label() { return t('command.id.panel.dataMerge.updateContentInDataFields'); } }, // [Data Merge Panel Menu] Update Content in Data Fields
  'id.panel.dataMerge.contentPlacementOptions': { get label() { return t('command.id.panel.dataMerge.contentPlacementOptions'); } }, // [Data Merge Panel Menu] Content Placement Options...
  'id.panel.dataMerge.showLogOfUpdateResults': { get label() { return t('command.id.panel.dataMerge.showLogOfUpdateResults'); } }, // [Data Merge Panel Menu] Show Log of Update Results
  'id.panel.trapPresets.newPreset': { get label() { return t('command.id.panel.trapPresets.newPreset'); } }, // [Trap Presets Panel Menu] New Preset...
  'id.panel.trapPresets.duplicatePreset': { get label() { return t('command.id.panel.trapPresets.duplicatePreset'); } }, // [Trap Presets Panel Menu] Duplicate Preset...
  'id.panel.trapPresets.deletePresets': { get label() { return t('command.id.panel.trapPresets.deletePresets'); } }, // [Trap Presets Panel Menu] Delete Presets
  'id.panel.trapPresets.presetOptions': { get label() { return t('command.id.panel.trapPresets.presetOptions'); } }, // [Trap Presets Panel Menu] Preset Options...
  'id.panel.trapPresets.assignTrapPreset': { get label() { return t('command.id.panel.trapPresets.assignTrapPreset'); } }, // [Trap Presets Panel Menu] Assign Trap Preset...
  'id.panel.trapPresets.loadTrapPresets': { get label() { return t('command.id.panel.trapPresets.loadTrapPresets'); } }, // [Trap Presets Panel Menu] Load Trap Presets...
  'id.panel.trapPresets.selectAllUnused': { get label() { return t('command.id.panel.trapPresets.selectAllUnused'); } }, // [Trap Presets Panel Menu] Select All Unused
  'id.panel.trapPresets.smallPanelRows': { get label() { return t('command.id.panel.trapPresets.smallPanelRows'); } }, // [Trap Presets Panel Menu] Small Panel Rows
  'id.panel.history.delete': { get label() { return t('command.id.panel.history.delete'); } }, // [History Panel Menu] Delete
  'id.panel.history.clearHistory': { get label() { return t('command.id.panel.history.clearHistory'); } }, // [History Panel Menu] Clear History
  'id.panel.history.newDocument': { get label() { return t('command.id.panel.history.newDocument'); } }, // [History Panel Menu] New Document
  'id.panel.book.addDocument': { get label() { return t('command.id.panel.book.addDocument'); } }, // [Book Panel Menu] Add Document...
  'id.panel.book.replaceDocument': { get label() { return t('command.id.panel.book.replaceDocument'); } }, // [Book Panel Menu] Replace Document...
  'id.panel.book.removeDocument': { get label() { return t('command.id.panel.book.removeDocument'); } }, // [Book Panel Menu] Remove Document
  'id.panel.book.saveBook': { get label() { return t('command.id.panel.book.saveBook'); } }, // [Book Panel Menu] Save Book
  'id.panel.book.saveBookAs': { get label() { return t('command.id.panel.book.saveBookAs'); } }, // [Book Panel Menu] Save Book As...
  'id.panel.book.closeBook': { get label() { return t('command.id.panel.book.closeBook'); } }, // [Book Panel Menu] Close Book
  'id.panel.book.preflightBook': { get label() { return t('command.id.panel.book.preflightBook'); } }, // [Book Panel Menu] Preflight Book...
  'id.panel.book.packageBookForPrint': { get label() { return t('command.id.panel.book.packageBookForPrint'); } }, // [Book Panel Menu] Package Book For Print...
  'id.panel.book.exportBookToPdf': { get label() { return t('command.id.panel.book.exportBookToPdf'); } }, // [Book Panel Menu] Export Book to PDF...
  'id.panel.book.printBook': { get label() { return t('command.id.panel.book.printBook'); } }, // [Book Panel Menu] Print Book...
  'id.panel.book.documentInformation': { get label() { return t('command.id.panel.book.documentInformation'); } }, // [Book Panel Menu] Document Information...
  'id.panel.book.revealInExplorer': { get label() { return t('command.id.panel.book.revealInExplorer'); } }, // [Book Panel Menu] Reveal in Explorer
  'id.panel.book.synchronizeBook': { get label() { return t('command.id.panel.book.synchronizeBook'); } }, // [Book Panel Menu] Synchronize Book
  'id.panel.book.synchronizeOptions': { get label() { return t('command.id.panel.book.synchronizeOptions'); } }, // [Book Panel Menu] Synchronize Options...
  'id.panel.book.updateAllCrossReferences': { get label() { return t('command.id.panel.book.updateAllCrossReferences'); } }, // [Book Panel Menu] Update All Cross-References
  'id.panel.book.updateNumbering.updatePageAndSectionNumbers': { get label() { return t('command.id.panel.book.updateNumbering.updatePageAndSectionNumbers'); } }, // [Book Panel Menu] Update Numbering › Update Page & Section Numbers
  'id.panel.book.updateNumbering.updateChapterAndParagraphNumbers': { get label() { return t('command.id.panel.book.updateNumbering.updateChapterAndParagraphNumbers'); } }, // [Book Panel Menu] Update Numbering › Update Chapter & Paragraph Numbers
  'id.panel.book.updateNumbering.updateAllNumbers': { get label() { return t('command.id.panel.book.updateNumbering.updateAllNumbers'); } }, // [Book Panel Menu] Update Numbering › Update All Numbers
  'id.panel.book.bookPageNumberingOptions': { get label() { return t('command.id.panel.book.bookPageNumberingOptions'); } }, // [Book Panel Menu] Book Page Numbering Options...
  'id.panel.book.documentNumberingOptions': { get label() { return t('command.id.panel.book.documentNumberingOptions'); } }, // [Book Panel Menu] Document Numbering Options...
  'id.panel.book.mergeIdenticallyNamedLayersOnExport': { get label() { return t('command.id.panel.book.mergeIdenticallyNamedLayersOnExport'); } }, // [Book Panel Menu] Merge Identically Named Layers on Export
  'id.panel.library.closeLibrary': { get label() { return t('command.id.panel.library.closeLibrary'); } }, // [Library Panel Menu] Close Library
  'id.panel.library.addItem': { get label() { return t('command.id.panel.library.addItem'); } }, // [Library Panel Menu] Add Item
  'id.panel.library.addItemsOnPage': { get label() { return t('command.id.panel.library.addItemsOnPage'); } }, // [Library Panel Menu] Add Items on Page
  'id.panel.library.addItemsOnPageAsSeparateObjects': { get label() { return t('command.id.panel.library.addItemsOnPageAsSeparateObjects'); } }, // [Library Panel Menu] Add Items on Page as Separate Objects
  'id.panel.library.placeItemS': { get label() { return t('command.id.panel.library.placeItemS'); } }, // [Library Panel Menu] Place Item(s)
  'id.panel.library.deleteItemS': { get label() { return t('command.id.panel.library.deleteItemS'); } }, // [Library Panel Menu] Delete Item(s)
  'id.panel.library.updateLibraryItem': { get label() { return t('command.id.panel.library.updateLibraryItem'); } }, // [Library Panel Menu] Update Library Item
  'id.panel.library.itemInformation': { get label() { return t('command.id.panel.library.itemInformation'); } }, // [Library Panel Menu] Item Information...
  'id.panel.library.showSubset': { get label() { return t('command.id.panel.library.showSubset'); } }, // [Library Panel Menu] Show Subset...
  'id.panel.library.showAll': { get label() { return t('command.id.panel.library.showAll'); } }, // [Library Panel Menu] Show All
  'id.panel.library.listView': { get label() { return t('command.id.panel.library.listView'); } }, // [Library Panel Menu] List View
  'id.panel.library.sortItems.byName': { get label() { return t('command.id.panel.library.sortItems.byName'); } }, // [Library Panel Menu] Sort Items › by Name
  'id.panel.library.sortItems.byNewest': { get label() { return t('command.id.panel.library.sortItems.byNewest'); } }, // [Library Panel Menu] Sort Items › by Newest
  'id.panel.library.sortItems.byOldest': { get label() { return t('command.id.panel.library.sortItems.byOldest'); } }, // [Library Panel Menu] Sort Items › by Oldest
  'id.panel.library.sortItems.byType': { get label() { return t('command.id.panel.library.sortItems.byType'); } }, // [Library Panel Menu] Sort Items › by Type
  'id.ctx.layoutNone.gridsAndGuides.guidesInBack': { get label() { return t('command.id.ctx.layoutNone.gridsAndGuides.guidesInBack'); } }, // [General Context Menu] Grids & Guides › Guides in Back
  'id.ctx.layoutNone.gridsAndGuides.gridsInBack': { get label() { return t('command.id.ctx.layoutNone.gridsAndGuides.gridsInBack'); } }, // [General Context Menu] Grids & Guides › Grids in Back
  'id.ctx.layoutObject.allowParentItemOverrides': { get label() { return t('command.id.ctx.layoutObject.allowParentItemOverrides'); } }, // [Layout Context Menu] Allow Parent Item Overrides
  'id.ctx.layoutObject.primaryTextFrame': { get label() { return t('command.id.ctx.layoutObject.primaryTextFrame'); } }, // [Layout Context Menu] Primary Text Frame
  'id.ctx.layoutObject.strokeWeight.0Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.0Pt'); } }, // [Layout Context Menu] Stroke Weight › 0 pt
  'id.ctx.layoutObject.strokeWeight.025Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.025Pt'); } }, // [Layout Context Menu] Stroke Weight › 0.25 pt
  'id.ctx.layoutObject.strokeWeight.05Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.05Pt'); } }, // [Layout Context Menu] Stroke Weight › 0.5 pt
  'id.ctx.layoutObject.strokeWeight.075Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.075Pt'); } }, // [Layout Context Menu] Stroke Weight › 0.75 pt
  'id.ctx.layoutObject.strokeWeight.1Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.1Pt'); } }, // [Layout Context Menu] Stroke Weight › 1 pt
  'id.ctx.layoutObject.strokeWeight.2Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.2Pt'); } }, // [Layout Context Menu] Stroke Weight › 2 pt
  'id.ctx.layoutObject.strokeWeight.3Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.3Pt'); } }, // [Layout Context Menu] Stroke Weight › 3 pt
  'id.ctx.layoutObject.strokeWeight.4Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.4Pt'); } }, // [Layout Context Menu] Stroke Weight › 4 pt
  'id.ctx.layoutObject.strokeWeight.5Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.5Pt'); } }, // [Layout Context Menu] Stroke Weight › 5 pt
  'id.ctx.layoutObject.strokeWeight.6Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.6Pt'); } }, // [Layout Context Menu] Stroke Weight › 6 pt
  'id.ctx.layoutObject.strokeWeight.7Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.7Pt'); } }, // [Layout Context Menu] Stroke Weight › 7 pt
  'id.ctx.layoutObject.strokeWeight.8Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.8Pt'); } }, // [Layout Context Menu] Stroke Weight › 8 pt
  'id.ctx.layoutObject.strokeWeight.9Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.9Pt'); } }, // [Layout Context Menu] Stroke Weight › 9 pt
  'id.ctx.layoutObject.strokeWeight.10Pt': { get label() { return t('command.id.ctx.layoutObject.strokeWeight.10Pt'); } }, // [Layout Context Menu] Stroke Weight › 10 pt
  'id.ctx.text.clearAllOverrides': { get label() { return t('command.id.ctx.text.clearAllOverrides'); } }, // [Text Context Menu] Clear All Overrides
  'id.ctx.text.loadSelectedGlyphInFind': { get label() { return t('command.id.ctx.text.loadSelectedGlyphInFind'); } }, // [Text Context Menu] Load Selected Glyph in Find
  'id.ctx.text.loadSelectedGlyphInChange': { get label() { return t('command.id.ctx.text.loadSelectedGlyphInChange'); } }, // [Text Context Menu] Load Selected Glyph in Change
  'id.ctx.text.goToFootnoteText': { get label() { return t('command.id.ctx.text.goToFootnoteText'); } }, // [Text Context Menu] Go to Footnote Text
  'id.ctx.text.goToEndnoteText': { get label() { return t('command.id.ctx.text.goToEndnoteText'); } }, // [Text Context Menu] Go to Endnote Text
  'id.ctx.text.arrangeByRows': { get label() { return t('command.id.ctx.text.arrangeByRows'); } }, // [Text Context Menu] Arrange by rows
  'id.ctx.text.arrangeByColumns': { get label() { return t('command.id.ctx.text.arrangeByColumns'); } }, // [Text Context Menu] Arrange by columns
  'id.ctx.table.convertToHeaderRows': { get label() { return t('command.id.ctx.table.convertToHeaderRows'); } }, // [Table Context Menu] Convert to Header Rows
  'id.ctx.table.convertToBodyRows': { get label() { return t('command.id.ctx.table.convertToBodyRows'); } }, // [Table Context Menu] Convert to Body Rows
  'id.ctx.table.convertToFooterRows': { get label() { return t('command.id.ctx.table.convertToFooterRows'); } }, // [Table Context Menu] Convert to Footer Rows
  'id.ctx.table.writingDirection.horizontal': { get label() { return t('command.id.ctx.table.writingDirection.horizontal'); } }, // [Table Context Menu] Writing Direction › Horizontal
  'id.ctx.table.writingDirection.vertical': { get label() { return t('command.id.ctx.table.writingDirection.vertical'); } }, // [Table Context Menu] Writing Direction › Vertical
  'id.ctx.table.rotate.0Deg': { get label() { return t('command.id.ctx.table.rotate.0Deg'); } }, // [Table Context Menu] Rotate › 0°
  'id.ctx.table.rotate.90Deg': { get label() { return t('command.id.ctx.table.rotate.90Deg'); } }, // [Table Context Menu] Rotate › 90°
  'id.ctx.table.rotate.180Deg': { get label() { return t('command.id.ctx.table.rotate.180Deg'); } }, // [Table Context Menu] Rotate › 180°
  'id.ctx.table.rotate.270Deg': { get label() { return t('command.id.ctx.table.rotate.270Deg'); } }, // [Table Context Menu] Rotate › 270°
  'id.ctx.guide.guideType.rulerGuide': { get label() { return t('command.id.ctx.guide.guideType.rulerGuide'); } }, // [Guide Context Menu] Guide Type › Ruler Guide
  'id.ctx.guide.moveGuides': { get label() { return t('command.id.ctx.guide.moveGuides'); } }, // [Guide Context Menu] Move Guides...
  'id.ctx.guide.unlock': { get label() { return t('command.id.ctx.guide.unlock'); } }, // [Guide Context Menu] Unlock
  'id.ctx.styleParagraph.apply': { get label() { return t('command.id.ctx.styleParagraph.apply'); } }, // [Text Style List Context Menu] Apply "^1"
  'id.ctx.styleParagraph.applyClearOverrides': { get label() { return t('command.id.ctx.styleParagraph.applyClearOverrides'); } }, // [Text Style List Context Menu] Apply "^1", Clear Overrides
  'id.ctx.styleParagraph.applyClearCharacterStyles': { get label() { return t('command.id.ctx.styleParagraph.applyClearCharacterStyles'); } }, // [Text Style List Context Menu] Apply "^1", Clear Character Styles
  'id.ctx.styleParagraph.applyClearAll': { get label() { return t('command.id.ctx.styleParagraph.applyClearAll'); } }, // [Text Style List Context Menu] Apply "^1", Clear All
  'id.ctx.styleParagraph.applyThenNextStyle': { get label() { return t('command.id.ctx.styleParagraph.applyThenNextStyle'); } }, // [Text Style List Context Menu] Apply "^1" then Next Style
  'id.ctx.styleParagraph.applyThenNextStyleClearOverrides': { get label() { return t('command.id.ctx.styleParagraph.applyThenNextStyleClearOverrides'); } }, // [Text Style List Context Menu] Apply "^1" then Next Style, Clear Overrides
  'id.ctx.styleParagraph.applyThenNextStyleClearCharacterStyles': { get label() { return t('command.id.ctx.styleParagraph.applyThenNextStyleClearCharacterStyles'); } }, // [Text Style List Context Menu] Apply "^1" then Next Style, Clear Character Styles
  'id.ctx.styleParagraph.applyThenNextStyleClearAll': { get label() { return t('command.id.ctx.styleParagraph.applyThenNextStyleClearAll'); } }, // [Text Style List Context Menu] Apply "^1" then Next Style, Clear All
  'id.ctx.styleParagraph.styleGroupOptions': { get label() { return t('command.id.ctx.styleParagraph.styleGroupOptions'); } }, // [Text Style List Context Menu] Style Group Options...
  'id.ctx.styleParagraph.duplicateGroup': { get label() { return t('command.id.ctx.styleParagraph.duplicateGroup'); } }, // [Text Style List Context Menu] Duplicate Group
  'id.ctx.styleParagraph.deleteGroup': { get label() { return t('command.id.ctx.styleParagraph.deleteGroup'); } }, // [Text Style List Context Menu] Delete Group...
  'id.ctx.styleObject.applyStyle': { get label() { return t('command.id.ctx.styleObject.applyStyle'); } }, // [Object Styles Item Context Menu] Apply Style
  'id.ctx.styleObject.applyStyleClearOverrides': { get label() { return t('command.id.ctx.styleObject.applyStyleClearOverrides'); } }, // [Object Styles Item Context Menu] Apply Style, Clear Overrides
  'id.ctx.styleObject.applyStyleClearAttributesNotDefinedByStyle': { get label() { return t('command.id.ctx.styleObject.applyStyleClearAttributesNotDefinedByStyle'); } }, // [Object Styles Item Context Menu] Apply Style, Clear Attributes Not Defined by Style
  'id.ctx.styleObject.applyStyleClearAll': { get label() { return t('command.id.ctx.styleObject.applyStyleClearAll'); } }, // [Object Styles Item Context Menu] Apply Style, Clear All
  'id.ctx.styleObject.styleGroupOptions': { get label() { return t('command.id.ctx.styleObject.styleGroupOptions'); } }, // [Object Styles Item Context Menu] Style Group Options...
  'id.ctx.styleObject.duplicateGroup': { get label() { return t('command.id.ctx.styleObject.duplicateGroup'); } }, // [Object Styles Item Context Menu] Duplicate Group...
  'id.ctx.styleObject.deleteGroup': { get label() { return t('command.id.ctx.styleObject.deleteGroup'); } }, // [Object Styles Item Context Menu] Delete Group...
  'id.ctx.styleTable.applyStyle': { get label() { return t('command.id.ctx.styleTable.applyStyle'); } }, // [Tables Style Panel Context Menu] Apply Style
  'id.ctx.styleTable.applyStyleClearOverrides': { get label() { return t('command.id.ctx.styleTable.applyStyleClearOverrides'); } }, // [Tables Style Panel Context Menu] Apply Style, Clear Overrides
  'id.ctx.styleTable.applyStyleClearCellStyles': { get label() { return t('command.id.ctx.styleTable.applyStyleClearCellStyles'); } }, // [Tables Style Panel Context Menu] Apply Style, Clear Cell Styles
  'id.ctx.styleTable.applyStyleClearAll': { get label() { return t('command.id.ctx.styleTable.applyStyleClearAll'); } }, // [Tables Style Panel Context Menu] Apply Style, Clear All
  'id.ctx.styleTable.styleGroupOptions': { get label() { return t('command.id.ctx.styleTable.styleGroupOptions'); } }, // [Tables Style Panel Context Menu] Style Group Options...
  'id.ctx.styleTable.duplicateStyleGroup': { get label() { return t('command.id.ctx.styleTable.duplicateStyleGroup'); } }, // [Tables Style Panel Context Menu] Duplicate Style Group
  'id.ctx.styleTable.deleteStyleGroup': { get label() { return t('command.id.ctx.styleTable.deleteStyleGroup'); } }, // [Tables Style Panel Context Menu] Delete Style Group
  'id.ctx.styleCell.applyStyle': { get label() { return t('command.id.ctx.styleCell.applyStyle'); } }, // [Cell Styles Panel Context Menu] Apply Style
  'id.ctx.styleCell.applyStyleClearAll': { get label() { return t('command.id.ctx.styleCell.applyStyleClearAll'); } }, // [Cell Styles Panel Context Menu] Apply Style, Clear All
  'id.ctx.styleCell.styleGroupOptions': { get label() { return t('command.id.ctx.styleCell.styleGroupOptions'); } }, // [Cell Styles Panel Context Menu] Style Group Options...
  'id.ctx.styleCell.duplicateStyleGroup': { get label() { return t('command.id.ctx.styleCell.duplicateStyleGroup'); } }, // [Cell Styles Panel Context Menu] Duplicate Style Group
  'id.ctx.styleCell.deleteStyleGroup': { get label() { return t('command.id.ctx.styleCell.deleteStyleGroup'); } }, // [Cell Styles Panel Context Menu] Delete Style Group
  'id.ctx.glyphs.deleteGlyphFromSet': { get label() { return t('command.id.ctx.glyphs.deleteGlyphFromSet'); } }, // [Glyphs Panel Context Menu] Delete Glyph from Set
  'id.ctx.glyphs.loadGlyphInFind': { get label() { return t('command.id.ctx.glyphs.loadGlyphInFind'); } }, // [Glyphs Panel Context Menu] Load Glyph in Find
  'id.ctx.glyphs.loadGlyphInChange': { get label() { return t('command.id.ctx.glyphs.loadGlyphInChange'); } }, // [Glyphs Panel Context Menu] Load Glyph in Change
  'id.ctx.glyphsRecent.deleteGlyphFromRecentlyUsed': { get label() { return t('command.id.ctx.glyphsRecent.deleteGlyphFromRecentlyUsed'); } }, // [Glyphs Panel Recently Used Context Menu] Delete Glyph from Recently Used
  'id.ctx.glyphsRecent.clearAllRecentlyUsed': { get label() { return t('command.id.ctx.glyphsRecent.clearAllRecentlyUsed'); } }, // [Glyphs Panel Recently Used Context Menu] Clear All Recently Used
  'id.ctx.tabs.clearAll': { get label() { return t('command.id.ctx.tabs.clearAll'); } }, // [Tabs Context Menu] Clear All
  'id.ctx.tabs.deleteTab': { get label() { return t('command.id.ctx.tabs.deleteTab'); } }, // [Tabs Context Menu] Delete Tab
  'id.ctx.tabs.repeatTab': { get label() { return t('command.id.ctx.tabs.repeatTab'); } }, // [Tabs Context Menu] Repeat Tab
  'id.ctx.tabs.resetIndents': { get label() { return t('command.id.ctx.tabs.resetIndents'); } }, // [Tabs Context Menu] Reset Indents
  'id.ctx.tabs.tabType.leftJustifiedTab': { get label() { return t('command.id.ctx.tabs.tabType.leftJustifiedTab'); } }, // [Tabs Context Menu] Tab Type › Left-Justified Tab
  'id.ctx.tabs.tabType.centerJustifiedTab': { get label() { return t('command.id.ctx.tabs.tabType.centerJustifiedTab'); } }, // [Tabs Context Menu] Tab Type › Center-Justified Tab
  'id.ctx.tabs.tabType.rightJustifiedTab': { get label() { return t('command.id.ctx.tabs.tabType.rightJustifiedTab'); } }, // [Tabs Context Menu] Tab Type › Right-Justified Tab
  'id.ctx.tabs.tabType.decimalTab': { get label() { return t('command.id.ctx.tabs.tabType.decimalTab'); } }, // [Tabs Context Menu] Tab Type › Decimal Tab
  'id.ctx.rulerH.points': { get label() { return t('command.id.ctx.rulerH.points'); } }, // [Horizontal Ruler Context Menu] Points
  'id.ctx.rulerH.picas': { get label() { return t('command.id.ctx.rulerH.picas'); } }, // [Horizontal Ruler Context Menu] Picas
  'id.ctx.rulerH.inches': { get label() { return t('command.id.ctx.rulerH.inches'); } }, // [Horizontal Ruler Context Menu] Inches
  'id.ctx.rulerH.inchesDecimal': { get label() { return t('command.id.ctx.rulerH.inchesDecimal'); } }, // [Horizontal Ruler Context Menu] Inches Decimal
  'id.ctx.rulerH.millimeters': { get label() { return t('command.id.ctx.rulerH.millimeters'); } }, // [Horizontal Ruler Context Menu] Millimeters
  'id.ctx.rulerH.centimeters': { get label() { return t('command.id.ctx.rulerH.centimeters'); } }, // [Horizontal Ruler Context Menu] Centimeters
  'id.ctx.rulerH.ciceros': { get label() { return t('command.id.ctx.rulerH.ciceros'); } }, // [Horizontal Ruler Context Menu] Ciceros
  'id.ctx.rulerH.agates': { get label() { return t('command.id.ctx.rulerH.agates'); } }, // [Horizontal Ruler Context Menu] Agates
  'id.ctx.rulerH.pixels': { get label() { return t('command.id.ctx.rulerH.pixels'); } }, // [Horizontal Ruler Context Menu] Pixels
  'id.ctx.rulerH.rulerPerPage': { get label() { return t('command.id.ctx.rulerH.rulerPerPage'); } }, // [Horizontal Ruler Context Menu] Ruler Per Page
  'id.ctx.rulerH.rulerPerSpread': { get label() { return t('command.id.ctx.rulerH.rulerPerSpread'); } }, // [Horizontal Ruler Context Menu] Ruler Per Spread
  'id.ctx.rulerH.rulerOnSpine': { get label() { return t('command.id.ctx.rulerH.rulerOnSpine'); } }, // [Horizontal Ruler Context Menu] Ruler On Spine
  'id.ctx.rulerV.points': { get label() { return t('command.id.ctx.rulerV.points'); } }, // [Vertical Ruler Context Menu] Points
  'id.ctx.rulerV.picas': { get label() { return t('command.id.ctx.rulerV.picas'); } }, // [Vertical Ruler Context Menu] Picas
  'id.ctx.rulerV.inches': { get label() { return t('command.id.ctx.rulerV.inches'); } }, // [Vertical Ruler Context Menu] Inches
  'id.ctx.rulerV.inchesDecimal': { get label() { return t('command.id.ctx.rulerV.inchesDecimal'); } }, // [Vertical Ruler Context Menu] Inches Decimal
  'id.ctx.rulerV.millimeters': { get label() { return t('command.id.ctx.rulerV.millimeters'); } }, // [Vertical Ruler Context Menu] Millimeters
  'id.ctx.rulerV.centimeters': { get label() { return t('command.id.ctx.rulerV.centimeters'); } }, // [Vertical Ruler Context Menu] Centimeters
  'id.ctx.rulerV.ciceros': { get label() { return t('command.id.ctx.rulerV.ciceros'); } }, // [Vertical Ruler Context Menu] Ciceros
  'id.ctx.rulerV.agates': { get label() { return t('command.id.ctx.rulerV.agates'); } }, // [Vertical Ruler Context Menu] Agates
  'id.ctx.rulerV.pixels': { get label() { return t('command.id.ctx.rulerV.pixels'); } }, // [Vertical Ruler Context Menu] Pixels
  'id.ctx.zeroPoint.lockZeroPoint': { get label() { return t('command.id.ctx.zeroPoint.lockZeroPoint'); } }, // [Zero Point Context Menu] Lock Zero Point
  'id.ctx.zeroPoint.points': { get label() { return t('command.id.ctx.zeroPoint.points'); } }, // [Zero Point Context Menu] Points
  'id.ctx.zeroPoint.picas': { get label() { return t('command.id.ctx.zeroPoint.picas'); } }, // [Zero Point Context Menu] Picas
  'id.ctx.zeroPoint.inches': { get label() { return t('command.id.ctx.zeroPoint.inches'); } }, // [Zero Point Context Menu] Inches
  'id.ctx.zeroPoint.inchesDecimal': { get label() { return t('command.id.ctx.zeroPoint.inchesDecimal'); } }, // [Zero Point Context Menu] Inches Decimal
  'id.ctx.zeroPoint.millimeters': { get label() { return t('command.id.ctx.zeroPoint.millimeters'); } }, // [Zero Point Context Menu] Millimeters
  'id.ctx.zeroPoint.centimeters': { get label() { return t('command.id.ctx.zeroPoint.centimeters'); } }, // [Zero Point Context Menu] Centimeters
  'id.ctx.zeroPoint.ciceros': { get label() { return t('command.id.ctx.zeroPoint.ciceros'); } }, // [Zero Point Context Menu] Ciceros
  'id.ctx.zeroPoint.agates': { get label() { return t('command.id.ctx.zeroPoint.agates'); } }, // [Zero Point Context Menu] Agates
  'id.ctx.zeroPoint.pixels': { get label() { return t('command.id.ctx.zeroPoint.pixels'); } }, // [Zero Point Context Menu] Pixels
  'id.ctx.pagesSection.deleteSection': { get label() { return t('command.id.ctx.pagesSection.deleteSection'); } }, // [Pages Panel Section Context Menu] Delete Section
  'id.ctx.linksHeader.hideThisColumn': { get label() { return t('command.id.ctx.linksHeader.hideThisColumn'); } }, // [Links Header Context Menu] Hide this Column
  'id.ctx.linkInfo.copyInfoField': { get label() { return t('command.id.ctx.linkInfo.copyInfoField'); } }, // [Link Info Context Menu] Copy Info Field
};

/** The menu bar: 파일 편집 레이아웃 문자 개체 표 보기 창 도움말. */
export const MAIN_MENUS: MenuDef[] = [
  { get label() { return t('menu.file'); }, mnemonic: 'F', items: [
    m('menu.main.file.new', [
      c('file.new'),
      c('id.file.new.book'),
      c('id.file.new.library'),
    ]),
    c('file.newProject'),
    c('file.open'),
    c('file.openProject'),
    c('file.openInNewWindow'),
    c('file.import'),
    m(RECENT_SUBMENU, [
      c('recent.none'),
    ]),
    S,
    c('file.close'),
    c('file.closeProject'),
    c('file.save'),
    c('file.saveAs'),
    c('file.saveCopy'),
    c('file.revert'),
    S,
    c('file.place'),
    c('id.file.importPdfComments'),
    S,
    m('menu.main.file.pdfPresets', [
      c('id.file.pdfPresets.define'),
      S,
      c('id.file.pdfPresets.highQualityPrint'),
      c('id.file.pdfPresets.pdfX1a2001'),
      c('id.file.pdfPresets.pdfX32002'),
      c('id.file.pdfPresets.pdfX42008'),
      c('id.file.pdfPresets.pressQuality'),
      c('id.file.pdfPresets.smallestFileSize'),
    ]),
    c('file.exportPdf'),
    c('file.exportIdml'),
    S,
    m('menu.main.file.documentPresets', [
      c('id.file.documentPresets.define'),
      S,
      c('id.file.documentPresets.default'),
    ]),
    c('file.documentSetup'),
    c('layout.adjust'),
    S,
    c('id.file.user'),
    c('file.fileInfo'),
    m('menu.main.file.userSettings', [
      c('id.file.userSettings.importUserSettings'),
      c('id.file.userSettings.exportUserSettings'),
    ]),
    S,
    c('file.package'),
    m('menu.main.file.printPresets', [
      c('id.file.printPresets.define'),
      S,
      c('id.file.printPresets.default'),
    ]),
    c('file.print'),
    c('id.file.printBooklet'),
    S,
    c('file.quit'),
  ] },
  { get label() { return t('menu.edit'); }, mnemonic: 'E', items: [
    c('edit.undo'),
    c('edit.redo'),
    S,
    c('edit.cut'),
    c('edit.copy'),
    c('edit.paste'),
    c('edit.pasteNoFormat'),
    c('edit.pasteInto'),
    c('edit.pasteInPlace'),
    c('edit.clear'),
    S,
    c('edit.duplicate'),
    c('edit.stepRepeat'),
    c('id.edit.placeAndLink'),
    S,
    c('edit.selectAll'),
    c('edit.deselectAll'),
    S,
    c('id.edit.editOriginal'),
    m('menu.main.edit.editWith', [
      c('id.list.editWith'),
      S,
      c('id.edit.editWith.other'),
    ]),
    c('id.edit.goToSource'),
    c('edit.storyEditor'),
    S,
    c('edit.quickApply'),
    c('edit.findChange'),
    c('edit.findNext'),
    m('menu.main.edit.spelling', [
      c('edit.spelling'),
      c('id.edit.spelling.dynamicSpelling'),
      c('id.edit.spelling.autocorrect'),
      c('id.edit.spelling.userDictionary'),
    ]),
    S,
    m('menu.main.edit.transparencyBlendSpace', [
      c('id.edit.transparencyBlendSpace.documentRgb'),
      c('id.edit.transparencyBlendSpace.documentCmyk'),
    ]),
    c('id.edit.transparencyFlattenerPresets'),
    S,
    c('edit.colorSettings'),
    c('id.edit.assignProfiles'),
    c('id.edit.convertToProfile'),
    S,
    c('edit.shortcuts'),
    m('menu.main.edit.preferences', [
      c('edit.preferences'),
      c('pref.language'),
      c('ai.settings'),
      c('id.edit.preferences.interface'),
      m('menu.interfaceColorTheme', [
        c('theme.dark'),
        c('theme.mediumDark'),
        c('theme.mediumLight'),
        c('theme.light'),
      ]),
      c('id.edit.preferences.userInterfaceScaling'),
      S,
      c('id.edit.preferences.type'),
      c('id.edit.preferences.advancedType'),
      c('id.edit.preferences.composition'),
      S,
      c('id.edit.preferences.unitsAndIncrements'),
      m('menu.rulerUnits', [
        c('pref.unitsDocument'),
        c('pref.unitsMillimeters'),
        c('pref.unitsPoints'),
      ]),
      c('id.edit.preferences.grids'),
      c('id.edit.preferences.guidesAndPasteboard'),
      S,
      c('id.edit.preferences.dictionary'),
      c('id.edit.preferences.spelling'),
      c('id.edit.preferences.autocorrect'),
      S,
      c('id.edit.preferences.notes'),
      c('id.edit.preferences.trackChanges'),
      S,
      c('id.edit.preferences.storyEditorDisplay'),
      c('id.edit.preferences.displayPerformance'),
      S,
      c('id.edit.preferences.appearanceOfBlack'),
      c('id.edit.preferences.fileHandling'),
      c('id.edit.preferences.clipboardHandling'),
    ]),
  ] },
  { get label() { return t('menu.layout'); }, mnemonic: 'L', items: [
    m('menu.main.layout.pages', [
      c('layout.addPage'),
      c('layout.insertPages'),
      c('layout.movePages'),
      c('id.layout.pages.duplicateSpread'),
      c('layout.deletePages'),
      c('id.layout.pages.hideSpread'),
      S,
      c('id.layout.pages.printPages'),
      S,
      c('id.layout.pages.applyParentToPages'),
    ]),
    c('layout.margins'),
    c('layout.rulerGuides'),
    c('layout.createGuides'),
    S,
    c('layout.firstPage'),
    c('layout.prevPage'),
    c('layout.nextPage'),
    c('layout.lastPage'),
    c('layout.nextSpread'),
    c('layout.prevSpread'),
    S,
    c('layout.goToPage'),
    S,
    c('layout.goBack'),
    c('layout.goForward'),
    S,
    c('layout.numbering'),
    S,
    c('layout.toc'),
    c('id.layout.updateTableOfContents'),
    c('id.layout.tableOfContentsStyles'),
  ] },
  { get label() { return t('menu.type'); }, mnemonic: 'T', items: [
    m('menu.main.type.font', [
      c('id.list.fonts'),
    ]),
    m('menu.main.type.size', [
      c('id.type.size.other'),
      S,
      c('id.type.size.6Pt'),
      c('id.type.size.8Pt'),
      c('id.type.size.9Pt'),
      c('id.type.size.10Pt'),
      c('id.type.size.11Pt'),
      c('id.type.size.12Pt'),
      c('id.type.size.14Pt'),
      c('id.type.size.18Pt'),
      c('id.type.size.24Pt'),
      c('id.type.size.30Pt'),
      c('id.type.size.36Pt'),
      c('id.type.size.48Pt'),
      c('id.type.size.60Pt'),
      c('id.type.size.72Pt'),
    ]),
    S,
    c('panel.character'),
    c('panel.paragraph'),
    c('type.tabs'),
    c('type.glyphs'),
    c('type.story'),
    S,
    c('panel.characterStyles'),
    c('panel.paragraphStyles'),
    S,
    c('type.createOutlines'),
    c('type.findFont'),
    m('menu.main.type.changeCase', [
      c('id.type.changeCase.uppercase'),
      c('id.type.changeCase.lowercase'),
      c('id.type.changeCase.titleCase'),
      c('id.type.changeCase.sentenceCase'),
    ]),
    S,
    m('menu.main.type.typeOnAPath', [
      c('id.type.typeOnAPath.options'),
      c('id.type.typeOnAPath.deleteTypeFromPath'),
    ]),
    m('menu.typeOrientation', [
      c('type.orientationHorizontal'),
      c('type.orientationVertical'),
    ]),
    S,
    m('menu.main.type.notes', [
      c('id.type.notes.notesMode'),
      S,
      c('id.type.notes.newNote'),
      c('id.type.notes.openNote'),
      c('id.type.notes.deleteNote'),
      S,
      c('id.type.notes.previousNote'),
      c('id.type.notes.nextNote'),
      S,
      c('id.type.notes.convertToNote'),
      c('id.type.notes.convertToText'),
      S,
      c('id.type.notes.splitNote'),
      S,
      c('id.type.notes.expandCollapseNotesInStory'),
      S,
      c('id.type.notes.removeNotesFromStory'),
      c('id.type.notes.removeAllNotes'),
    ]),
    m('menu.main.type.trackChanges', [
      c('id.type.trackChanges.trackChangesInCurrentStory'),
      S,
      c('id.type.trackChanges.enableTrackingInAllStories'),
      c('id.type.trackChanges.disableTrackingInAllStories'),
      S,
      c('id.type.trackChanges.previousChange'),
      c('id.type.trackChanges.nextChange'),
      S,
      c('id.type.trackChanges.acceptChange'),
      c('id.type.trackChanges.rejectChange'),
      S,
      c('id.type.trackChanges.acceptChangeFindNext'),
      c('id.type.trackChanges.rejectChangeFindNext'),
      S,
      m('menu.main.type.trackChanges.acceptAllChanges', [
        c('id.type.trackChanges.acceptAllChanges.inThisStory'),
        c('id.type.trackChanges.acceptAllChanges.inThisDocument'),
      ]),
      m('menu.main.type.trackChanges.rejectAllChanges', [
        c('id.type.trackChanges.rejectAllChanges.inThisStory'),
        c('id.type.trackChanges.rejectAllChanges.inThisDocument'),
      ]),
      S,
      m('menu.main.type.trackChanges.acceptAllChangesByThisUser', [
        c('id.type.trackChanges.acceptAllChangesByThisUser.inThisStory'),
        c('id.type.trackChanges.acceptAllChangesByThisUser.inThisDocument'),
      ]),
      m('menu.main.type.trackChanges.rejectAllChangesByThisUser', [
        c('id.type.trackChanges.rejectAllChangesByThisUser.inThisStory'),
        c('id.type.trackChanges.rejectAllChangesByThisUser.inThisDocument'),
      ]),
    ]),
    S,
    c('id.type.insertFootnote'),
    c('id.type.documentFootnoteOptions'),
    c('id.type.insertEndnote'),
    c('id.type.documentEndnoteOptions'),
    c('id.type.convertFootnotesAndEndnotes'),
    S,
    m('menu.main.type.hyperlinksAndCrossReferences', [
      c('id.type.hyperlinksAndCrossReferences.newHyperlink'),
      c('id.type.hyperlinksAndCrossReferences.editHyperlink'),
      S,
      c('id.type.hyperlinksAndCrossReferences.insertCrossReference'),
      c('id.type.hyperlinksAndCrossReferences.crossReferenceOptions'),
      c('id.type.hyperlinksAndCrossReferences.updateCrossReference'),
    ]),
    m('menu.main.type.textVariables', [
      c('id.type.textVariables.define'),
      m('menu.main.type.textVariables.insertVariable', [
        c('id.type.textVariables.insertVariable.chapterNumber'),
        c('id.type.textVariables.insertVariable.creationDate'),
        c('id.type.textVariables.insertVariable.fileName'),
        c('id.type.textVariables.insertVariable.imageName'),
        c('id.type.textVariables.insertVariable.lastPageNumber'),
        c('id.type.textVariables.insertVariable.modificationDate'),
        c('id.type.textVariables.insertVariable.outputDate'),
        c('id.type.textVariables.insertVariable.runningHeader'),
      ]),
      c('id.type.textVariables.convertVariableToText'),
    ]),
    S,
    m('menu.main.type.bulletedAndNumberedLists', [
      c('id.type.bulletedAndNumberedLists.applyRemoveBullets'),
      c('id.type.bulletedAndNumberedLists.applyRemoveNumbers'),
      c('id.type.bulletedAndNumberedLists.restartContinueNumbering'),
      c('id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText'),
      c('id.type.bulletedAndNumberedLists.defineLists'),
    ]),
    S,
    m('menu.main.type.insertSpecialCharacter', [
      m('menu.main.type.insertSpecialCharacter.symbols', [
        c('id.type.insertSpecialCharacter.symbols.bulletCharacter'),
        c('id.type.insertSpecialCharacter.symbols.copyrightSymbol'),
        c('id.type.insertSpecialCharacter.symbols.ellipsis'),
        c('id.type.insertSpecialCharacter.symbols.paragraphSymbol'),
        c('id.type.insertSpecialCharacter.symbols.registeredTrademarkSymbol'),
        c('id.type.insertSpecialCharacter.symbols.sectionSymbol'),
        c('id.type.insertSpecialCharacter.symbols.trademarkSymbol'),
      ]),
      m('menu.main.type.insertSpecialCharacter.markers', [
        c('id.type.insertSpecialCharacter.markers.currentPageNumber'),
        c('id.type.insertSpecialCharacter.markers.nextPageNumber'),
        c('id.type.insertSpecialCharacter.markers.previousPageNumber'),
        c('id.type.insertSpecialCharacter.markers.sectionMarker'),
        c('id.type.insertSpecialCharacter.markers.footnoteNumber'),
      ]),
      m('menu.main.type.insertSpecialCharacter.hyphensAndDashes', [
        c('id.type.insertSpecialCharacter.hyphensAndDashes.emDash'),
        c('id.type.insertSpecialCharacter.hyphensAndDashes.enDash'),
        c('id.type.insertSpecialCharacter.hyphensAndDashes.discretionaryHyphen'),
        c('id.type.insertSpecialCharacter.hyphensAndDashes.nonbreakingHyphen'),
      ]),
      m('menu.main.type.insertSpecialCharacter.quotationMarks', [
        c('id.type.insertSpecialCharacter.quotationMarks.doubleLeftQuotationMarks'),
        c('id.type.insertSpecialCharacter.quotationMarks.doubleRightQuotationMarks'),
        c('id.type.insertSpecialCharacter.quotationMarks.singleLeftQuotationMark'),
        c('id.type.insertSpecialCharacter.quotationMarks.singleRightQuotationMark'),
        c('id.type.insertSpecialCharacter.quotationMarks.straightDoubleQuotationMarks'),
        c('id.type.insertSpecialCharacter.quotationMarks.straightSingleQuotationMarkApostrophe'),
      ]),
      m('menu.main.type.insertSpecialCharacter.other', [
        c('id.type.insertSpecialCharacter.other.tab'),
        c('id.type.insertSpecialCharacter.other.rightIndentTab'),
        c('id.type.insertSpecialCharacter.other.indentToHere'),
        c('id.type.insertSpecialCharacter.other.endNestedStyleHere'),
        c('id.type.insertSpecialCharacter.other.nonJoiner'),
      ]),
    ]),
    m('menu.main.type.insertWhiteSpace', [
      c('id.type.insertWhiteSpace.emSpace'),
      c('id.type.insertWhiteSpace.enSpace'),
      S,
      c('id.type.insertWhiteSpace.nonbreakingSpace'),
      c('id.type.insertWhiteSpace.nonbreakingSpaceFixedWidth'),
      S,
      c('id.type.insertWhiteSpace.hairSpace'),
      c('id.type.insertWhiteSpace.sixthSpace'),
      c('id.type.insertWhiteSpace.thinSpace'),
      c('id.type.insertWhiteSpace.quarterSpace'),
      c('id.type.insertWhiteSpace.thirdSpace'),
      c('id.type.insertWhiteSpace.punctuationSpace'),
      c('id.type.insertWhiteSpace.figureSpace'),
      c('id.type.insertWhiteSpace.flushSpace'),
    ]),
    m('menu.main.type.insertBreakCharacter', [
      c('id.type.insertBreakCharacter.columnBreak'),
      c('id.type.insertBreakCharacter.frameBreak'),
      c('id.type.insertBreakCharacter.pageBreak'),
      c('id.type.insertBreakCharacter.oddPageBreak'),
      c('id.type.insertBreakCharacter.evenPageBreak'),
      c('id.type.insertBreakCharacter.paragraphReturn'),
      S,
      c('id.type.insertBreakCharacter.forcedLineBreak'),
      c('id.type.insertBreakCharacter.discretionaryLineBreak'),
    ]),
    c('type.placeholder'),
    S,
    c('type.hiddenChars'),
  ] },
  { get label() { return t('menu.object'); }, mnemonic: 'O', items: [
    m('menu.main.object.transform', [
      c('object.move'),
      c('id.object.transform.scale'),
      c('id.object.transform.rotate'),
      c('id.object.transform.shear'),
      S,
      c('id.object.transform.rotate90DegCw'),
      c('id.object.transform.rotate90DegCcw'),
      c('id.object.transform.rotate180Deg'),
      S,
      c('id.object.transform.flipHorizontal'),
      c('id.object.transform.flipVertical'),
      S,
      c('id.object.transform.clearTransformations'),
    ]),
    m('menu.main.object.transformAgain', [
      c('object.transformAgain'),
      c('id.object.transformAgain.transformAgainIndividually'),
      c('object.transformSequenceAgain'),
      c('id.object.transformAgain.transformSequenceAgainIndividually'),
    ]),
    m('menu.main.object.arrange', [
      c('object.bringFront'),
      c('object.forward'),
      c('object.backward'),
      c('object.sendBack'),
    ]),
    m('menu.main.object.select', [
      c('object.selectAbove'),
      c('object.selectNextAbove'),
      c('object.selectNextBelow'),
      c('object.selectBelow'),
      S,
      c('object.selectContainer'),
      c('object.selectContent'),
      S,
      c('object.selectPrevInGroup'),
      c('object.selectNextInGroup'),
    ]),
    S,
    c('object.group'),
    c('object.ungroup'),
    c('object.lock'),
    c('object.unlockAll'),
    c('object.hide'),
    c('object.showAll'),
    S,
    c('id.object.insertMathml'),
    S,
    c('id.object.generateQrCode'),
    S,
    c('object.textFrameOptions'),
    m('menu.main.object.anchoredObject', [
      c('id.object.anchoredObject.options'),
      c('id.object.anchoredObject.insert'),
      c('id.object.anchoredObject.release'),
    ]),
    S,
    m('menu.main.object.fitting', [
      c('object.fillProportional'),
      c('object.fitProportional'),
      S,
      c('object.fitFrameContent'),
      c('object.fitContentFrame'),
      c('object.centerContent'),
      S,
      c('id.object.fitting.clearFrameFittingOptions'),
      S,
      c('object.fittingOptions'),
    ]),
    m('menu.main.object.content', [
      c('id.object.content.graphic'),
      c('id.object.content.text'),
      c('id.object.content.unassigned'),
    ]),
    S,
    m('menu.main.object.effects', [
      c('id.object.effects.transparency'),
      S,
      c('object.dropShadow'),
      c('id.object.effects.innerShadow'),
      c('id.object.effects.outerGlow'),
      c('id.object.effects.innerGlow'),
      c('id.object.effects.bevelAndEmboss'),
      c('id.object.effects.satin'),
      c('id.object.effects.basicFeather'),
      c('id.object.effects.directionalFeather'),
      c('id.object.effects.gradientFeather'),
      S,
      c('id.object.effects.clearEffects'),
      c('id.object.effects.clearAllTransparency'),
      S,
      c('id.object.effects.globalLight'),
    ]),
    c('object.cornerOptions'),
    S,
    c('object.layerOptions'),
    m('menu.main.object.captions', [
      c('id.object.captions.captionSetup'),
      c('id.object.captions.generateLiveCaption'),
      c('id.object.captions.generateStaticCaption'),
      c('id.object.captions.convertToStaticCaption'),
    ]),
    m('menu.main.object.clippingPath', [
      c('id.object.clippingPath.options'),
      c('id.object.clippingPath.convertClippingPathToFrame'),
    ]),
    c('id.object.imageColorSettings'),
    S,
    m('menu.main.object.paths', [
      c('id.object.paths.join'),
      c('id.object.paths.openPath'),
      c('id.object.paths.closePath'),
      c('id.object.paths.reversePath'),
      S,
      c('object.compoundPath'),
      c('id.object.paths.releaseCompoundPath'),
    ]),
    m('menu.main.object.pathfinder', [
      c('id.object.pathfinder.add'),
      c('id.object.pathfinder.subtract'),
      c('id.object.pathfinder.intersect'),
      c('id.object.pathfinder.excludeOverlap'),
      c('id.object.pathfinder.minusBack'),
    ]),
    m('menu.main.object.convertShape', [
      c('id.object.convertShape.rectangle'),
      c('id.object.convertShape.roundedRectangle'),
      c('id.object.convertShape.beveledRectangle'),
      c('id.object.convertShape.inverseRoundedRectangle'),
      c('id.object.convertShape.ellipse'),
      c('id.object.convertShape.triangle'),
      c('id.object.convertShape.polygon'),
      c('id.object.convertShape.line'),
      c('id.object.convertShape.orthogonalLine'),
    ]),
    m('menu.main.object.convertPoint', [
      c('id.object.convertPoint.plain'),
      c('id.object.convertPoint.corner'),
      c('id.object.convertPoint.smooth'),
      c('id.object.convertPoint.symmetrical'),
    ]),
    S,
    m('menu.main.object.displayPerformance', [
      c('id.object.displayPerformance.fastDisplay'),
      c('id.object.displayPerformance.typicalDisplay'),
      c('id.object.displayPerformance.highQualityDisplay'),
      S,
      c('id.object.displayPerformance.useViewSetting'),
    ]),
  ] },
  { get label() { return t('menu.table'); }, mnemonic: 'A', items: [
    c('table.create'),
    c('table.convertText'),
    c('id.table.convertTableToText'),
    S,
    m('menu.main.table.tableOptions', [
      c('id.table.tableOptions.tableSetup'),
      c('id.table.tableOptions.alternatingRowStrokes'),
      c('id.table.tableOptions.alternatingColumnStrokes'),
      c('id.table.tableOptions.alternatingFills'),
      c('id.table.tableOptions.headersAndFooters'),
    ]),
    m('menu.main.table.cellOptions', [
      c('id.table.cellOptions.text'),
      c('id.table.cellOptions.graphic'),
      c('id.table.cellOptions.strokesAndFills'),
      c('id.table.cellOptions.rowsAndColumns'),
      c('id.table.cellOptions.diagonalLines'),
    ]),
    S,
    m('menu.main.table.insert', [
      c('id.table.insert.row'),
      c('id.table.insert.column'),
    ]),
    m('menu.main.table.delete', [
      c('id.table.delete.row'),
      c('id.table.delete.column'),
      c('id.table.delete.table'),
    ]),
    m('menu.main.table.select', [
      c('id.table.select.cell'),
      c('id.table.select.row'),
      c('id.table.select.column'),
      c('id.table.select.table'),
      S,
      c('id.table.select.headerRows'),
      c('id.table.select.bodyRows'),
      c('id.table.select.footerRows'),
    ]),
    S,
    c('table.merge'),
    c('id.table.unmergeCells'),
    c('id.table.splitCellHorizontally'),
    c('id.table.splitCellVertically'),
    c('id.table.pasteBefore'),
    c('id.table.pasteAfter'),
    c('id.table.convertCellToGraphicCell'),
    c('id.table.convertCellToTextCell'),
    m('menu.main.table.convertRows', [
      c('id.table.convertRows.toHeader'),
      c('id.table.convertRows.toBody'),
      c('id.table.convertRows.toFooter'),
    ]),
    S,
    c('id.table.distributeRowsEvenly'),
    c('id.table.distributeColumnsEvenly'),
    S,
    c('id.table.goToRow'),
    c('id.table.editHeader'),
    c('id.table.editFooter'),
  ] },
  { get label() { return t('menu.view'); }, mnemonic: 'V', items: [
    c('view.overprint'),
    S,
    m('menu.main.view.proofSetup', [
      c('id.view.proofSetup.custom'),
      S,
      c('id.view.proofSetup.documentCmyk'),
      c('id.view.proofSetup.workingCmyk'),
    ]),
    c('view.proofColors'),
    S,
    c('view.zoomIn'),
    c('view.zoomOut'),
    c('view.fitPage'),
    c('view.fitSpread'),
    c('view.fitSelection'),
    c('view.actualSize'),
    c('view.pasteboard'),
    S,
    c('id.view.matchPasteboardToThemeColor'),
    S,
    m('menu.main.view.rotateSpread', [
      c('id.view.rotateSpread.90DegCw'),
      c('id.view.rotateSpread.90DegCcw'),
      c('id.view.rotateSpread.180Deg'),
      S,
      c('id.view.rotateSpread.clearRotation'),
    ]),
    S,
    m('menu.main.view.screenMode', [
      c('view.mode.normal'),
      c('view.mode.preview'),
      c('view.mode.bleed'),
      c('view.mode.slug'),
      S,
      c('view.mode.presentation'),
    ]),
    m('menu.main.view.displayPerformance', [
      c('id.view.displayPerformance.fastDisplay'),
      c('id.view.displayPerformance.typicalDisplay'),
      c('id.view.displayPerformance.highQualityDisplay'),
      S,
      c('id.view.displayPerformance.allowObjectLevelDisplaySettings'),
      c('id.view.displayPerformance.clearObjectLevelDisplaySettings'),
    ]),
    S,
    c('view.spreads'),
    c('id.view.showHiddenSpreads'),
    c('view.rulers'),
    S,
    m('menu.main.view.extras', [
      c('view.frameEdges'),
      c('view.threads'),
      S,
      c('id.view.extras.hideNotes'),
      c('id.view.extras.showLinkBadge'),
      S,
      c('view.contentGrabber'),
      c('id.view.extras.hideLiveCorners'),
      c('id.view.extras.hideAnchoredObjectControl'),
      c('id.view.extras.hideConveyor'),
    ]),
    m('menu.main.view.gridsAndGuides', [
      c('view.guides'),
      c('view.lockGuides'),
      c('view.lockColumnGuides'),
      c('view.snapGuides'),
      c('view.smartGuides'),
      c('id.view.gridsAndGuides.deleteAllGuidesOnSpread'),
      S,
      c('view.baselineGrid'),
      c('view.documentGrid'),
      c('id.view.gridsAndGuides.snapToDocumentGrid'),
    ]),
    S,
    m('menu.main.view.storyEditor', [
      c('id.view.storyEditor.showChanges'),
      S,
      c('id.view.storyEditor.showStyleNameColumn'),
      c('id.view.storyEditor.showDepthRuler'),
      c('id.view.storyEditor.showParagraphBreakMarks'),
      S,
      c('id.view.storyEditor.collapseAllFootnotes'),
    ]),
  ] },
  { get label() { return t('menu.window'); }, mnemonic: 'W', items: [
    m('menu.main.window.arrange', [
      c('id.window.arrange.tileAllVertically'),
      c('id.window.arrange.tileAllHorizontally'),
      c('id.window.arrange.2UpHorizontal'),
      c('id.window.arrange.2UpVertical'),
      c('id.window.arrange.3UpHorizontal'),
      c('id.window.arrange.3UpVertical'),
      c('id.window.arrange.3UpStacked'),
      c('id.window.arrange.4Up'),
      c('id.window.arrange.4UpVertical'),
      c('id.window.arrange.4UpHorizontal'),
      c('id.window.arrange.4UpStacked'),
      c('id.window.arrange.5UpStacked'),
      c('id.window.arrange.6Up'),
      c('id.window.arrange.6UpStacked'),
      S,
      c('id.window.arrange.cascade'),
      c('id.window.arrange.tile'),
      S,
      c('id.window.arrange.floatInWindow'),
      c('id.window.arrange.floatAllInWindows'),
      c('id.window.arrange.consolidateAllWindows'),
      S,
      c('id.window.arrange.newWindow'),
      c('id.window.arrange.splitWindow'),
      S,
      c('id.window.arrange.minimize'),
    ]),
    m('menu.main.window.workspace', [
      c('window.workspaceFolio'),
      c('window.workspaceClassic'),
      S,
      c('window.resetWorkspace'),
      c('id.window.workspace.newWorkspace'),
      c('id.window.workspace.deleteWorkspace'),
      S,
      c('window.undoLayout'),
      c('window.redoLayout'),
    ]),
    S,
    m('menu.main.window.color', [
      c('panel.color'),
      c('panel.gradient'),
      c('panel.swatches'),
    ]),
    m('menu.main.window.comments', [
      c('id.window.comments.pdfComments'),
    ]),
    c('window.control'),
    m('menu.main.window.editorial', [
      c('id.window.editorial.notes'),
      c('id.window.editorial.trackChanges'),
    ]),
    c('panel.effects'),
    c('id.window.history'),
    c('panel.info'),
    m('menu.main.window.interactive', [
      c('id.window.interactive.hyperlinks'),
    ]),
    c('panel.layers'),
    c('panel.links'),
    c('id.window.mathExpressions'),
    m('menu.main.window.objectAndLayout', [
      c('panel.align'),
      c('id.window.objectAndLayout.pathfinder'),
      c('panel.transform'),
    ]),
    m('menu.main.window.output', [
      c('id.window.output.attributes'),
      c('id.window.output.flattenerPreview'),
      c('panel.preflight'),
      c('panel.separations'),
      c('id.window.output.trapPresets'),
    ]),
    c('panel.pages'),
    c('panel.project'),
    c('panel.ai'),
    c('id.window.properties'),
    c('panel.stroke'),
    m('menu.main.window.styles', [
      c('id.window.styles.cellStyles'),
      c('panel.characterStyles'),
      c('panel.objectStyles'),
      c('panel.paragraphStyles'),
      c('id.window.styles.tableStyles'),
    ]),
    c('panel.textWrap'),
    c('window.tools'),
    c('window.sidebar'),
    c('window.hidePanels'),
    m('menu.main.window.typeAndTables', [
      c('panel.character'),
      c('id.window.typeAndTables.conditionalText'),
      c('id.window.typeAndTables.crossReferences'),
      c('type.glyphs'),
      c('id.window.typeAndTables.index'),
      c('panel.paragraph'),
      c('type.story'),
      c('id.window.typeAndTables.table'),
    ]),
    m('menu.main.window.utilities', [
      c('id.window.utilities.backgroundTasks'),
      c('id.window.utilities.dataMerge'),
      c('id.window.utilities.scriptLabel'),
      c('id.window.utilities.toolHints'),
    ]),
  ] },
  { get label() { return t('menu.help'); }, mnemonic: 'H', items: [
    c('help.help'),
    c('help.shortcuts'),
    S,
    c('help.limits'),
    m('menu.diagnostics', [
      c('help.credentialStoreTest'),
    ]),
    S,
    c('help.sample'),
    c('help.magazineSample'),
    S,
    c('help.about'),
  ] },
];

/** Panel fly-out menus by Folio panel id. */
export const PANEL_MENUS: Record<string, MenuEntry[]> = {
  // Pages Panel Menu
  pages: [
    c('layout.insertPages'),
    c('layout.movePages'),
    c('id.layout.pages.duplicateSpread'),
    c('id.panel.pages.deleteSpread'),
    c('id.layout.pages.hideSpread'),
    S,
    c('id.layout.pages.printPages'),
    S,
    c('id.panel.pages.newParent'),
    c('id.panel.pages.parentOptions'),
    c('id.layout.pages.applyParentToPages'),
    c('id.panel.pages.overrideAllParentPageItems'),
    m('menu.pagesPanelMenu.parentPages', [
      c('id.panel.pages.parentPages.saveAsParent'),
      c('id.panel.pages.parentPages.loadParentPages'),
      c('id.panel.pages.parentPages.selectUnusedParents'),
      S,
      c('id.panel.pages.parentPages.removeAllLocalOverrides'),
      c('id.panel.pages.parentPages.detachAllObjectsFromParent'),
      c('id.panel.pages.parentPages.allowParentItemOverridesOnSelection'),
      S,
      c('id.panel.pages.parentPages.showParentItems'),
    ]),
    S,
    c('layout.numbering'),
    S,
    c('id.panel.pages.allowDocumentPagesToShuffle'),
    c('id.panel.pages.allowSelectedSpreadToShuffle'),
    S,
    m('menu.pagesPanelMenu.pageAttributes', [
      m('menu.pagesPanelMenu.pageAttributes.colorLabel', [
        c('id.panel.pages.pageAttributes.colorLabel.none'),
        c('id.panel.pages.pageAttributes.colorLabel.useParentColor'),
        S,
        c('id.panel.pages.pageAttributes.colorLabel.white'),
        c('id.panel.pages.pageAttributes.colorLabel.pink'),
        c('id.panel.pages.pageAttributes.colorLabel.gold'),
        c('id.panel.pages.pageAttributes.colorLabel.yellow'),
        c('id.panel.pages.pageAttributes.colorLabel.green'),
        c('id.panel.pages.pageAttributes.colorLabel.lightBlue'),
        c('id.panel.pages.pageAttributes.colorLabel.lavender'),
        c('id.panel.pages.pageAttributes.colorLabel.gray'),
        c('id.panel.pages.pageAttributes.colorLabel.red'),
        c('id.panel.pages.pageAttributes.colorLabel.orange'),
        c('id.panel.pages.pageAttributes.colorLabel.tan'),
        c('id.panel.pages.pageAttributes.colorLabel.darkGreen'),
        c('id.panel.pages.pageAttributes.colorLabel.blue'),
        c('id.panel.pages.pageAttributes.colorLabel.violet'),
        c('id.panel.pages.pageAttributes.colorLabel.black'),
      ]),
      m('menu.pagesPanelMenu.pageAttributes.rotateSpreadView', [
        c('id.view.rotateSpread.90DegCw'),
        c('id.view.rotateSpread.90DegCcw'),
        c('id.view.rotateSpread.180Deg'),
        S,
        c('id.view.rotateSpread.clearRotation'),
      ]),
      m('menu.pagesPanelMenu.pageAttributes.spreadFlattening', [
        c('id.panel.pages.pageAttributes.spreadFlattening.default'),
        S,
        c('id.panel.pages.pageAttributes.spreadFlattening.noneIgnoreTransparency'),
        S,
        c('id.panel.pages.pageAttributes.spreadFlattening.custom'),
      ]),
    ]),
    S,
    m('menu.pagesPanelMenu.viewPages', [
      c('id.panel.pages.viewPages.horizontally'),
      c('id.panel.pages.viewPages.vertically'),
    ]),
    c('view.spreads'),
    c('id.panel.pages.panelOptions'),
  ],
  // Layers Panel Menu
  layers: [
    c('id.panel.layers.newLayer'),
    c('id.panel.layers.duplicateLayer'),
    c('id.panel.layers.deleteLayer'),
    S,
    c('id.panel.layers.layerOptions'),
    S,
    c('id.panel.layers.showAllLayers'),
    c('id.panel.layers.unlockAllLayers'),
    c('id.panel.layers.unlockAll'),
    S,
    c('id.panel.layers.ungroupRemembersLayers'),
    c('id.panel.layers.pasteRemembersLayers'),
    S,
    c('id.panel.layers.mergeLayers'),
    c('id.panel.layers.deleteUnusedLayers'),
    S,
    c('id.panel.layers.selectItemS'),
    c('id.panel.layers.selectAndFitItem'),
    S,
    c('id.panel.layers.smallPanelRows'),
  ],
  // Links Panel Menu
  links: [
    c('id.panel.links.relink'),
    c('id.panel.links.relinkToFolder'),
    c('id.panel.links.relinkFileExtension'),
    c('links.addRoot'),
    c('links.refresh'),
    S,
    c('id.panel.links.updateLink'),
    c('id.panel.links.updateAllLinks'),
    S,
    c('id.panel.links.goToLink'),
    c('id.panel.links.embedLink'),
    c('id.panel.links.linkOptions'),
    S,
    c('id.edit.editOriginal'),
    m('menu.linksPanelMenu.editWith', [
      c('id.list.editWith'),
      S,
      c('id.edit.editWith.other'),
    ]),
    c('id.edit.goToSource'),
    c('id.panel.links.revealInExplorer'),
    S,
    m('menu.linksPanelMenu.utilities', [
      c('id.panel.links.utilities.searchForMissingLinks'),
      c('id.panel.links.utilities.copyLinkSTo'),
      c('id.panel.links.utilities.xmpFileInfo'),
      c('id.panel.links.utilities.autoSizeLinkInformation'),
      c('id.panel.links.utilities.showLinkInformationPane'),
      S,
      c('id.panel.links.utilities.versions'),
    ]),
    m('menu.linksPanelMenu.copyInfo', [
      c('id.panel.links.copyInfo.copyFullPath'),
      c('id.panel.links.copyInfo.copyPlatformStylePath'),
      c('id.panel.links.copyInfo.copyAllLinkInformation'),
      c('id.panel.links.copyInfo.copyInfoForSelectedLinks'),
    ]),
    m('menu.linksPanelMenu.captions', [
      c('id.object.captions.captionSetup'),
      c('id.panel.links.captions.generateLiveCaptions'),
      c('id.panel.links.captions.generateStaticCaptions'),
      c('id.object.captions.convertToStaticCaption'),
    ]),
    S,
    c('id.panel.links.panelOptions'),
  ],
  // Stroke Panel Menu
  stroke: [
    c('id.panel.stroke.showOptions'),
    S,
    c('id.panel.stroke.strokeStyles'),
  ],
  // Color Panel Menu
  color: [
    c('id.panel.color.showOptions'),
    S,
    c('id.panel.color.hsb'),
    c('id.panel.color.lab'),
    c('id.panel.color.cmyk'),
    c('id.panel.color.rgb'),
    S,
    c('id.panel.color.addToSwatches'),
  ],
  // Swatches Panel Menu
  swatches: [
    c('id.panel.swatches.newColorSwatch'),
    c('id.panel.swatches.newTintSwatch'),
    c('id.panel.swatches.newGradientSwatch'),
    c('id.panel.swatches.newMixedInkSwatch'),
    c('id.panel.swatches.newMixedInkGroup'),
    c('id.panel.swatches.newColorGroup'),
    c('id.panel.swatches.duplicateSwatch'),
    c('id.panel.swatches.deleteSwatch'),
    c('id.panel.swatches.ungroupColorGroup'),
    S,
    c('id.panel.swatches.swatchOptions'),
    c('id.panel.swatches.colorGroupOptions'),
    S,
    c('id.panel.swatches.findThisColor'),
    S,
    c('id.panel.swatches.loadSwatches'),
    c('id.panel.swatches.saveSwatches'),
    S,
    c('id.panel.swatches.selectAllUnused'),
    c('id.panel.swatches.addUnnamedColors'),
    S,
    c('id.panel.swatches.largeList'),
    c('id.panel.swatches.smallList'),
    c('id.panel.swatches.smallThumbnail'),
    c('id.panel.swatches.largeThumbnail'),
    S,
    m('menu.swatchesPanelMenu.sort', [
      c('id.panel.swatches.sort.allSwatchesByName'),
      c('id.panel.swatches.sort.allSwatchesByColorValues'),
      c('id.panel.swatches.sort.selectedSwatchesByName'),
      c('id.panel.swatches.sort.selectedSwatchesByColorValues'),
    ]),
    S,
    c('id.panel.swatches.mergeSwatches'),
    S,
    c('id.panel.swatches.inkManager'),
    S,
    c('id.panel.swatches.hideOptions'),
  ],
  // Gradient Panel Menu
  gradient: [
    c('id.panel.gradient.showOptions'),
  ],
  // Info Panel Menu
  info: [
    c('id.panel.info.showOptions'),
    S,
    c('id.panel.info.showContentOffset'),
    S,
    c('file.fileInfo'),
  ],
  // Character Panel Menu
  character: [
    c('id.panel.character.showOptions'),
    S,
    m('menu.characterPanelMenu.opentype', [
      c('id.panel.character.opentype.discretionaryLigatures'),
      c('id.panel.character.opentype.fractions'),
      c('id.panel.character.opentype.ordinal'),
      c('id.panel.character.opentype.swash'),
      c('id.panel.character.opentype.titlingAlternates'),
      c('id.panel.character.opentype.contextualAlternates'),
      c('id.panel.character.opentype.allSmallCaps'),
      c('id.panel.character.opentype.slashedZero'),
      m('menu.characterPanelMenu.opentype.stylisticSets', [
        c('id.panel.character.opentype.stylisticSets.set1'),
        c('id.panel.character.opentype.stylisticSets.set2'),
        c('id.panel.character.opentype.stylisticSets.set3'),
        c('id.panel.character.opentype.stylisticSets.set4'),
        c('id.panel.character.opentype.stylisticSets.set5'),
        c('id.panel.character.opentype.stylisticSets.set6'),
        c('id.panel.character.opentype.stylisticSets.set7'),
        c('id.panel.character.opentype.stylisticSets.set8'),
        c('id.panel.character.opentype.stylisticSets.set9'),
        c('id.panel.character.opentype.stylisticSets.set10'),
        c('id.panel.character.opentype.stylisticSets.set11'),
        c('id.panel.character.opentype.stylisticSets.set12'),
        c('id.panel.character.opentype.stylisticSets.set13'),
        c('id.panel.character.opentype.stylisticSets.set14'),
        c('id.panel.character.opentype.stylisticSets.set15'),
        c('id.panel.character.opentype.stylisticSets.set16'),
        c('id.panel.character.opentype.stylisticSets.set17'),
        c('id.panel.character.opentype.stylisticSets.set18'),
        c('id.panel.character.opentype.stylisticSets.set19'),
        c('id.panel.character.opentype.stylisticSets.set20'),
      ]),
      m('menu.characterPanelMenu.opentype.positionalForms', [
        c('id.panel.character.opentype.positionalForms.generalForm'),
        c('id.panel.character.opentype.positionalForms.automaticForm'),
        c('id.panel.character.opentype.positionalForms.initialForm'),
        c('id.panel.character.opentype.positionalForms.medialForm'),
        c('id.panel.character.opentype.positionalForms.finalForm'),
        c('id.panel.character.opentype.positionalForms.isolatedForm'),
      ]),
      S,
      c('id.panel.character.opentype.superscriptSuperior'),
      c('id.panel.character.opentype.subscriptInferior'),
      c('id.panel.character.opentype.numerator'),
      c('id.panel.character.opentype.denominator'),
      S,
      c('id.panel.character.opentype.tabularLining'),
      c('id.panel.character.opentype.proportionalOldstyle'),
      c('id.panel.character.opentype.proportionalLining'),
      c('id.panel.character.opentype.tabularOldstyle'),
      c('id.panel.character.opentype.defaultFigureStyle'),
    ]),
    c('id.panel.character.allCaps'),
    c('id.panel.character.smallCaps'),
    c('id.panel.character.lowercase'),
    c('id.panel.character.superscript'),
    c('id.panel.character.subscript'),
    S,
    c('id.panel.character.underline'),
    c('id.panel.character.strikethrough'),
    c('id.panel.character.ligatures'),
    S,
    c('id.panel.character.underlineOptions'),
    c('id.panel.character.strikethroughOptions'),
    S,
    c('id.panel.character.noBreak'),
  ],
  // Paragraph Panel Menu
  paragraph: [
    c('id.panel.paragraph.showOptions'),
    S,
    c('id.panel.paragraph.worldReadySingleLineComposer'),
    c('id.panel.paragraph.worldReadyParagraphComposer'),
    c('id.panel.paragraph.paragraphComposer'),
    c('id.panel.paragraph.singleLineComposer'),
    S,
    c('id.panel.paragraph.onlyAlignFirstLineToGrid'),
    c('id.panel.paragraph.balanceRaggedLines'),
    c('id.panel.paragraph.ignoreOpticalMargin'),
    m('menu.autoTateChuYoko', [
      c('para.autoTcyOff'),
      c('para.autoTcy2'),
      c('para.autoTcy3'),
      c('para.autoTcy4'),
      c('para.autoTcyRoman'),
    ]),
    S,
    c('id.panel.paragraph.justification'),
    c('id.panel.paragraph.keepOptions'),
    c('id.panel.paragraph.spanColumns'),
    c('id.panel.paragraph.hyphenation'),
    c('id.panel.paragraph.dropCapsAndNestedStyles'),
    c('id.panel.paragraph.grepStyles'),
    c('id.panel.paragraph.paragraphRules'),
    c('id.panel.paragraph.paragraphBordersAndShading'),
    S,
    c('id.panel.paragraph.bulletsAndNumbering'),
    c('id.type.bulletedAndNumberedLists.restartContinueNumbering'),
    c('id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText'),
    c('id.type.bulletedAndNumberedLists.defineLists'),
  ],
  // Paragraph Style Panel Menu
  paragraphStyles: [
    c('id.panel.paragraphStyles.newParagraphStyle'),
    c('id.panel.paragraphStyles.duplicateStyle'),
    c('id.panel.paragraphStyles.deleteStyles'),
    S,
    c('id.panel.paragraphStyles.redefineStyle'),
    S,
    c('id.panel.paragraphStyles.styleOptions'),
    S,
    c('id.panel.paragraphStyles.clearOverrides'),
    c('id.panel.paragraphStyles.toggleStyleOverrideHighlighter'),
    c('id.panel.paragraphStyles.convertStyleSBulletsAndNumberingToText'),
    c('id.panel.paragraphStyles.breakLinkToStyle'),
    S,
    c('id.panel.paragraphStyles.loadParagraphStyles'),
    c('id.panel.paragraphStyles.loadAllTextStyles'),
    S,
    c('id.panel.paragraphStyles.selectAllUnused'),
    S,
    c('id.panel.paragraphStyles.newStyleGroup'),
    c('id.panel.paragraphStyles.openAllStyleGroups'),
    c('id.panel.paragraphStyles.closeAllStyleGroups'),
    S,
    c('id.panel.paragraphStyles.copyToGroup'),
    c('id.panel.paragraphStyles.newGroupFromStyles'),
    S,
    c('id.panel.paragraphStyles.sortByName'),
    S,
    c('id.panel.paragraphStyles.smallPanelRows'),
  ],
  // Character Style Panel Menu
  characterStyles: [
    c('id.panel.characterStyles.newCharacterStyle'),
    c('id.panel.characterStyles.duplicateStyle'),
    c('id.panel.characterStyles.deleteStyle'),
    S,
    c('id.panel.characterStyles.redefineStyle'),
    c('id.panel.characterStyles.toggleStyleOverrideHighlighter'),
    S,
    c('id.panel.characterStyles.styleOptions'),
    S,
    c('id.panel.characterStyles.breakLinkToStyle'),
    S,
    c('id.panel.characterStyles.loadCharacterStyles'),
    c('id.panel.paragraphStyles.loadAllTextStyles'),
    S,
    c('id.panel.characterStyles.selectAllUnused'),
    S,
    c('id.panel.characterStyles.newStyleGroup'),
    c('id.panel.characterStyles.openAllStyleGroups'),
    c('id.panel.characterStyles.closeAllStyleGroups'),
    S,
    c('id.panel.characterStyles.copyToGroup'),
    c('id.panel.characterStyles.newGroupFromStyles'),
    S,
    c('id.panel.characterStyles.sortByName'),
    S,
    c('id.panel.characterStyles.smallPanelRows'),
  ],
  // Object Styles Panel Menu
  objectStyles: [
    c('id.panel.objectStyles.newObjectStyle'),
    c('id.panel.objectStyles.duplicateObjectStyle'),
    c('id.panel.objectStyles.deleteStyles'),
    S,
    c('id.panel.objectStyles.redefineStyle'),
    S,
    c('id.panel.objectStyles.styleOptions'),
    S,
    c('id.panel.objectStyles.clearOverrides'),
    c('id.panel.objectStyles.clearOverridesWhenApplyingStyle'),
    c('id.panel.objectStyles.clearAttributesNotDefinedByStyle'),
    c('id.panel.objectStyles.breakLinkToStyle'),
    S,
    m('menu.objectStylesPanelMenu.defaultTextFrameStyle', [
      c('id.panel.objectStyles.defaultTextFrameStyle.none'),
      c('id.panel.objectStyles.defaultTextFrameStyle.basicGraphicsFrame'),
      c('id.panel.objectStyles.defaultTextFrameStyle.basicTextFrame'),
    ]),
    m('menu.objectStylesPanelMenu.defaultGraphicFrameStyle', [
      c('id.panel.objectStyles.defaultGraphicFrameStyle.none'),
      c('id.panel.objectStyles.defaultGraphicFrameStyle.basicGraphicsFrame'),
      c('id.panel.objectStyles.defaultGraphicFrameStyle.basicTextFrame'),
    ]),
    S,
    c('id.panel.objectStyles.loadObjectStyles'),
    S,
    c('id.panel.objectStyles.selectAllUnused'),
    S,
    c('id.panel.objectStyles.newStyleGroup'),
    c('id.panel.objectStyles.openAllGroups'),
    c('id.panel.objectStyles.closeAllGroups'),
    S,
    c('id.panel.objectStyles.copyToGroup'),
    c('id.panel.objectStyles.newGroupFromStyles'),
    S,
    c('id.panel.objectStyles.sortByName'),
    S,
    c('id.panel.objectStyles.smallPanelRows'),
  ],
  // Align Panel Menu
  align: [
    c('id.panel.align.showOptions'),
    S,
    c('id.panel.align.cancelKeyObject'),
  ],
  // Transform Panel Menu
  transform: [
    c('id.object.transform.clearTransformations'),
    c('id.panel.transform.redefineScalingAs100'),
    S,
    c('id.object.transform.rotate90DegCw'),
    c('id.object.transform.rotate90DegCcw'),
    c('id.object.transform.rotate180Deg'),
    S,
    c('id.object.transform.flipHorizontal'),
    c('id.object.transform.flipVertical'),
    S,
    c('id.panel.transform.dimensionsIncludeStrokeWeight'),
    c('id.panel.transform.transformationsAreTotals'),
    c('id.panel.info.showContentOffset'),
    c('id.panel.transform.adjustStrokeWeightWhenScaling'),
    c('id.panel.transform.adjustEffectsWhenScaling'),
  ],
  // Text Wrap Panel Menu
  textWrap: [
    c('id.panel.textWrap.showOptions'),
    c('id.panel.textWrap.applyToParentPageOnly'),
  ],
  // Effects Panel Menu
  effects: [
    c('id.panel.effects.showOptions'),
    m('menu.effectsPanelMenu.effects', [
      c('id.object.effects.transparency'),
      S,
      c('object.dropShadow'),
      c('id.object.effects.innerShadow'),
      c('id.object.effects.outerGlow'),
      c('id.object.effects.innerGlow'),
      c('id.object.effects.bevelAndEmboss'),
      c('id.object.effects.satin'),
      c('id.object.effects.basicFeather'),
      c('id.object.effects.directionalFeather'),
      c('id.object.effects.gradientFeather'),
    ]),
    S,
    c('id.object.effects.clearEffects'),
    c('id.object.effects.clearAllTransparency'),
    S,
    c('id.object.effects.globalLight'),
  ],
  // Preflight Panel Menu
  preflight: [
    c('preflight.rerun'),
    S,
    c('id.panel.preflight.defineProfiles'),
    c('id.panel.preflight.preflightOptions'),
    S,
    c('id.panel.preflight.saveReport'),
    S,
    m('menu.preflightPanelMenu.limitNumberOfRowsPerError', [
      c('id.panel.preflight.limitNumberOfRowsPerError.25'),
      c('id.panel.preflight.limitNumberOfRowsPerError.50'),
      c('id.panel.preflight.limitNumberOfRowsPerError.100'),
      c('id.panel.preflight.limitNumberOfRowsPerError.noLimit'),
    ]),
    S,
    c('id.panel.preflight.enablePreflightForAllDocuments'),
  ],
  // Separations Preview Panel Menu
  separations: [
    c('id.panel.separations.inkManager'),
    c('id.panel.separations.showSinglePlatesInBlack'),
    c('id.panel.separations.desaturateBlack'),
    S,
    c('view.overprint'),
  ],
  // Table Styles Panel Menu
  tableStyles: [
    c('id.panel.tableStyles.newTableStyle'),
    c('id.panel.tableStyles.duplicateStyle'),
    c('id.panel.tableStyles.deleteStyle'),
    S,
    c('id.panel.tableStyles.redefineStyle'),
    S,
    c('id.panel.tableStyles.styleOptions'),
    S,
    c('id.panel.tableStyles.clearOverrides'),
    c('id.panel.tableStyles.breakLinkToStyle'),
    S,
    c('id.panel.tableStyles.loadTableStyles'),
    c('id.panel.tableStyles.loadTableAndCellStyles'),
    S,
    c('id.panel.tableStyles.selectAllUnused'),
    S,
    c('id.panel.tableStyles.newStyleGroup'),
    c('id.panel.tableStyles.openAllStyleGroups'),
    c('id.panel.tableStyles.closeAllStyleGroups'),
    S,
    c('id.panel.tableStyles.copyToGroup'),
    c('id.panel.tableStyles.newGroupFromStyles'),
    S,
    c('id.panel.tableStyles.sortByName'),
    S,
    c('id.panel.tableStyles.smallPanelRows'),
  ],
  // Cell Styles Panel Menu
  cellStyles: [
    c('id.panel.cellStyles.newCellStyle'),
    c('id.panel.cellStyles.duplicateStyle'),
    c('id.panel.cellStyles.deleteStyle'),
    S,
    c('id.panel.cellStyles.redefineStyle'),
    S,
    c('id.panel.cellStyles.styleOptions'),
    S,
    c('id.panel.cellStyles.clearOverrides'),
    c('id.panel.cellStyles.clearAttributesNotDefinedByStyle'),
    c('id.panel.cellStyles.breakLinkToStyle'),
    S,
    c('id.panel.cellStyles.loadCellStyles'),
    c('id.panel.cellStyles.loadTableAndCellStyles'),
    S,
    c('id.panel.cellStyles.selectAllUnused'),
    S,
    c('id.panel.cellStyles.newStyleGroup'),
    c('id.panel.cellStyles.openAllStyleGroups'),
    c('id.panel.cellStyles.closeAllStyleGroups'),
    S,
    c('id.panel.cellStyles.copyToGroup'),
    c('id.panel.cellStyles.newGroupFromStyles'),
    S,
    c('id.panel.cellStyles.sortByName'),
    S,
    c('id.panel.cellStyles.smallPanelRows'),
  ],
  // Hyperlinks Panel Menu
  hyperlinks: [
    c('id.type.hyperlinksAndCrossReferences.newHyperlink'),
    c('id.panel.hyperlinks.deleteHyperlink'),
    S,
    c('id.panel.hyperlinks.newHyperlinkDestination'),
    S,
    c('id.type.hyperlinksAndCrossReferences.editHyperlink'),
    c('id.panel.hyperlinks.hyperlinkDestinationOptions'),
    c('id.panel.hyperlinks.deleteUnusedDestinations'),
    S,
    c('id.panel.hyperlinks.goToSource'),
    c('id.panel.hyperlinks.goToDestination'),
    c('id.panel.hyperlinks.renameHyperlink'),
    c('id.panel.hyperlinks.resetHyperlink'),
    S,
    m('menu.hyperlinksPanelMenu.sort', [
      c('id.panel.hyperlinks.sort.manually'),
      c('id.panel.hyperlinks.sort.byName'),
      c('id.panel.hyperlinks.sort.byType'),
    ]),
  ],
  // Cross-Reference Panel Menu
  crossReferences: [
    c('id.type.hyperlinksAndCrossReferences.insertCrossReference'),
    c('id.type.hyperlinksAndCrossReferences.crossReferenceOptions'),
    c('id.panel.crossReferences.deleteCrossReference'),
    c('id.type.hyperlinksAndCrossReferences.updateCrossReference'),
    c('id.panel.crossReferences.relinkCrossReference'),
    S,
    c('id.panel.hyperlinks.newHyperlinkDestination'),
    c('id.panel.hyperlinks.hyperlinkDestinationOptions'),
    S,
    c('id.panel.hyperlinks.goToSource'),
    c('id.panel.hyperlinks.goToDestination'),
    S,
    c('id.panel.crossReferences.defineCrossReferenceFormats'),
    S,
    m('menu.crossReferencePanelMenu.sort', [
      c('id.panel.hyperlinks.sort.manually'),
      c('id.panel.hyperlinks.sort.byName'),
      c('id.panel.hyperlinks.sort.byType'),
    ]),
  ],
  // Glyphs Panel Menu
  glyphs: [
    c('id.panel.glyphs.newGlyphSet'),
    c('glyphs.addToSet'),
    c('glyphs.editSet'),
    c('glyphs.viewSet'),
    c('glyphs.deleteSet'),
    S,
    c('glyphs.importSets'),
    c('glyphs.exportSets'),
    S,
    m('menu.glyphsPanelMenu.sortGlyphs', [
      c('id.panel.glyphs.sortGlyphs.byCidGid'),
      c('id.panel.glyphs.sortGlyphs.byUnicode'),
    ]),
  ],
  // Index Panel Menu
  index: [
    c('id.panel.index.new'),
    c('id.panel.index.duplicate'),
    c('id.panel.index.delete'),
    S,
    c('id.panel.index.importTopics'),
    c('id.panel.index.removeUnusedTopics'),
    S,
    c('id.panel.index.goToSelectedMarker'),
    c('id.panel.index.options'),
    S,
    c('id.panel.index.capitalize'),
    c('id.panel.index.sortOptions'),
    c('id.panel.index.updatePreview'),
    S,
    c('id.panel.index.generateIndex'),
    S,
    c('id.panel.index.showFindField'),
    c('id.panel.index.showUnusedTopics'),
  ],
  // Conditional Text Panel Menu
  conditionalText: [
    c('id.panel.conditionalText.newCondition'),
    c('id.panel.conditionalText.deleteCondition'),
    S,
    c('id.panel.conditionalText.conditionOptions'),
    S,
    c('id.panel.conditionalText.showAll'),
    c('id.panel.conditionalText.hideAll'),
    S,
    c('id.panel.conditionalText.loadConditions'),
    c('id.panel.conditionalText.loadConditionsAndSets'),
    S,
    c('id.panel.conditionalText.selectAllUnused'),
  ],
  // Data Merge Panel Menu
  dataMerge: [
    c('id.panel.dataMerge.selectDataSource'),
    c('dataMerge.connectDatabase'),
    c('id.panel.dataMerge.exportToPdf'),
    c('id.panel.dataMerge.updateDataSource'),
    c('id.panel.dataMerge.removeDataSource'),
    S,
    c('id.panel.dataMerge.preview'),
    c('id.panel.dataMerge.createMergedDocument'),
    c('id.panel.dataMerge.updateContentInDataFields'),
    S,
    c('id.panel.dataMerge.contentPlacementOptions'),
    S,
    c('id.panel.dataMerge.showLogOfUpdateResults'),
  ],
  // Trap Presets Panel Menu
  trapPresets: [
    c('id.panel.trapPresets.newPreset'),
    c('id.panel.trapPresets.duplicatePreset'),
    c('id.panel.trapPresets.deletePresets'),
    S,
    c('id.panel.trapPresets.presetOptions'),
    c('id.panel.trapPresets.assignTrapPreset'),
    S,
    c('id.panel.trapPresets.loadTrapPresets'),
    c('id.panel.trapPresets.selectAllUnused'),
    S,
    c('id.panel.trapPresets.smallPanelRows'),
  ],
  // History Panel Menu
  history: [
    c('edit.redo'),
    c('edit.undo'),
    S,
    c('id.panel.history.delete'),
    c('id.panel.history.clearHistory'),
    S,
    c('id.panel.history.newDocument'),
  ],
  // Notes Panel Menu
  notes: [
    c('id.type.notes.newNote'),
    c('id.type.notes.openNote'),
    c('id.type.notes.deleteNote'),
    S,
    c('id.type.notes.previousNote'),
    c('id.type.notes.nextNote'),
    S,
    c('id.type.notes.convertToNote'),
    c('id.type.notes.convertToText'),
    S,
    c('id.type.notes.splitNote'),
    S,
    c('id.type.notes.expandCollapseNotesInStory'),
    S,
    c('id.type.notes.removeNotesFromStory'),
    c('id.type.notes.removeAllNotes'),
  ],
  // Flattener Preview Panel Menu
  flattenerPreview: [
    c('id.edit.transparencyFlattenerPresets'),
  ],
  // Book Panel Menu
  book: [
    c('id.panel.book.addDocument'),
    c('id.panel.book.replaceDocument'),
    c('id.panel.book.removeDocument'),
    S,
    c('id.panel.book.saveBook'),
    c('id.panel.book.saveBookAs'),
    c('id.panel.book.closeBook'),
    S,
    c('id.panel.book.preflightBook'),
    c('id.panel.book.packageBookForPrint'),
    c('id.panel.book.exportBookToPdf'),
    c('id.panel.book.printBook'),
    S,
    c('id.panel.book.documentInformation'),
    c('id.panel.book.revealInExplorer'),
    S,
    c('id.panel.book.synchronizeBook'),
    c('id.panel.book.synchronizeOptions'),
    c('id.panel.book.updateAllCrossReferences'),
    S,
    m('menu.bookPanelMenu.updateNumbering', [
      c('id.panel.book.updateNumbering.updatePageAndSectionNumbers'),
      c('id.panel.book.updateNumbering.updateChapterAndParagraphNumbers'),
      c('id.panel.book.updateNumbering.updateAllNumbers'),
    ]),
    c('id.panel.book.bookPageNumberingOptions'),
    c('id.panel.book.documentNumberingOptions'),
    S,
    c('id.panel.book.mergeIdenticallyNamedLayersOnExport'),
  ],
  // Library Panel Menu
  library: [
    c('id.panel.library.closeLibrary'),
    S,
    c('id.panel.library.addItem'),
    c('id.panel.library.addItemsOnPage'),
    c('id.panel.library.addItemsOnPageAsSeparateObjects'),
    c('id.panel.library.placeItemS'),
    c('id.panel.library.deleteItemS'),
    c('id.panel.library.updateLibraryItem'),
    S,
    c('id.panel.library.itemInformation'),
    S,
    c('id.panel.library.showSubset'),
    c('id.panel.library.showAll'),
    S,
    c('id.panel.library.listView'),
    S,
    m('menu.libraryPanelMenu.sortItems', [
      c('id.panel.library.sortItems.byName'),
      c('id.panel.library.sortItems.byNewest'),
      c('id.panel.library.sortItems.byOldest'),
      c('id.panel.library.sortItems.byType'),
    ]),
  ],
};

/** Canvas right-click menus. */
export const CONTEXT_MENUS: Record<string, MenuEntry[]> = {
  // layoutNone: nothing selected (General Context Menu; object-level display items removed)
  layoutNone: [
    c('edit.paste'),
    c('edit.pasteInPlace'),
    S,
    c('view.zoomIn'),
    c('view.zoomOut'),
    c('view.actualSize'),
    c('view.pasteboard'),
    S,
    c('id.view.matchPasteboardToThemeColor'),
    S,
    m('menu.generalContextMenu.select', [
      c('object.selectAbove'),
      c('object.selectNextAbove'),
      c('object.selectNextBelow'),
      c('object.selectBelow'),
    ]),
    S,
    c('view.rulers'),
    S,
    m('menu.generalContextMenu.gridsAndGuides', [
      c('view.guides'),
      c('view.lockGuides'),
      c('view.lockColumnGuides'),
      c('view.snapGuides'),
      c('view.smartGuides'),
      c('id.ctx.layoutNone.gridsAndGuides.guidesInBack'),
      c('id.view.gridsAndGuides.deleteAllGuidesOnSpread'),
      S,
      c('view.baselineGrid'),
      c('view.documentGrid'),
      c('id.view.gridsAndGuides.snapToDocumentGrid'),
      c('id.ctx.layoutNone.gridsAndGuides.gridsInBack'),
    ]),
    S,
    m('menu.generalContextMenu.displayPerformance', [
      c('id.view.displayPerformance.fastDisplay'),
      c('id.view.displayPerformance.typicalDisplay'),
      c('id.view.displayPerformance.highQualityDisplay'),
      S,
      c('id.view.displayPerformance.allowObjectLevelDisplaySettings'),
      c('id.view.displayPerformance.clearObjectLevelDisplaySettings'),
    ]),
  ],
  // layoutObject: frame/object selected (Layout Context Menu without graphic-content and view-level display items)
  layoutObject: [
    c('edit.cut'),
    c('edit.copy'),
    c('edit.paste'),
    c('edit.pasteInto'),
    c('edit.pasteInPlace'),
    S,
    m('menu.layoutContextMenu.zoom', [
      c('view.zoomIn'),
      c('view.zoomOut'),
      c('view.actualSize'),
      c('view.pasteboard'),
      c('view.fitSelection'),
    ]),
    S,
    c('object.textFrameOptions'),
    m('menu.layoutContextMenu.anchoredObject', [
      c('id.object.anchoredObject.options'),
      c('id.object.anchoredObject.release'),
    ]),
    c('type.placeholder'),
    c('edit.storyEditor'),
    S,
    c('id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText'),
    c('id.panel.paragraph.bulletsAndNumbering'),
    S,
    m('menu.layoutContextMenu.transform', [
      c('object.move'),
      c('id.object.transform.scale'),
      c('id.object.transform.rotate'),
      c('id.object.transform.shear'),
      S,
      c('id.object.transform.rotate90DegCw'),
      c('id.object.transform.rotate90DegCcw'),
      c('id.object.transform.rotate180Deg'),
      S,
      c('id.object.transform.flipHorizontal'),
      c('id.object.transform.flipVertical'),
      S,
      c('id.object.transform.clearTransformations'),
    ]),
    m('menu.layoutContextMenu.transformAgain', [
      c('object.transformAgain'),
      c('id.object.transformAgain.transformAgainIndividually'),
      c('object.transformSequenceAgain'),
      c('id.object.transformAgain.transformSequenceAgainIndividually'),
    ]),
    m('menu.layoutContextMenu.arrange', [
      c('object.bringFront'),
      c('object.forward'),
      c('object.backward'),
      c('object.sendBack'),
    ]),
    m('menu.layoutContextMenu.select', [
      c('object.selectAbove'),
      c('object.selectNextAbove'),
      c('object.selectNextBelow'),
      c('object.selectBelow'),
      S,
      c('object.selectContainer'),
      c('object.selectContent'),
      c('object.selectPrevInGroup'),
      c('object.selectNextInGroup'),
    ]),
    S,
    c('object.group'),
    c('object.ungroup'),
    S,
    c('object.lock'),
    c('object.hide'),
    c('object.showAll'),
    c('id.ctx.layoutObject.allowParentItemOverrides'),
    c('id.ctx.layoutObject.primaryTextFrame'),
    m('menu.layoutContextMenu.strokeWeight', [
      c('id.ctx.layoutObject.strokeWeight.0Pt'),
      c('id.ctx.layoutObject.strokeWeight.025Pt'),
      c('id.ctx.layoutObject.strokeWeight.05Pt'),
      c('id.ctx.layoutObject.strokeWeight.075Pt'),
      c('id.ctx.layoutObject.strokeWeight.1Pt'),
      c('id.ctx.layoutObject.strokeWeight.2Pt'),
      c('id.ctx.layoutObject.strokeWeight.3Pt'),
      c('id.ctx.layoutObject.strokeWeight.4Pt'),
      c('id.ctx.layoutObject.strokeWeight.5Pt'),
      c('id.ctx.layoutObject.strokeWeight.6Pt'),
      c('id.ctx.layoutObject.strokeWeight.7Pt'),
      c('id.ctx.layoutObject.strokeWeight.8Pt'),
      c('id.ctx.layoutObject.strokeWeight.9Pt'),
      c('id.ctx.layoutObject.strokeWeight.10Pt'),
    ]),
    S,
    m('menu.layoutContextMenu.fitting', [
      c('object.fillProportional'),
      c('object.fitProportional'),
      S,
      c('object.fitFrameContent'),
      c('object.fitContentFrame'),
      c('object.centerContent'),
      S,
      c('id.object.fitting.clearFrameFittingOptions'),
      S,
      c('object.fittingOptions'),
    ]),
    m('menu.layoutContextMenu.content', [
      c('id.object.content.graphic'),
      c('id.object.content.text'),
      c('id.object.content.unassigned'),
    ]),
    S,
    m('menu.layoutContextMenu.effects', [
      c('id.object.effects.transparency'),
      S,
      c('object.dropShadow'),
      c('id.object.effects.innerShadow'),
      c('id.object.effects.outerGlow'),
      c('id.object.effects.innerGlow'),
      c('id.object.effects.bevelAndEmboss'),
      c('id.object.effects.satin'),
      c('id.object.effects.basicFeather'),
      c('id.object.effects.directionalFeather'),
      c('id.object.effects.gradientFeather'),
    ]),
    c('object.layerOptions'),
    c('id.object.clippingPath.options'),
    c('id.object.clippingPath.convertClippingPathToFrame'),
    m('menu.layoutContextMenu.captions', [
      c('id.object.captions.captionSetup'),
      c('id.object.captions.generateLiveCaption'),
      c('id.object.captions.generateStaticCaption'),
      c('id.object.captions.convertToStaticCaption'),
    ]),
    m('menu.layoutContextMenu.hyperlinks', [
      c('id.type.hyperlinksAndCrossReferences.newHyperlink'),
      c('id.type.hyperlinksAndCrossReferences.editHyperlink'),
      c('id.panel.hyperlinks.goToDestination'),
    ]),
    c('id.object.generateQrCode'),
    c('id.object.insertMathml'),
    S,
    m('menu.layoutContextMenu.displayPerformance', [
      c('id.object.displayPerformance.fastDisplay'),
      c('id.object.displayPerformance.typicalDisplay'),
      c('id.object.displayPerformance.highQualityDisplay'),
      S,
      c('id.object.displayPerformance.useViewSetting'),
    ]),
  ],
  // layoutGraphic: graphic content selected (Layout Context Menu: edit/transform/fitting/effects/captions/links items)
  layoutGraphic: [
    c('edit.cut'),
    c('edit.copy'),
    c('edit.paste'),
    c('edit.pasteInto'),
    c('edit.pasteInPlace'),
    S,
    m('menu.layoutContextMenu.zoom', [
      c('view.zoomIn'),
      c('view.zoomOut'),
      c('view.actualSize'),
      c('view.pasteboard'),
      c('view.fitSelection'),
    ]),
    S,
    m('menu.layoutContextMenu.transform', [
      c('object.move'),
      c('id.object.transform.scale'),
      c('id.object.transform.rotate'),
      c('id.object.transform.shear'),
      S,
      c('id.object.transform.rotate90DegCw'),
      c('id.object.transform.rotate90DegCcw'),
      c('id.object.transform.rotate180Deg'),
      S,
      c('id.object.transform.flipHorizontal'),
      c('id.object.transform.flipVertical'),
      S,
      c('id.object.transform.clearTransformations'),
    ]),
    m('menu.layoutContextMenu.transformAgain', [
      c('object.transformAgain'),
      c('id.object.transformAgain.transformAgainIndividually'),
      c('object.transformSequenceAgain'),
      c('id.object.transformAgain.transformSequenceAgainIndividually'),
    ]),
    m('menu.layoutContextMenu.arrange', [
      c('object.bringFront'),
      c('object.forward'),
      c('object.backward'),
      c('object.sendBack'),
    ]),
    m('menu.layoutContextMenu.select', [
      c('object.selectAbove'),
      c('object.selectNextAbove'),
      c('object.selectNextBelow'),
      c('object.selectBelow'),
      S,
      c('object.selectContainer'),
      c('object.selectContent'),
      c('object.selectPrevInGroup'),
      c('object.selectNextInGroup'),
    ]),
    S,
    c('object.lock'),
    c('object.hide'),
    c('object.showAll'),
    S,
    m('menu.layoutContextMenu.fitting', [
      c('object.fillProportional'),
      c('object.fitProportional'),
      S,
      c('object.fitFrameContent'),
      c('object.fitContentFrame'),
      c('object.centerContent'),
      S,
      c('id.object.fitting.clearFrameFittingOptions'),
      S,
      c('object.fittingOptions'),
    ]),
    S,
    m('menu.layoutContextMenu.effects', [
      c('id.object.effects.transparency'),
      S,
      c('object.dropShadow'),
      c('id.object.effects.innerShadow'),
      c('id.object.effects.outerGlow'),
      c('id.object.effects.innerGlow'),
      c('id.object.effects.bevelAndEmboss'),
      c('id.object.effects.satin'),
      c('id.object.effects.basicFeather'),
      c('id.object.effects.directionalFeather'),
      c('id.object.effects.gradientFeather'),
    ]),
    c('object.layerOptions'),
    c('id.object.clippingPath.options'),
    c('id.object.clippingPath.convertClippingPathToFrame'),
    m('menu.layoutContextMenu.captions', [
      c('id.object.captions.captionSetup'),
      c('id.object.captions.generateLiveCaption'),
      c('id.object.captions.generateStaticCaption'),
      c('id.object.captions.convertToStaticCaption'),
    ]),
    c('id.edit.editOriginal'),
    m('menu.layoutContextMenu.editWith', [
      c('id.list.editWith'),
      S,
      c('id.edit.editWith.other'),
    ]),
    c('id.edit.goToSource'),
    m('menu.layoutContextMenu.hyperlinks', [
      c('id.type.hyperlinksAndCrossReferences.newHyperlink'),
      c('id.type.hyperlinksAndCrossReferences.editHyperlink'),
      c('id.panel.hyperlinks.goToDestination'),
    ]),
    c('id.object.generateQrCode'),
    S,
    m('menu.layoutContextMenu.displayPerformance', [
      c('id.object.displayPerformance.fastDisplay'),
      c('id.object.displayPerformance.typicalDisplay'),
      c('id.object.displayPerformance.highQualityDisplay'),
      S,
      c('id.object.displayPerformance.useViewSetting'),
    ]),
    S,
    m('menu.layoutContextMenu.graphics', [
      c('id.panel.links.revealInExplorer'),
      c('id.object.imageColorSettings'),
    ]),
  ],
  // text: text editing (Text Context Menu without its table rows)
  text: [
    c('edit.cut'),
    c('edit.copy'),
    c('edit.paste'),
    c('edit.pasteNoFormat'),
    S,
    m('menu.textContextMenu.font', [
      c('id.list.fonts'),
    ]),
    m('menu.textContextMenu.size', [
      c('id.type.size.other'),
      c('id.type.size.6Pt'),
      c('id.type.size.8Pt'),
      c('id.type.size.9Pt'),
      c('id.type.size.10Pt'),
      c('id.type.size.11Pt'),
      c('id.type.size.12Pt'),
      c('id.type.size.14Pt'),
      c('id.type.size.18Pt'),
      c('id.type.size.24Pt'),
      c('id.type.size.30Pt'),
      c('id.type.size.36Pt'),
      c('id.type.size.48Pt'),
      c('id.type.size.60Pt'),
      c('id.type.size.72Pt'),
    ]),
    c('id.ctx.text.clearAllOverrides'),
    c('id.type.bulletedAndNumberedLists.restartContinueNumbering'),
    c('id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText'),
    c('id.panel.paragraph.bulletsAndNumbering'),
    S,
    c('edit.findChange'),
    c('id.ctx.text.loadSelectedGlyphInFind'),
    c('id.ctx.text.loadSelectedGlyphInChange'),
    S,
    m('menu.textContextMenu.spelling', [
      c('edit.spelling'),
      c('id.edit.spelling.dynamicSpelling'),
      c('id.edit.spelling.autocorrect'),
      c('id.edit.spelling.userDictionary'),
    ]),
    c('object.textFrameOptions'),
    m('menu.textContextMenu.anchoredObject', [
      c('id.object.anchoredObject.options'),
      c('id.object.anchoredObject.insert'),
      c('id.object.anchoredObject.release'),
    ]),
    S,
    m('menu.textContextMenu.changeCase', [
      c('id.type.changeCase.uppercase'),
      c('id.type.changeCase.lowercase'),
      c('id.type.changeCase.titleCase'),
      c('id.type.changeCase.sentenceCase'),
    ]),
    c('type.hiddenChars'),
    S,
    m('menu.textContextMenu.typeOnAPath', [
      c('id.type.typeOnAPath.options'),
      c('id.type.typeOnAPath.deleteTypeFromPath'),
    ]),
    S,
    c('id.type.insertFootnote'),
    c('id.ctx.text.goToFootnoteText'),
    c('id.view.storyEditor.collapseAllFootnotes'),
    c('id.type.insertEndnote'),
    c('id.ctx.text.goToEndnoteText'),
    S,
    m('menu.textContextMenu.insertVariable', [
      c('id.type.textVariables.insertVariable.chapterNumber'),
      c('id.type.textVariables.insertVariable.creationDate'),
      c('id.type.textVariables.insertVariable.fileName'),
      c('id.type.textVariables.insertVariable.imageName'),
      c('id.type.textVariables.insertVariable.lastPageNumber'),
      c('id.type.textVariables.insertVariable.modificationDate'),
      c('id.type.textVariables.insertVariable.outputDate'),
      c('id.type.textVariables.insertVariable.runningHeader'),
    ]),
    c('id.type.textVariables.convertVariableToText'),
    c('id.object.captions.convertToStaticCaption'),
    S,
    m('menu.textContextMenu.insertSpecialCharacter', [
      m('menu.textContextMenu.insertSpecialCharacter.symbols', [
        c('id.type.insertSpecialCharacter.symbols.bulletCharacter'),
        c('id.type.insertSpecialCharacter.symbols.copyrightSymbol'),
        c('id.type.insertSpecialCharacter.symbols.ellipsis'),
        c('id.type.insertSpecialCharacter.symbols.paragraphSymbol'),
        c('id.type.insertSpecialCharacter.symbols.registeredTrademarkSymbol'),
        c('id.type.insertSpecialCharacter.symbols.sectionSymbol'),
        c('id.type.insertSpecialCharacter.symbols.trademarkSymbol'),
      ]),
      m('menu.textContextMenu.insertSpecialCharacter.markers', [
        c('id.type.insertSpecialCharacter.markers.currentPageNumber'),
        c('id.type.insertSpecialCharacter.markers.nextPageNumber'),
        c('id.type.insertSpecialCharacter.markers.previousPageNumber'),
        c('id.type.insertSpecialCharacter.markers.sectionMarker'),
        c('id.type.insertSpecialCharacter.markers.footnoteNumber'),
      ]),
      m('menu.textContextMenu.insertSpecialCharacter.hyphensAndDashes', [
        c('id.type.insertSpecialCharacter.hyphensAndDashes.emDash'),
        c('id.type.insertSpecialCharacter.hyphensAndDashes.enDash'),
        c('id.type.insertSpecialCharacter.hyphensAndDashes.discretionaryHyphen'),
        c('id.type.insertSpecialCharacter.hyphensAndDashes.nonbreakingHyphen'),
      ]),
      m('menu.textContextMenu.insertSpecialCharacter.quotationMarks', [
        c('id.type.insertSpecialCharacter.quotationMarks.doubleLeftQuotationMarks'),
        c('id.type.insertSpecialCharacter.quotationMarks.doubleRightQuotationMarks'),
        c('id.type.insertSpecialCharacter.quotationMarks.singleLeftQuotationMark'),
        c('id.type.insertSpecialCharacter.quotationMarks.singleRightQuotationMark'),
        c('id.type.insertSpecialCharacter.quotationMarks.straightDoubleQuotationMarks'),
        c('id.type.insertSpecialCharacter.quotationMarks.straightSingleQuotationMarkApostrophe'),
      ]),
      m('menu.textContextMenu.insertSpecialCharacter.other', [
        c('id.type.insertSpecialCharacter.other.tab'),
        c('id.type.insertSpecialCharacter.other.rightIndentTab'),
        c('id.type.insertSpecialCharacter.other.indentToHere'),
        c('id.type.insertSpecialCharacter.other.endNestedStyleHere'),
        c('id.type.insertSpecialCharacter.other.nonJoiner'),
      ]),
    ]),
    m('menu.textContextMenu.insertWhiteSpace', [
      c('id.type.insertWhiteSpace.emSpace'),
      c('id.type.insertWhiteSpace.enSpace'),
      S,
      c('id.type.insertWhiteSpace.nonbreakingSpace'),
      c('id.type.insertWhiteSpace.nonbreakingSpaceFixedWidth'),
      S,
      c('id.type.insertWhiteSpace.hairSpace'),
      c('id.type.insertWhiteSpace.sixthSpace'),
      c('id.type.insertWhiteSpace.thinSpace'),
      c('id.type.insertWhiteSpace.quarterSpace'),
      c('id.type.insertWhiteSpace.thirdSpace'),
      c('id.type.insertWhiteSpace.punctuationSpace'),
      c('id.type.insertWhiteSpace.figureSpace'),
      c('id.type.insertWhiteSpace.flushSpace'),
    ]),
    m('menu.textContextMenu.insertBreakCharacter', [
      c('id.type.insertBreakCharacter.columnBreak'),
      c('id.type.insertBreakCharacter.frameBreak'),
      c('id.type.insertBreakCharacter.pageBreak'),
      c('id.type.insertBreakCharacter.oddPageBreak'),
      c('id.type.insertBreakCharacter.evenPageBreak'),
      c('id.type.insertBreakCharacter.paragraphReturn'),
      S,
      c('id.type.insertBreakCharacter.forcedLineBreak'),
      c('id.type.insertBreakCharacter.discretionaryLineBreak'),
    ]),
    c('type.placeholder'),
    S,
    m('menu.textContextMenu.hyperlinks', [
      c('id.type.hyperlinksAndCrossReferences.newHyperlink'),
      c('id.type.hyperlinksAndCrossReferences.editHyperlink'),
      c('id.panel.hyperlinks.goToDestination'),
    ]),
    m('menu.textContextMenu.interactive', [
      c('id.type.hyperlinksAndCrossReferences.insertCrossReference'),
    ]),
    S,
    c('edit.storyEditor'),
    S,
    c('id.type.notes.newNote'),
    c('id.type.notes.openNote'),
    c('id.type.notes.deleteNote'),
    c('id.type.notes.previousNote'),
    c('id.type.notes.nextNote'),
    c('id.type.notes.convertToNote'),
    c('id.type.notes.convertToText'),
    c('id.type.notes.splitNote'),
    c('id.type.notes.expandCollapseNotesInStory'),
    c('id.type.notes.removeNotesFromStory'),
    c('id.type.notes.removeAllNotes'),
    c('id.ctx.text.arrangeByRows'),
    c('id.ctx.text.arrangeByColumns'),
  ],
  // table: table cells (Table Context Menu)
  table: [
    m('menu.tableContextMenu.tableOptions', [
      c('id.table.tableOptions.tableSetup'),
      c('id.table.tableOptions.alternatingRowStrokes'),
      c('id.table.tableOptions.alternatingColumnStrokes'),
      c('id.table.tableOptions.alternatingFills'),
      c('id.table.tableOptions.headersAndFooters'),
    ]),
    m('menu.tableContextMenu.cellOptions', [
      c('id.table.cellOptions.text'),
      c('id.table.cellOptions.graphic'),
      c('id.table.cellOptions.strokesAndFills'),
      c('id.table.cellOptions.rowsAndColumns'),
      c('id.table.cellOptions.diagonalLines'),
    ]),
    S,
    m('menu.tableContextMenu.insert', [
      c('id.table.insert.row'),
      c('id.table.insert.column'),
    ]),
    m('menu.tableContextMenu.delete', [
      c('id.table.delete.row'),
      c('id.table.delete.column'),
      c('id.table.delete.table'),
    ]),
    m('menu.tableContextMenu.select', [
      c('id.table.select.cell'),
      c('id.table.select.row'),
      c('id.table.select.column'),
      c('id.table.select.table'),
      S,
      c('id.table.select.headerRows'),
      c('id.table.select.bodyRows'),
      c('id.table.select.footerRows'),
    ]),
    S,
    c('table.merge'),
    c('id.table.unmergeCells'),
    c('id.table.splitCellHorizontally'),
    c('id.table.splitCellVertically'),
    c('id.table.pasteBefore'),
    c('id.table.pasteAfter'),
    c('id.table.convertCellToGraphicCell'),
    c('id.table.convertCellToTextCell'),
    c('id.ctx.table.convertToHeaderRows'),
    c('id.ctx.table.convertToBodyRows'),
    c('id.ctx.table.convertToFooterRows'),
    S,
    c('id.table.distributeRowsEvenly'),
    c('id.table.distributeColumnsEvenly'),
    S,
    m('menu.tableContextMenu.writingDirection', [
      c('id.ctx.table.writingDirection.horizontal'),
      c('id.ctx.table.writingDirection.vertical'),
    ]),
    S,
    m('menu.tableContextMenu.rotate', [
      c('id.ctx.table.rotate.0Deg'),
      c('id.ctx.table.rotate.90Deg'),
      c('id.ctx.table.rotate.180Deg'),
      c('id.ctx.table.rotate.270Deg'),
    ]),
    S,
    c('id.type.bulletedAndNumberedLists.convertBulletsAndNumberingToText'),
    c('id.panel.paragraph.bulletsAndNumbering'),
    S,
    c('id.table.goToRow'),
    c('id.table.editHeader'),
    c('id.table.editFooter'),
  ],
  // guide: ruler guide selected (Guide Context Menu)
  guide: [
    c('edit.cut'),
    c('edit.copy'),
    c('edit.paste'),
    S,
    c('layout.rulerGuides'),
    m('menu.guideContextMenu.guideType', [
      c('id.ctx.guide.guideType.rulerGuide'),
    ]),
    S,
    c('view.guides'),
    c('view.lockGuides'),
    c('view.snapGuides'),
    c('view.smartGuides'),
    c('id.ctx.layoutNone.gridsAndGuides.guidesInBack'),
    c('id.ctx.guide.moveGuides'),
    S,
    c('object.lock'),
    c('id.ctx.guide.unlock'),
    S,
    c('id.view.gridsAndGuides.deleteAllGuidesOnSpread'),
    c('id.ctx.layoutObject.allowParentItemOverrides'),
  ],
  // styleParagraph: Paragraph / Character Styles row (Text Style List Context Menu)
  styleParagraph: [
    c('id.panel.paragraphStyles.styleOptions'),
    c('id.panel.paragraphStyles.duplicateStyle'),
    c('id.panel.paragraphStyles.deleteStyles'),
    c('id.panel.paragraphStyles.redefineStyle'),
    S,
    c('id.ctx.styleParagraph.apply'),
    c('id.ctx.styleParagraph.applyClearOverrides'),
    c('id.ctx.styleParagraph.applyClearCharacterStyles'),
    c('id.ctx.styleParagraph.applyClearAll'),
    S,
    c('id.ctx.styleParagraph.applyThenNextStyle'),
    c('id.ctx.styleParagraph.applyThenNextStyleClearOverrides'),
    c('id.ctx.styleParagraph.applyThenNextStyleClearCharacterStyles'),
    c('id.ctx.styleParagraph.applyThenNextStyleClearAll'),
    S,
    c('id.panel.paragraphStyles.convertStyleSBulletsAndNumberingToText'),
    c('id.panel.paragraphStyles.newParagraphStyle'),
    S,
    c('id.ctx.styleParagraph.styleGroupOptions'),
    c('id.ctx.styleParagraph.duplicateGroup'),
    c('id.ctx.styleParagraph.deleteGroup'),
    S,
    c('id.panel.paragraphStyles.copyToGroup'),
    c('id.panel.paragraphStyles.newGroupFromStyles'),
  ],
  // styleObject: Object Styles row (Object Styles Item Context Menu)
  styleObject: [
    c('id.panel.objectStyles.styleOptions'),
    c('id.panel.objectStyles.duplicateObjectStyle'),
    c('id.panel.objectStyles.deleteStyles'),
    c('id.panel.objectStyles.redefineStyle'),
    c('id.panel.objectStyles.clearOverrides'),
    S,
    c('id.ctx.styleObject.applyStyle'),
    c('id.ctx.styleObject.applyStyleClearOverrides'),
    c('id.ctx.styleObject.applyStyleClearAttributesNotDefinedByStyle'),
    c('id.ctx.styleObject.applyStyleClearAll'),
    S,
    m('menu.objectStylesPanelMenu.defaultTextFrameStyle', [
      c('id.panel.objectStyles.defaultTextFrameStyle.none'),
      c('id.panel.objectStyles.defaultTextFrameStyle.basicGraphicsFrame'),
      c('id.panel.objectStyles.defaultTextFrameStyle.basicTextFrame'),
    ]),
    m('menu.objectStylesPanelMenu.defaultGraphicFrameStyle', [
      c('id.panel.objectStyles.defaultGraphicFrameStyle.none'),
      c('id.panel.objectStyles.defaultGraphicFrameStyle.basicGraphicsFrame'),
      c('id.panel.objectStyles.defaultGraphicFrameStyle.basicTextFrame'),
    ]),
    S,
    c('id.panel.objectStyles.newObjectStyle'),
    S,
    c('id.ctx.styleObject.styleGroupOptions'),
    c('id.ctx.styleObject.duplicateGroup'),
    c('id.ctx.styleObject.deleteGroup'),
    S,
    c('id.panel.objectStyles.copyToGroup'),
    c('id.panel.objectStyles.newGroupFromStyles'),
  ],
  // styleTable: Table Styles row (Tables Style Panel Context Menu)
  styleTable: [
    c('id.panel.tableStyles.styleOptions'),
    c('id.panel.tableStyles.duplicateStyle'),
    c('id.panel.tableStyles.deleteStyle'),
    c('id.panel.tableStyles.redefineStyle'),
    S,
    c('id.ctx.styleTable.applyStyle'),
    c('id.ctx.styleTable.applyStyleClearOverrides'),
    c('id.ctx.styleTable.applyStyleClearCellStyles'),
    c('id.ctx.styleTable.applyStyleClearAll'),
    S,
    c('id.panel.tableStyles.newTableStyle'),
    S,
    c('id.ctx.styleTable.styleGroupOptions'),
    c('id.ctx.styleTable.duplicateStyleGroup'),
    c('id.ctx.styleTable.deleteStyleGroup'),
    S,
    c('id.panel.tableStyles.copyToGroup'),
    c('id.panel.tableStyles.newGroupFromStyles'),
  ],
  // styleCell: Cell Styles row (Cell Styles Panel Context Menu)
  styleCell: [
    c('id.panel.cellStyles.styleOptions'),
    c('id.panel.cellStyles.duplicateStyle'),
    c('id.panel.cellStyles.deleteStyle'),
    c('id.panel.cellStyles.redefineStyle'),
    S,
    c('id.ctx.styleCell.applyStyle'),
    c('id.ctx.styleCell.applyStyleClearAll'),
    S,
    c('id.panel.cellStyles.newCellStyle'),
    S,
    c('id.ctx.styleCell.styleGroupOptions'),
    c('id.ctx.styleCell.duplicateStyleGroup'),
    c('id.ctx.styleCell.deleteStyleGroup'),
    S,
    c('id.panel.cellStyles.copyToGroup'),
    c('id.panel.cellStyles.newGroupFromStyles'),
  ],
  // glyphs: Glyphs panel cell (Glyphs Panel Context Menu)
  glyphs: [
    c('id.panel.glyphs.newGlyphSet'),
    c('glyphs.addToSet'),
    c('glyphs.editSet'),
    c('glyphs.viewSet'),
    c('glyphs.deleteSet'),
    S,
    c('id.ctx.glyphs.deleteGlyphFromSet'),
    S,
    c('id.ctx.glyphs.loadGlyphInFind'),
    c('id.ctx.glyphs.loadGlyphInChange'),
  ],
  // glyphsRecent: Glyphs panel cell in the Recent Glyphs view (Glyphs Panel Recently Used Context Menu)
  glyphsRecent: [
    c('glyphs.addToSet'),
    S,
    c('id.ctx.glyphsRecent.deleteGlyphFromRecentlyUsed'),
    c('id.ctx.glyphsRecent.clearAllRecentlyUsed'),
    S,
    c('id.ctx.glyphs.loadGlyphInFind'),
    c('id.ctx.glyphs.loadGlyphInChange'),
  ],
  // tabs: Tabs panel marker (Tabs Context Menu)
  tabs: [
    c('id.ctx.tabs.clearAll'),
    c('id.ctx.tabs.deleteTab'),
    c('id.ctx.tabs.repeatTab'),
    c('id.ctx.tabs.resetIndents'),
    S,
    m('menu.tabsContextMenu.tabType', [
      c('id.ctx.tabs.tabType.leftJustifiedTab'),
      c('id.ctx.tabs.tabType.centerJustifiedTab'),
      c('id.ctx.tabs.tabType.rightJustifiedTab'),
      c('id.ctx.tabs.tabType.decimalTab'),
    ]),
  ],
  // rulerH: horizontal ruler (units, ruler origin; Horizontal Ruler Context Menu)
  rulerH: [
    c('id.ctx.rulerH.points'),
    c('id.ctx.rulerH.picas'),
    c('id.ctx.rulerH.inches'),
    c('id.ctx.rulerH.inchesDecimal'),
    c('id.ctx.rulerH.millimeters'),
    c('id.ctx.rulerH.centimeters'),
    c('id.ctx.rulerH.ciceros'),
    c('id.ctx.rulerH.agates'),
    c('id.ctx.rulerH.pixels'),
    S,
    c('id.ctx.rulerH.rulerPerPage'),
    c('id.ctx.rulerH.rulerPerSpread'),
    c('id.ctx.rulerH.rulerOnSpine'),
    S,
    c('view.rulers'),
    S,
    c('id.view.gridsAndGuides.deleteAllGuidesOnSpread'),
  ],
  // rulerV: vertical ruler (units; Vertical Ruler Context Menu)
  rulerV: [
    c('id.ctx.rulerV.points'),
    c('id.ctx.rulerV.picas'),
    c('id.ctx.rulerV.inches'),
    c('id.ctx.rulerV.inchesDecimal'),
    c('id.ctx.rulerV.millimeters'),
    c('id.ctx.rulerV.centimeters'),
    c('id.ctx.rulerV.ciceros'),
    c('id.ctx.rulerV.agates'),
    c('id.ctx.rulerV.pixels'),
    S,
    c('view.rulers'),
    S,
    c('id.view.gridsAndGuides.deleteAllGuidesOnSpread'),
  ],
  // zeroPoint: ruler corner / zero point (lock, units of both rulers; Zero Point Context Menu)
  zeroPoint: [
    c('id.ctx.zeroPoint.lockZeroPoint'),
    S,
    c('id.ctx.zeroPoint.points'),
    c('id.ctx.zeroPoint.picas'),
    c('id.ctx.zeroPoint.inches'),
    c('id.ctx.zeroPoint.inchesDecimal'),
    c('id.ctx.zeroPoint.millimeters'),
    c('id.ctx.zeroPoint.centimeters'),
    c('id.ctx.zeroPoint.ciceros'),
    c('id.ctx.zeroPoint.agates'),
    c('id.ctx.zeroPoint.pixels'),
    S,
    c('view.rulers'),
  ],
  // pagesPages: page icons of the Pages panel (Pages Panel Pages Context Menu)
  pagesPages: [
    c('layout.insertPages'),
    c('layout.movePages'),
    c('id.layout.pages.duplicateSpread'),
    c('id.panel.pages.deleteSpread'),
    c('id.layout.pages.hideSpread'),
    S,
    c('id.layout.pages.printPages'),
    S,
    c('id.panel.pages.newParent'),
    c('id.panel.pages.parentOptions'),
    c('id.layout.pages.applyParentToPages'),
    c('id.panel.pages.overrideAllParentPageItems'),
    S,
    c('layout.numbering'),
    S,
    c('id.panel.pages.allowDocumentPagesToShuffle'),
    c('id.panel.pages.allowSelectedSpreadToShuffle'),
    S,
    m('menu.pagesPanelPagesContextMenu.pageAttributes', [
      m('menu.pagesPanelPagesContextMenu.pageAttributes.colorLabel', [
        c('id.panel.pages.pageAttributes.colorLabel.none'),
        c('id.panel.pages.pageAttributes.colorLabel.useParentColor'),
        S,
        c('id.panel.pages.pageAttributes.colorLabel.white'),
        c('id.panel.pages.pageAttributes.colorLabel.pink'),
        c('id.panel.pages.pageAttributes.colorLabel.gold'),
        c('id.panel.pages.pageAttributes.colorLabel.yellow'),
        c('id.panel.pages.pageAttributes.colorLabel.green'),
        c('id.panel.pages.pageAttributes.colorLabel.lightBlue'),
        c('id.panel.pages.pageAttributes.colorLabel.lavender'),
        c('id.panel.pages.pageAttributes.colorLabel.gray'),
        c('id.panel.pages.pageAttributes.colorLabel.red'),
        c('id.panel.pages.pageAttributes.colorLabel.orange'),
        c('id.panel.pages.pageAttributes.colorLabel.tan'),
        c('id.panel.pages.pageAttributes.colorLabel.darkGreen'),
        c('id.panel.pages.pageAttributes.colorLabel.blue'),
        c('id.panel.pages.pageAttributes.colorLabel.violet'),
        c('id.panel.pages.pageAttributes.colorLabel.black'),
      ]),
      m('menu.pagesPanelPagesContextMenu.pageAttributes.rotateSpreadView', [
        c('id.view.rotateSpread.90DegCw'),
        c('id.view.rotateSpread.90DegCcw'),
        c('id.view.rotateSpread.180Deg'),
        S,
        c('id.view.rotateSpread.clearRotation'),
      ]),
      m('menu.pagesPanelPagesContextMenu.pageAttributes.spreadFlattening', [
        c('id.panel.pages.pageAttributes.spreadFlattening.default'),
        S,
        c('id.panel.pages.pageAttributes.spreadFlattening.noneIgnoreTransparency'),
        S,
        c('id.panel.pages.pageAttributes.spreadFlattening.custom'),
      ]),
    ]),
    S,
    m('menu.pagesPanelPagesContextMenu.viewPages', [
      c('id.panel.pages.viewPages.horizontally'),
      c('id.panel.pages.viewPages.vertically'),
    ]),
    c('id.panel.pages.panelOptions'),
  ],
  // pagesPanel: empty area of the Pages panel (Pages Panel Context Menu)
  pagesPanel: [
    c('layout.insertPages'),
    c('layout.movePages'),
    S,
    c('id.panel.pages.newParent'),
    c('id.layout.pages.applyParentToPages'),
    S,
    m('menu.pagesPanelContextMenu.viewPages', [
      c('id.panel.pages.viewPages.horizontally'),
      c('id.panel.pages.viewPages.vertically'),
    ]),
    c('id.panel.pages.panelOptions'),
  ],
  // pagesSection: section marker of a page icon in the Pages panel (Pages Panel Section Context Menu)
  pagesSection: [
    c('layout.numbering'),
    c('id.ctx.pagesSection.deleteSection'),
  ],
  // layer: layer row of the Layers panel (Layers Panel Layer Context Menu)
  layer: [
    c('id.panel.layers.newLayer'),
    c('id.panel.layers.duplicateLayer'),
    c('id.panel.layers.deleteLayer'),
    S,
    c('id.panel.layers.layerOptions'),
    S,
    c('id.panel.layers.mergeLayers'),
    c('id.panel.layers.selectItemS'),
  ],
  // linksHeader: column header of the Links panel (Links Header Context Menu)
  linksHeader: [
    c('id.panel.links.panelOptions'),
    c('id.ctx.linksHeader.hideThisColumn'),
    c('id.panel.links.copyInfo.copyInfoForSelectedLinks'),
  ],
  // linkInfo: link information pane of the Links panel (Link Info Context Menu)
  linkInfo: [
    c('id.panel.links.panelOptions'),
    c('id.panel.links.utilities.xmpFileInfo'),
    c('id.panel.links.copyInfo.copyFullPath'),
    c('id.panel.links.copyInfo.copyPlatformStylePath'),
    c('id.panel.links.copyInfo.copyAllLinkInformation'),
    c('id.ctx.linkInfo.copyInfoField'),
    c('id.panel.links.utilities.autoSizeLinkInformation'),
  ],
};
