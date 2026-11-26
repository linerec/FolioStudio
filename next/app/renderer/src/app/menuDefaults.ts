import { t as tr } from '../i18n';

/** Type ▸ Text Variables ▸ Insert Variable (and the same items of the text context menu): menu id → the kind of the document variable it
 * inserts (`VariableType`) and the name of the variable InDesign gives a new document, which is taken first; the document has to
 * define one of that kind (a variable named "Running Header" first for that kind). What the instance shows (dates from the file's
 * dates, the file name, the link of the graphic under the frame) the engine works out (oracle textvars). */
export const TEXT_VARIABLES: Record<string, { kind: string; name?: string }> = {
  'id.type.textVariables.insertVariable.chapterNumber': { kind: 'ChapterNumberType' },
  'id.type.textVariables.insertVariable.lastPageNumber': { kind: 'LastPageNumberType' },
  'id.type.textVariables.insertVariable.runningHeader': { kind: 'MatchParagraphStyleType', name: 'Running Header' },
  'id.type.textVariables.insertVariable.creationDate': { kind: 'CreationDateType', name: 'Creation Date' },
  'id.type.textVariables.insertVariable.fileName': { kind: 'FileNameType', name: 'File Name' },
  'id.type.textVariables.insertVariable.imageName': { kind: 'LiveCaptionType', name: 'Image Name' },
  'id.type.textVariables.insertVariable.modificationDate': { kind: 'ModificationDateType', name: 'Modification Date' },
  'id.type.textVariables.insertVariable.outputDate': { kind: 'OutputDateType', name: 'Output Date' },
};

/** Type ▸ Insert Special Character / White Space / Break Character: menu id → the character
 * the engine inserts, by InDesign's scripting name (oracle text-special). */
export const SPECIAL_CHARACTERS: Record<string, string> = {
  'id.type.insertSpecialCharacter.symbols.bulletCharacter': 'BULLET_CHARACTER',
  'id.type.insertSpecialCharacter.symbols.copyrightSymbol': 'COPYRIGHT_SYMBOL',
  'id.type.insertSpecialCharacter.symbols.ellipsis': 'ELLIPSIS_CHARACTER',
  'id.type.insertSpecialCharacter.symbols.paragraphSymbol': 'PARAGRAPH_SYMBOL',
  'id.type.insertSpecialCharacter.symbols.registeredTrademarkSymbol': 'REGISTERED_TRADEMARK',
  'id.type.insertSpecialCharacter.symbols.sectionSymbol': 'SECTION_SYMBOL',
  'id.type.insertSpecialCharacter.symbols.trademarkSymbol': 'TRADEMARK_SYMBOL',
  'id.type.insertSpecialCharacter.markers.currentPageNumber': 'AUTO_PAGE_NUMBER',
  'id.type.insertSpecialCharacter.markers.nextPageNumber': 'NEXT_PAGE_NUMBER',
  'id.type.insertSpecialCharacter.markers.previousPageNumber': 'PREVIOUS_PAGE_NUMBER',
  'id.type.insertSpecialCharacter.markers.sectionMarker': 'SECTION_MARKER',
  'id.type.insertSpecialCharacter.hyphensAndDashes.emDash': 'EM_DASH',
  'id.type.insertSpecialCharacter.hyphensAndDashes.enDash': 'EN_DASH',
  'id.type.insertSpecialCharacter.hyphensAndDashes.discretionaryHyphen': 'DISCRETIONARY_HYPHEN',
  'id.type.insertSpecialCharacter.hyphensAndDashes.nonbreakingHyphen': 'NONBREAKING_HYPHEN',
  'id.type.insertSpecialCharacter.quotationMarks.doubleLeftQuotationMarks': 'DOUBLE_LEFT_QUOTE',
  'id.type.insertSpecialCharacter.quotationMarks.doubleRightQuotationMarks': 'DOUBLE_RIGHT_QUOTE',
  'id.type.insertSpecialCharacter.quotationMarks.singleLeftQuotationMark': 'SINGLE_LEFT_QUOTE',
  'id.type.insertSpecialCharacter.quotationMarks.singleRightQuotationMark': 'SINGLE_RIGHT_QUOTE',
  'id.type.insertSpecialCharacter.quotationMarks.straightDoubleQuotationMarks': 'DOUBLE_STRAIGHT_QUOTE',
  'id.type.insertSpecialCharacter.quotationMarks.straightSingleQuotationMarkApostrophe': 'SINGLE_STRAIGHT_QUOTE',
  'id.type.insertSpecialCharacter.other.tab': 'TAB',
  'id.type.insertSpecialCharacter.other.rightIndentTab': 'RIGHT_INDENT_TAB',
  'id.type.insertSpecialCharacter.other.indentToHere': 'INDENT_HERE_TAB',
  'id.type.insertSpecialCharacter.other.endNestedStyleHere': 'END_NESTED_STYLE',
  'id.type.insertSpecialCharacter.other.nonJoiner': 'ZERO_WIDTH_NONJOINER',
  'id.type.insertWhiteSpace.emSpace': 'EM_SPACE',
  'id.type.insertWhiteSpace.enSpace': 'EN_SPACE',
  'id.type.insertWhiteSpace.nonbreakingSpace': 'NONBREAKING_SPACE',
  'id.type.insertWhiteSpace.nonbreakingSpaceFixedWidth': 'FIXED_WIDTH_NONBREAKING_SPACE',
  'id.type.insertWhiteSpace.hairSpace': 'HAIR_SPACE',
  'id.type.insertWhiteSpace.sixthSpace': 'SIXTH_SPACE',
  'id.type.insertWhiteSpace.thinSpace': 'THIN_SPACE',
  'id.type.insertWhiteSpace.quarterSpace': 'QUARTER_SPACE',
  'id.type.insertWhiteSpace.thirdSpace': 'THIRD_SPACE',
  'id.type.insertWhiteSpace.punctuationSpace': 'PUNCTUATION_SPACE',
  'id.type.insertWhiteSpace.figureSpace': 'FIGURE_SPACE',
  'id.type.insertWhiteSpace.flushSpace': 'FLUSH_SPACE',
  'id.type.insertBreakCharacter.columnBreak': 'COLUMN_BREAK',
  'id.type.insertBreakCharacter.frameBreak': 'FRAME_BREAK',
  'id.type.insertBreakCharacter.pageBreak': 'PAGE_BREAK',
  'id.type.insertBreakCharacter.oddPageBreak': 'ODD_PAGE_BREAK',
  'id.type.insertBreakCharacter.evenPageBreak': 'EVEN_PAGE_BREAK',
  'id.type.insertBreakCharacter.paragraphReturn': 'PARAGRAPH_RETURN',
  'id.type.insertBreakCharacter.forcedLineBreak': 'FORCED_LINE_BREAK',
  'id.type.insertBreakCharacter.discretionaryLineBreak': 'DISCRETIONARY_LINE_BREAK'
};
/** Type ▸ Size presets (pt). */
export const TYPE_SIZES = [6, 8, 9, 10, 11, 12, 14, 18, 24, 30, 36, 48, 60, 72] as const;

export const NOT_YET = () => tr('아직 없는 기능입니다');
export const READ_ONLY = () => tr('읽기 전용으로 연 문서입니다');

