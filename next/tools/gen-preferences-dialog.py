"""Writes next/shared/dialogs/preferences.json: Edit > Preferences as one dialog of the shared frame (one tab per category).

The screen and the engine read the same declaration (the engine through dialog_state::schema, the menu through the file itself).
The categories and fields are listed here, in InDesign's order; the interface colours and the autocorrect languages are read from the
screen's own lists (ui/uicolors.ts, features/preferences/autocorrect.ts). A document preference (`docPref`) names only its key and
label: its type, default and range are the engine's (folio_idml::docprefs), filled in when the dialog is read. A view setting
(`view`) is sent with setViewPref, a document preference with setDocPrefs, the rest are app settings (userSettings preferences).
Run: python -B -I next/tools/gen-preferences-dialog.py  (a test fails when the file is not what this writes)
"""
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SRC = ROOT / 'next/app/renderer/src'
OUT = ROOT / 'next/shared/dialogs/preferences.json'


def ui_colors():
    text = (SRC / 'ui/uicolors.ts').read_text(encoding='utf-8')
    body = text.split('UI_COLOR_NAMES = ', 1)[1].split('];', 1)[0]
    return re.findall(r"\['(\w+)', tr\('([^']+)'\)\]", body)


def guides_note():
    text = (SRC / 'ui/tools.tsx').read_text(encoding='utf-8')
    return re.search(r"guidesNote = \(\) => tr\('([^']+)'\)", text).group(1)


def autocorrect_languages():
    text = (SRC / 'features/preferences/autocorrect.ts').read_text(encoding='utf-8')
    body = text.split('autoCorrectLanguages: string[] = [', 1)[1].split('];', 1)[0]
    return json.loads('[' + body + ']')


TABS = []
FIELDS = []
TAB = None


def tab(id, label, document=False, note=None, differs=None, css=None):
    global TAB
    TAB = id
    entry = {'id': id, 'label': label, 'command': 'edit.preferences' if id == 'general' else 'id.edit.preferences.' + id}
    if document:
        entry['document'] = True
    if css:
        # the class of the category's panel (the screen's checks and styles find the category's part by it)
        entry['className'] = css
    if note:
        # a grey note on what Folio does there (shown with the document's part); never a comparison with InDesign (user decision 2026-11-21)
        entry['note'], entry['differs'] = note, differs
    TABS.append(entry)


def add(field, group=None):
    field['tab'] = TAB
    if group:
        field['group'] = group
    FIELDS.append(field)
    return field


def opts(pairs):
    return [{'value': v, 'label': l} for v, l in pairs]


def check(key, label, initial, group=None):
    return add({'name': key, 'label': label, 'type': 'boolean', 'default': initial}, group)


def pick(key, label, initial, pairs, group=None):
    return add({'name': key, 'label': label, 'type': 'enum', 'default': initial, 'options': opts(pairs)}, group)


def colors(key, label, initial, group=None, label_args=None):
    # the app settings name the interface colours as InDesign's scripting does (LIGHT_BLUE)
    f = add({'name': key, 'label': label, 'type': 'enum', 'default': initial, 'options': 'appColors'}, group)
    if label_args:
        f['labelArgs'] = label_args
    return f


def doc(key, label, pairs=None):
    f = add({'name': key, 'label': label, 'docPref': True, 'document': True})
    if pairs:
        f['options'] = opts(pairs)
    return f


def view(key, label, group, **kind):
    # a document setting kept in its preferences and shown only with a document (setViewPref)
    return add({'name': key, 'label': label, 'view': True, 'document': True, 'hideWithoutDocument': True, **kind}, group)


def view_color(key, label, group):
    return view(key, label, group, type='enum', default='*custom', options='viewColors')


def number(lo, hi, default, **more):
    return {'type': 'number', 'min': lo, 'max': hi, 'default': default, **more}


UNITS = [('Points', '포인트'), ('Picas', '파이카'), ('Inches', '인치'), ('InchesDecimal', '인치(소수)'), ('Millimeters', '밀리미터'),
         ('Centimeters', '센티미터'), ('Ciceros', '시세로'), ('Agates', '에이전트'), ('Pixels', '픽셀')]


def build():
    g, galley = 'generalPreferences.', 'galleyPreferences.'
    tab('general', '일반')
    pick(g + 'pageNumbering', '페이지 번호 표시', 'SECTION', [('SECTION', '섹션 번호 매기기'), ('ABSOLUTE', '절대 번호 매기기')])
    check(g + 'showStartWorkspace', '문서를 열지 않았을 때 시작 작업 영역 표시', True)
    check(g + 'showWhatsNewOnStartup', '시작할 때 새로운 기능 표시', True)
    check(g + 'showLegacyNewDocumentDialog', '기존 새 문서 대화상자 사용', False)
    check(g + 'preventSelectingLockedItems', '잠긴 개체 선택 방지', True)
    # the engine opens its local MCP endpoint only while this is on (crates/server/src/attach.rs, next/engine/MCP.ko.md)
    check('folio.mcp.allowConnection', 'LLM 연결(MCP) 허용', False)
    check('folio.mcp.allowDatabaseRefresh', 'LLM 이 데이터베이스 원본을 새로 고칠 수 있음', False)
    # saving, exporting and placing over MCP, inside the open project only (crates/server/src/attach_paths.rs)
    check('folio.mcp.allowFileWrites', 'LLM 이 파일을 쓸 수 있음', False)
    # File ▸ New ▸ Project… over MCP only inside this folder of this computer (empty: none; attach_paths.rs new_project_folder)
    add({'name': 'folio.mcp.newProjectFolder', 'label': 'LLM 이 새 프로젝트를 만들 수 있는 폴더', 'type': 'text', 'path': 'folder',
         'choose': '위치…', 'chooseTitle': 'LLM 이 새 프로젝트를 만들 수 있는 폴더', 'default': '', 'maxLength': 1024})
    tab('interface', '인터페이스')
    pick('folio.theme', '색상 테마', 'dark', [('dark', '어둡게'), ('mediumDark', '중간 어둡게'), ('mediumLight', '중간 밝게'), ('light', '밝게')])
    pick(g + 'toolTips', '도구 설명', 'FAST', [('NORMAL', '보통'), ('NONE', '없음'), ('FAST', '빠르게')])
    # Folio's own: a card (name, keys, one line) the moment the pointer is on a tool, a small toolbox button or the reference point (ui/toolhelpcard.tsx);
    # the Tools panel menu (≡) has the same setting (uix-e1)
    check('folio.instantToolHelp', '도구 설명 즉시 보기', True)
    pick(g + 'toolsPanel', '도구 패널 모양', 'DOUBLE_COLUMN', [('SINGLE_COLUMN', '한 열'), ('DOUBLE_COLUMN', '두 열'), ('SINGLE_ROW', '한 행')])
    for key, label, on in [('autoCollapseIconPanels', '아이콘 패널 자동 축소', False), ('autoShowHiddenPanels', '숨겨진 패널 자동 표시', True),
                           ('openDocumentsAsTabs', '문서를 탭으로 열기', True), ('enableFloatingWindowDocking', '부동 문서 창 도킹 사용', True),
                           ('showTransformationValues', '변형 값 표시', True), ('showContentGrabber', '내용 선택 도구 표시', True),
                           ('showLiveCorners', '라이브 모퉁이 표시', True), ('showAnchorObjectAdornment', '고정된 개체 표시', True),
                           ('highlightObjectUnderSelectionTool', '선택 도구 아래 개체 강조', True)]:
        check(g + key, label, on)
    tab('userInterfaceScaling', '사용자 인터페이스 크기 조정')
    f = pick('folio.uiScale', '사용자 인터페이스 크기 (%)', '100', [(v, v + '%') for v in ['100', '125', '150', '175', '200']])
    for o in f['options']:
        o['native'] = True
    tab('type', '문자')
    for key, label, on in [('dragAndDropTextInLayout', '레이아웃 보기에서 텍스트 끌어 놓기', False), ('allowDragAndDropTextInStory', '스토리 편집기에서 텍스트 끌어 놓기', True),
                           ('tripleClickSelectsLine', '세 번 클릭하여 줄 선택', True), ('smartCutAndPaste', '잘라내기 및 붙이기에서 공백 자동 조정', True),
                           ('singleClickConvertsFramesToTextFrames', '문자 도구로 클릭하여 프레임을 텍스트 프레임으로 변환', True)]:
        check('textEditingPreferences.' + key, label, on)
    t = 'TextPreference.'
    doc(t + 'TypographersQuotes', '인용 부호 자동 사용')
    doc(t + 'UseOpticalSize', '광학 크기 자동 사용')
    doc(t + 'UseParagraphLeading', '전체 단락에 행간 적용')
    doc(t + 'QuoteCharactersRotatedInVertical', '세로쓰기에서 인용 부호 회전')
    doc(t + 'SmartTextReflow', '스마트 텍스트 리플로우')
    doc(t + 'AddPages', '페이지 추가 위치', [('EndOfStory', '스토리 끝'), ('EndOfSection', '섹션 끝'), ('EndOfDocument', '문서 끝')])
    doc(t + 'LimitToMasterTextFrames', '기본 텍스트 프레임으로 제한')
    doc(t + 'PreserveFacingPageSpreads', '마주보는 페이지 스프레드 유지')
    doc(t + 'DeleteEmptyPages', '빈 페이지 삭제')
    tab('advancedType', '고급 문자', document=True)
    for key, label in [('SuperscriptSize', '위 첨자 크기 (%)'), ('SuperscriptPosition', '위 첨자 위치 (%)'), ('SubscriptSize', '아래 첨자 크기 (%)'),
                       ('SubscriptPosition', '아래 첨자 위치 (%)'), ('SmallCap', '작은 대문자 크기 (%)')]:
        doc(t + key, label)
    tab('composition', '컴포지션', document=True)
    for key, label in [('HighlightHjViolations', '하이픈 넣기 및 균등 배치 위반 강조'), ('HighlightKeeps', '유지 옵션 위반 강조'),
                       ('HighlightSubstitutedGlyphs', '대체 글리프 강조'), ('HighlightCustomSpacing', '사용자 정의 자간/커닝 강조'),
                       ('HighlightSubstitutedFonts', '대체 글꼴 강조'), ('HighlightKinsoku', '금칙 위반 강조'), ('JustifyTextWraps', '개체 옆의 텍스트 균등 배치'),
                       ('AbutTextToTextWrap', '행간 단위로 건너뛰기'), ('ZOrderTextWrap', '텍스트 감싸기가 아래의 텍스트에만 영향')]:
        doc(t + key, label)
    tab('dictionary', '사전', document=True)
    # the quotation marks of each language the document has (setLanguageQuotes); the rows are the document's languages
    add({'name': 'languageQuotes', 'label': '사전 언어', 'type': 'array', 'default': [], 'document': True, 'fixedRows': True, 'maxRows': 1000, 'fields': [
        {'name': 'id', 'type': 'text', 'default': '', 'hidden': True},
        {'name': 'name', 'label': '언어', 'type': 'text', 'default': '', 'disabled': True},
        {'name': 'singleQuotes', 'label': '작은따옴표 쌍', 'type': 'text', 'default': '', 'maxLength': 8, 'messages': {'quotePair': '여는 부호와 닫는 부호를 하나씩 입력하세요.'}},
        {'name': 'doubleQuotes', 'label': '큰따옴표 쌍', 'type': 'text', 'default': '', 'maxLength': 8, 'messages': {'quotePair': '여는 부호와 닫는 부호를 하나씩 입력하세요.'}},
    ]})
    d = 'DictionaryPreference.'
    doc(d + 'Composition', '다음을 사용하여 작성', [('UseUserDictionary', '사용자 사전'), ('UseDocument', '문서'), ('Both', '사용자 사전 및 문서')])
    doc(d + 'MergeUserDictionary', '사용자 사전을 문서에 병합')
    doc(d + 'RecomposeWhenChanged', '사전이 바뀌면 모든 스토리 다시 작성')
    tab('unitsAndIncrements', '단위 및 증감', document=True, css='units-prefs-dialog')
    view('rulerOrigin', '원점', '눈금자 단위', type='enum', default='SpreadOrigin', options=opts([('SpreadOrigin', '펼침면'), ('PageOrigin', '페이지'), ('SpineOrigin', '책등')]))
    view('horizontalUnits', '가로', '눈금자 단위', type='enum', default='Points', options=opts(UNITS))
    view('verticalUnits', '세로', '눈금자 단위', type='enum', default='Points', options=opts(UNITS))
    view('strokeUnits', '획', '다른 단위', type='enum', default='Points', options=opts([('Points', '포인트'), ('Millimeters', '밀리미터')]))
    # the increments are kept in points and shown in the units the dialog has chosen (InDesign: the box follows the ruler unit)
    view('cursorKeyIncrement', '커서 키', '키보드 증감', **number(0.001, 100, 1, unit='length-h'))
    view('leadingKeyIncrement', '크기/행간', '키보드 증감', **number(0.001, 200, 2, unit='length-pt'))
    view('baselineShiftKeyIncrement', '기준선 이동', '키보드 증감', **number(0.001, 200, 2, unit='length-pt'))
    view('kerningKeyIncrement', '커닝/자간', '키보드 증감', **number(1, 100, 20, suffix='/1000 em'))
    tab('grids', '격자', document=True, css='grid-prefs-dialog')
    view_color('baselineColor', '색상', '기준선 격자')
    view('baselineStart', '시작 위치', '기준선 격자', **number(0, 1000, 36, unit='length-v'))
    view('baselineRelativeTo', '상대 위치', '기준선 격자', type='enum', default='page', options=opts([('page', '페이지 위쪽'), ('margin', '여백 위쪽')]))
    view('baselineDivision', '간격', '기준선 격자', **number(1, 8640, 12, unit='length-v'))
    view('baselineThreshold', '보기 한계값', '기준선 격자', **number(5, 4000, 75, unit='percent'))
    view_color('gridColor', '색상', '문서 격자')
    view('horizontalDivision', '가로 격자선 간격', '문서 격자', **number(0.01, 1000, 72, unit='length-v'))
    view('horizontalSubdivision', '가로 세분', '문서 격자', **number(1, 1000, 8, integer=True))
    view('verticalDivision', '세로 격자선 간격', '문서 격자', **number(0.01, 1000, 72, unit='length-h'))
    view('verticalSubdivision', '세로 세분', '문서 격자', **number(1, 1000, 8, integer=True))
    view('gridsInBack', '격자를 뒤로', None, type='boolean', default=True)
    tab('guidesAndPasteboard', '안내선 및 대지', note=guides_note(), differs='guides', css='guides-prefs-dialog')
    # the document's guide and pasteboard settings, then the Smart Guide options, which are application settings (InDesign:
    # app.smartGuidePreferences, all on by default - measured, artifacts/guideprefs-c1-20261107), so the category opens without a
    # document too; the Smart Guides colour (GRID_GREEN) is the last row of the Color group. Same keys as View > Grids & Guides >
    # Smart Guides (features/view/marks.ts).
    view_color('rulerGuidesColor', '눈금자 안내선', '색상')
    view_color('previewBackgroundColor', '미리 보기 배경', '색상')
    view_color('bleedGuideColor', '도련', '색상')
    view_color('slugGuideColor', '슬러그', '색상')
    colors('smartGuidePreferences.guideColor', '고급 안내선', 'GRID_GREEN', '색상')
    view('snapZone', '물리기 영역', '안내선 옵션', **number(1, 36, 4, integer=True, suffix='px'))
    view('rulerGuidesThreshold', '눈금자 안내선 보기 한계값', '안내선 옵션', **number(5, 4000, 5, unit='percent'))
    view('guidesInBack', '안내선을 뒤로', '안내선 옵션', type='boolean', default=False)
    view('pasteboardHorizontal', '가로 최소 간격', '대지 옵션', **number(-100000, 100000, 0, unit='length-h', disabledWhen={'matchPreviewBackground': True}))
    view('pasteboardVertical', '세로 최소 간격', '대지 옵션', **number(0, 100000, 72, unit='length-v', disabledWhen={'matchPreviewBackground': True}))
    view('matchPreviewBackground', '미리 보기 배경을 테마 색상과 일치', '대지 옵션', type='boolean', default=False)
    for key, label in [('alignToObjectCenter', '개체 가운데에 정렬'), ('alignToObjectEdges', '개체 가장자리에 정렬'), ('smartDimensions', '고급 크기'), ('smartSpacing', '고급 간격')]:
        check('smartGuidePreferences.' + key, label, True, '고급 안내선 옵션')
    tab('spelling', '맞춤법')
    s = 'spellPreferences.'
    check(s + 'checkMisspelledWords', '맞춤법이 틀린 단어', True)
    check(s + 'checkRepeatedWords', '반복된 단어', True)
    check(s + 'checkCapitalizedWords', '대문자로 시작하지 않은 단어', True)
    check(s + 'checkCapitalizedSentences', '대문자로 시작하지 않은 문장', True)
    check(s + 'dynamicSpellCheck', '동적 맞춤법 검사 사용', False)
    colors(s + 'misspelledWordColor', '맞춤법 오류 색상', 'RED')
    colors(s + 'repeatedWordColor', '반복된 단어 색상', 'GREEN')
    colors(s + 'uncapitalizedWordColor', '단어 대문자 오류 색상', 'GREEN')
    colors(s + 'uncapitalizedSentenceColor', '문장 대문자 오류 색상', 'GREEN')
    tab('autocorrect', '자동 고침')
    check('autoCorrectPreferences.autoCorrect', '자동 고침 사용', False)
    check('autoCorrectPreferences.autoCorrectCapitalizationErrors', '대문자 오류 자동 고침', False)
    # the word lists of every language (app setting autoCorrectTables); one word once per language, kept sorted by word
    add({'name': 'autoCorrect', 'label': '자동 고침 단어 목록', 'type': 'array', 'default': [], 'maxRows': 10000, 'unique': ['language', 'word'],
         'addLabel': '새 항목', 'removeLabel': '삭제', 'newRow': {'language': 'English: USA'}, 'messages': {'duplicate': '이 단어는 목록에 이미 있습니다.'}, 'fields': [
        {'name': 'language', 'label': '언어', 'type': 'enum', 'default': 'English: USA', 'options': [{'value': l, 'label': l, 'native': True} for l in autocorrect_languages()]},
        {'name': 'word', 'label': '잘못 입력한 단어', 'type': 'text', 'default': '', 'maxLength': 256},
        {'name': 'correction', 'label': '고친 단어', 'type': 'text', 'default': '', 'maxLength': 256},
    ]})
    tab('notes', '메모')
    n = 'notePreferences.'
    pick(n + 'noteColorChoices', '메모 색상 기준', 'USE_USER_COLOR', [('USE_USER_COLOR', '사용자 색상'), ('USE_NOTE_PREF_COLOR', '사용자 지정 색상')])
    colors(n + 'noteColor', '메모 색상', 'GREEN')
    pick(n + 'noteBackgroundColor', '메모 배경', 'GALLEY_BACKGROUND_COLOR', [('GALLEY_BACKGROUND_COLOR', '스토리 편집기 배경'), ('USE_NOTE_COLOR', '메모 색상')])
    check(n + 'showNoteTips', '메모 도구 설명 표시', True)
    check(n + 'findAndReplaceNoteContents', '찾기/바꾸기에 메모 내용 포함', True)
    check(n + 'spellCheckNotes', '맞춤법 검사에 메모 포함', True)
    tab('trackChanges', '변경 내용 추적')
    c = 'trackChangesPreferences.'
    for kind, prefix, label, marking in [('Added', 'added', '추가된 텍스트', 'NONE'), ('Deleted', 'deleted', '삭제된 텍스트', 'STRIKETHROUGH'), ('Moved', 'moved', '이동된 텍스트', 'OUTLINE')]:
        args = {'name': label}
        pick(c + prefix + 'TextColorChoice', '{name}: 글자 색상 기준', 'CHANGE_USES_GALLEY_TEXT_COLOR', [
            ('CHANGE_USES_GALLEY_TEXT_COLOR', '스토리 편집기 글자 색상'), ('CHANGE_USES_CHANGE_PREF_COLOR', '사용자 지정 색상')])['labelArgs'] = args
        colors(c + 'textColorFor' + kind + 'Text', '{name}: 글자 색상', 'BLACK', label_args=args)
        pick(c + prefix + 'BackgroundColorChoice', '{name}: 배경 색상 기준', 'CHANGE_BACKGROUND_USES_USER_COLOR', [
            ('CHANGE_BACKGROUND_USES_GALLEY_BACKGROUND_COLOR', '스토리 편집기 배경'), ('CHANGE_BACKGROUND_USES_USER_COLOR', '사용자 색상'),
            ('CHANGE_BACKGROUND_USES_CHANGE_PREF_COLOR', '사용자 지정 색상')])['labelArgs'] = args
        colors(c + 'backgroundColorFor' + kind + 'Text', '{name}: 배경 색상', 'YELLOW', label_args=args)
        pick(c + 'markingFor' + kind + 'Text', '{name}: 표시', marking, [('NONE', '없음'), ('STRIKETHROUGH', '취소선'), ('UNDERLINE_SINGLE', '밑줄'), ('OUTLINE', '윤곽선')])['labelArgs'] = args
    for key, label in [('showAddedText', '추가된 텍스트 표시'), ('showDeletedText', '삭제된 텍스트 표시'), ('showMovedText', '이동된 텍스트 표시'), ('showChangeBars', '변경 막대 표시')]:
        check(c + key, label, True)
    colors(c + 'changeBarColor', '변경 막대 색상', 'CYAN')
    pick(c + 'locationForChangeBar', '변경 막대 위치', 'LEFT_ALIGN', [('LEFT_ALIGN', '왼쪽'), ('RIGHT_ALIGN', '오른쪽')])
    check(c + 'spellCheckDeletedText', '맞춤법 검사에 삭제된 텍스트 포함', True)
    check(c + 'preventDuplicateColor', '사용자 색상 중복 방지', False)
    tab('storyEditorDisplay', '스토리 편집기 표시')
    colors(galley + 'backgroundColor', '배경 색상', 'WHITE')
    colors(galley + 'textColor', '텍스트 색상', 'BLACK')
    pick(galley + 'cursorType', '커서 모양', 'STANDARD_CURSOR', [('STANDARD_CURSOR', '표준'), ('THICK_CURSOR', '굵게'), ('BARBELL_CURSOR', '바벨'), ('BLOCK_CURSOR', '블록')])
    check(galley + 'blinkCursor', '커서 깜박임', True)
    pick(galley + 'lineSpacingValue', '줄 간격', 'SINGLE_SPACE', [('SINGLE_SPACE', '한 줄'), ('ONE_AND_HALF_SPACE', '1.5줄'), ('DOUBLE_SPACE', '두 줄'), ('TRIPLE_SPACE', '세 줄')])
    check(galley + 'smoothText', '텍스트 매끄럽게', True)
    pick(galley + 'antiAliasType', '텍스트 매끄럽게 방식', 'GRAY_ANTIALIASING', [('GRAY_ANTIALIASING', '회색조'), ('COLOR_ANTIALIASING', '색상'), ('THICKER_ANTIALIASING', '더 굵게')])
    check(galley + 'showDepthRuler', '깊이 눈금자 표시', True)
    check(galley + 'showParagraphStyleNames', '단락 스타일 이름 표시', True)
    check(galley + 'showInfoColumn', '정보 열 표시', True)
    check(galley + 'showParagraphBreakMarks', '단락 나누기 기호 표시', False)
    tab('displayPerformance', '표시 성능')
    pick('displayPerformancePreferences.defaultDisplaySettings', '기본 보기', 'HIGH_QUALITY', [('OPTIMIZED', '빠른 표시'), ('TYPICAL', '일반 표시'), ('HIGH_QUALITY', '고품질 표시')])
    check('displayPerformancePreferences.ignoreLocalSettings', '개체별 표시 설정 무시', False)
    check('displayPerformancePreferences.persistLocalSettings', '개체별 표시 설정 유지', False)
    pick('grabberPreferences.grabberPanning', '손 도구 사용 중 표시', 'NO_GREEKING', [('NO_GREEKING', '이미지와 텍스트 표시'), ('GREEK_IMAGES', '이미지 숨기기'), ('GREEK_IMAGES_AND_TEXT', '이미지와 텍스트 숨기기')])
    tab('appearanceOfBlack', '검정 모양')
    check('colorSettings.idealizedBlackToScreen', '화면에서 모든 검정을 진한 검정으로 표시', True)
    check('colorSettings.idealizedBlackToExport', '인쇄 및 내보내기에서 모든 검정을 진한 검정으로 출력', True)
    tab('fileHandling', '파일 처리')
    check(g + 'includePreview', '문서와 함께 미리 보기 저장', True)
    pick(g + 'previewPages', '미리 보기 페이지', 'FIRST_2_PAGES', [('FIRST_PAGE', '첫 페이지'), ('FIRST_2_PAGES', '처음 2페이지'), ('FIRST_5_PAGES', '처음 5페이지'), ('FIRST_10_PAGES', '처음 10페이지'), ('ALL_PAGES', '모든 페이지')])
    pick(g + 'previewSize', '미리 보기 크기', 'LARGE', [('SMALL', '작게'), ('MEDIUM', '중간'), ('LARGE', '크게'), ('EXTRA_LARGE', '아주 크게')])
    check('linkingPreferences.checkLinksAtOpen', '문서를 열 때 연결 확인', True)
    check('linkingPreferences.findMissingLinksAtOpen', '문서를 열 때 누락된 연결 찾기', True)
    doc(t + 'LinkTextFilesWhenImporting', '텍스트 및 스프레드시트 파일 가져올 때 연결 만들기')
    tab('clipboardHandling', '클립보드 처리')
    for key, label, on in [('preferStyledTextWhenPasting', '붙일 때 모든 정보 유지', False), ('preferPDFWhenPasting', '붙일 때 PDF 우선 사용', False),
                           ('copyPDFToClipboard', 'PDF를 클립보드에 복사', True), ('pasteRemembersLayers', '붙일 때 레이어 기억', False),
                           ('preservePdfClipboardAtQuit', '종료 후 PDF 클립보드 유지', False), ('showPasteOptions', '붙이기 옵션 표시', True)]:
        check('clipboardPreferences.' + key, label, on)

    names = ui_colors()
    app_colors = [{'value': re.sub(r'([a-z])([A-Z])', r'\1_\2', v).upper(), 'label': l} for v, l in names]
    view_colors = [{'value': '*custom', 'label': '사용자 지정', 'current': True}] + [{'value': v, 'label': l} for v, l in names]
    return {
        'title': '환경 설정',
        'scope': 'either',
        'style': 'preferences',
        'target': 'none',
        'defaults': 'preferences',
        'build': 'preferences',
        'unitFields': {'horizontal': 'horizontalUnits', 'vertical': 'verticalUnits'},
        'description': (
            'Edit > Preferences, every category as a tab (open one with tab). With a document open for editing it opens in its scope: '
            'OK changes the document as one step (setViewPref for units, grids, guides and pasteboard, setLanguageQuotes for the '
            'dictionary quotation marks, setDocPrefs for the document preferences, only what changed) and then saves the app settings '
            '(userSettings patch preferences and autoCorrectTables). Without one (scope app) the document fields are greyed and only '
            'the app settings are saved. Lengths are in points. autoCorrect rows are language, word, correction.'),
        'lists': {'appColors': app_colors, 'viewColors': view_colors},
        'tabs': TABS,
        'fields': FIELDS,
    }


def text():
    return json.dumps(build(), ensure_ascii=False, indent=1) + '\n'


if __name__ == '__main__':
    out = text()
    if '--check' in sys.argv:
        sys.exit(0 if OUT.read_text(encoding='utf-8') == out else 1)
    OUT.write_text(out, encoding='utf-8', newline='\n')
    print(f'wrote {OUT.relative_to(ROOT)} ({len(FIELDS)} fields, {len(TABS)} tabs)')
