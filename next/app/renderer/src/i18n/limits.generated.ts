// Generated: python -B -I next/tools/limits-report.py
// Do not edit; edit next/oracle/rules/limits.toml and regenerate. Keep LF line endings.
export type LimitKind = 'approx' | 'unsupported' | 'unverified' | 'excluded';
export type LimitArea = { id: string; ko: string; en: string };
export type LimitDifference = { id: string; kind: LimitKind; area: string; top: number | null; ko: string; en: string };
export type LimitsData = {
  version: number; doc: string; areas: LimitArea[];
  /** Only what Folio does not support or removed on purpose (`unsupported`, `excluded`): the Help window's list. */
  differences: LimitDifference[];
};
export const LIMITS: LimitsData = {
  "version": 1,
  "doc": "artifacts/finish-20261113/LIMITS.ko.md",
  "areas": [
    {
      "id": "text",
      "ko": "글자",
      "en": "Characters"
    },
    {
      "id": "paragraph",
      "ko": "단락·조판",
      "en": "Paragraphs and composition"
    },
    {
      "id": "textpath",
      "ko": "패스 위 글자",
      "en": "Type on a path"
    },
    {
      "id": "object",
      "ko": "개체·그림",
      "en": "Objects and graphics"
    },
    {
      "id": "color",
      "ko": "색",
      "en": "Color"
    },
    {
      "id": "math",
      "ko": "수식",
      "en": "Equations"
    },
    {
      "id": "document",
      "ko": "문서 구조",
      "en": "Document structure"
    },
    {
      "id": "output",
      "ko": "출력",
      "en": "Output"
    },
    {
      "id": "import",
      "ko": "가져오기",
      "en": "Import and Place"
    },
    {
      "id": "exchange",
      "ko": "InDesign 교환",
      "en": "InDesign exchange"
    },
    {
      "id": "screen",
      "ko": "화면 도구",
      "en": "Tools and panels"
    },
    {
      "id": "excluded",
      "ko": "제외한 기능",
      "en": "Removed features"
    }
  ],
  "differences": [
    {
      "id": "book-index-toc",
      "kind": "unsupported",
      "area": "document",
      "top": 6,
      "ko": "책 문서를 포함해 만든 목차와 색인에서 다른 문서로 이동하는 하이퍼링크는 생성되지 않습니다.",
      "en": "Book-wide tables of contents and indexes do not generate hyperlinks to other documents in the book."
    },
    {
      "id": "place-formats",
      "kind": "unsupported",
      "area": "import",
      "top": 7,
      "ko": "벡터 PostScript EPS 는 해석하지 않고 파일에 든 미리 보기 그림(없으면 회색 상자)으로 화면과 PDF 에 나옵니다(사전 검사가 알립니다). 인쇄용은 PDF 로 바꿔 놓거나 IDML 로 넘기세요. Photoshop EPS 는 파일 안의 원래 그림으로 나옵니다. EMF·WMF 의 클리핑·무늬 브러시와 복사가 아닌 래스터 연산은 그리지 않습니다. SVG 안의 그림이 바깥 파일을 가리키면 그리지 않습니다(SVG 안에 담긴 그림은 그립니다). 스크립트와 애니메이션은 실행하지 않고 처음 모습을 그립니다. PDF 로 내보낼 때 SVG 의 필터·마스크·그레이디언트·무늬·불투명도·비트맵은 300 ppi 픽셀로 들어갑니다.",
      "en": "A vector PostScript EPS is not interpreted: it appears on screen and in the PDF as the preview picture it carries (a grey box when it has none), and preflight says so. For print, place it as PDF or hand over IDML. A Photoshop EPS appears as the picture it holds. Clipping, pattern brushes and raster operations other than a copy in EMF and WMF are not drawn. A picture inside an SVG that names an outside file is not drawn (a picture held inside the SVG is). Scripts and animation are not run; the first appearance is drawn. In PDF export, SVG filters, masks, gradients, patterns, opacity and bitmaps are written as 300 ppi pixels."
    },
    {
      "id": "indd-write",
      "kind": "unsupported",
      "area": "exchange",
      "top": null,
      "ko": "Folio 는 INDD 를 직접 쓰지 않습니다. InDesign 에 넘기려면 IDML 로 내보내고, INDD 가 필요하면 InDesign 에서 그 IDML 을 열어 저장하세요.",
      "en": "Folio does not write INDD files. Export IDML for InDesign; if you need an INDD, open that IDML in InDesign and save it."
    },
    {
      "id": "cloud-library",
      "kind": "unsupported",
      "area": "object",
      "top": null,
      "ko": "InDesign 라이브러리(INDB·INDL)와 클라우드 라이브러리는 열지 않습니다. Folio 라이브러리(.foliolibrary)는 프로젝트 안에 저장합니다. 돌리거나 뒤집은 개체를 글 안에 놓기, 구조·페이지 항목은 지원하지 않습니다.",
      "en": "InDesign libraries (INDB, INDL) and cloud libraries are not opened. Folio libraries (.foliolibrary) are stored in the project. Placing rotated or flipped objects inside text, and structure or page items, are not supported."
    },
    {
      "id": "duplicate-refusals",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "연결된 틀 가운데 몇 개만 복제하거나 복사할 때, 그 글에 메모·색인 표시자·하이퍼링크·XML 요소·변경 추적 같은 다른 글 안 요소가 있으면 거절하고 문서를 바꾸지 않습니다. 표·각주·변수·고정 개체는 그 글자가 든 틀과 함께 복제·복사됩니다.",
      "en": "Duplicating or copying only some frames of a threaded chain is refused (the document is left unchanged) when its text holds other inline elements such as notes, index markers, hyperlinks, XML elements or tracked changes. Tables, footnotes, variables and anchored objects go with the frame that holds their character."
    },
    {
      "id": "page-tool-liquid",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "페이지 도구의 '액체 페이지 규칙'과 '기준 페이지 오버레이 표시', 같은 문서의 보기 창을 별도 앱 창으로 떼는 것은 지원하지 않습니다.",
      "en": "Liquid page rules and the master page overlay in the Page tool, and detaching a second view of the same document into a separate app window, are not supported."
    },
    {
      "id": "conveyor-open-source",
      "kind": "unsupported",
      "area": "document",
      "top": null,
      "ko": "컨베이어에서 원본 문서가 열려 있으면 업데이트하지 않고 '저장하고 닫은 뒤 다시 하세요'라고 안내합니다.",
      "en": "If the source document of a Conveyor link is open, Folio does not update and asks you to save and close it first."
    },
    {
      "id": "print-send",
      "kind": "unsupported",
      "area": "output",
      "top": null,
      "ko": "PostScript 파일은 쓰지 않고 PDF 로 저장합니다. 책 전체를 프린터로 바로 보내기, 분판·그래픽·색상 관리·고급 설정의 출력 반영, 색 막대·슬러그·J 마크, 짝/홀수 쪽 순서, 인쇄 안 함 개체 인쇄, 마스터 페이지 인쇄는 출력에 쓰지 않습니다.",
      "en": "Folio writes PDF, not PostScript files. Sending a whole book straight to a printer, and applying separations, graphics, colour management and advanced settings, colour bars, slug, J marks, even/odd page order, printing non-printing objects and printing master pages, are not applied to output."
    },
    {
      "id": "draw-tools-rotated",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "90도 배수가 아닌 각도로 돌린 펼침면에서는 선·문자 틀 도구를, 돌린 펼침면 위의 돌아간 쪽(IDML 안에서 쪽 자체가 돈 것)에서는 사각형·타원·다각형·선·프레임·문자 틀 도구를 지원하지 않습니다. 똑바른 펼침면의 돌아간 쪽에서는 이 도구들과 한 번 눌러 치수 입력이 됩니다(쪽의 테두리 상자 왼쪽 위 기준, 개체는 펼침면에 똑바로).",
      "en": "On a spread turned by an angle that is not a multiple of 90 degrees the Line and Type tools, and on a turned page (a page turned inside the IDML) of a turned spread the Rectangle, Ellipse, Polygon, Line, frame and Type tools, are not supported. On a turned page of an upright spread these tools and entering dimensions with a click work (measured from the top-left of the page's bounding box, the object upright on the spread)."
    },
    {
      "id": "excluded-autostyle",
      "kind": "excluded",
      "area": "excluded",
      "top": null,
      "ko": "온라인으로 문단을 분류하는 자동 스타일과 스타일 팩은 지원하지 않습니다. 이 기능이 문서에 남긴 값은 그대로 열고 다시 내보냅니다.",
      "en": "Auto Style and Style Packs, which use an online service to classify paragraphs, are unsupported. Values this feature left in a document are still opened and exported unchanged."
    },
    {
      "id": "excluded-content-aware-fit",
      "kind": "excluded",
      "area": "excluded",
      "top": null,
      "ko": "내용 인식 맞춤(그림 내용에 따라 배율과 자르는 자리를 정하는 기능)은 뺐습니다. 이미 맞춘 결과는 그림의 변환 행렬로 문서에 남아 그대로 열립니다.",
      "en": "Content-Aware Fit (scale and crop chosen from picture content) was removed. A result already applied stays in the document as the picture's transform and opens unchanged."
    },
    {
      "id": "excluded-overlays",
      "kind": "excluded",
      "area": "excluded",
      "top": null,
      "ko": "디지털 출판용 오버레이 패널은 지원하지 않습니다. 개체에 저장된 상호작용 레이블은 그대로 읽고 내보냅니다.",
      "en": "The digital publishing Overlays panel is unsupported. Interaction labels attached to objects are preserved on import and export."
    },
    {
      "id": "panel-hyperlink-destinations",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "하이퍼링크 패널 메뉴의 '하이퍼링크 업데이트', '웹 주소 상태 자동 확인'과 상호 참조 형식 불러오기는 만들지 않습니다. 다른 문서의 대상으로 가는 하이퍼링크를 만들지 않아 문서 밖에서 바뀔 대상이 없습니다. 웹 주소는 자동으로 확인하지 않고 패널의 '웹 주소 상태 확인' 단추를 누를 때만 확인합니다. 하이퍼링크 대상(텍스트 앵커·페이지·URL) 만들기·옵션·안 쓰는 대상 삭제와 하이퍼링크 재설정은 있습니다.",
      "en": "The Hyperlinks panel menu items Update Hyperlink and Auto Update URL Status, and Load Cross-Reference Formats, are not built. Folio makes no hyperlinks to destinations in other documents, so no destination changes outside the document. Web addresses are not checked automatically: the panel's Check URL Status button checks them when pressed. New Hyperlink Destination (text anchor, page, URL), Destination Options, Delete Unused Destinations and Reset Hyperlink are built."
    },
    {
      "id": "panel-conditions-rest",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "조건부 텍스트 패널 메뉴의 옵션 표시는 없습니다. 패널이 조건 세트와 표시자 같은 옵션을 늘 보여 줍니다. 세트 상자의 [이름 없음]을 고르는 것만으로는 쓰는 세트를 비우지 않습니다(세트를 지우면 비워집니다).",
      "en": "The Conditional Text panel menu has no Show Options: the panel always shows the options such as condition sets and indicators. Choosing [Unnamed] in the set box alone does not clear the set in use (deleting the set does)."
    },
    {
      "id": "panel-datamerge-rest",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "데이터 필드 내용 업데이트는 Folio 에서 만든 병합 문서의 글 필드와 QR 코드 필드를 바꿉니다. 그림 필드는 바꾸지 않으니 그림이 바뀌면 병합 문서를 다시 만듭니다. 미리 보기는 글 필드와 QR 코드 필드를 바꿔 보여 주고 그림 필드는 그대로 둡니다.",
      "en": "Update Content in Data Fields changes the text and QR code fields of a merged document made in Folio; picture fields are not changed (create the merged document again when a picture changes). Preview shows the text and QR code fields with a record's values and leaves picture fields as they are."
    },
    {
      "id": "panel-control-rest",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "제어 막대 사용자 지정은 개체·문자·단락·페이지 제어 막대마다 보일 항목 묶음을 따로 고릅니다. 견본 패널의 색상 그룹만 보기는 없습니다(그룹은 목록 안에 묶여 보입니다). 이 팝업 메뉴들(정렬 기준, 견본 표시)은 그리지 않고 같은 일을 하는 선택 상자가 있습니다(정렬 기준에는 재단 여유도 있습니다).",
      "en": "Customize picks the groups shown in each control bar (object, character, paragraph, page) separately. Show Color Groups in the Swatches filter is not built (groups are shown together in the list). Those popup menus (Align to, Show Swatch Kinds) are not drawn; the same choices are select boxes in the panels (Align to includes Bleed)."
    },
    {
      "id": "menuaudit-b-book-library-menu",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "책 패널 메뉴의 EPUB 내보내기, 옛 InDesign 버전 문서 자동 변환, 라이브러리 패널 메뉴의 섬네일 보기와 Creative Cloud 라이브러리로 옮기기는 지원하지 않습니다. 라이브러리는 목록으로 봅니다. 책을 다른 이름으로 저장은 프로젝트 안 .foliobook 으로 되고, 내보낼 때 같은 이름 레이어 병합은 책 PDF 의 Acrobat 레이어를 이름마다 하나로 합칩니다.",
      "en": "The Book panel menu's Export Book to EPUB and Automatic Document Conversion, and the Library panel menu's Thumbnail Views and Migrate to CC Libraries are not supported; a library is shown as a list. Save Book As writes a .foliobook in the project, and Merge Identically Named Layers on Export joins the book PDF's Acrobat layers into one per name."
    },
    {
      "id": "menuaudit-b-ruler-custom",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "눈금자 오른쪽 클릭 메뉴의 사용자 지정 단위(Custom…)는 지원하지 않습니다. 눈금자 단위는 포인트·파이카·인치·소수 인치·밀리미터·센티미터·시세로·애게이트·픽셀 아홉 가지입니다.",
      "en": "The ruler menu's Custom… unit is not supported. Ruler units are the nine of Points, Picas, Inches, Inches Decimal, Millimeters, Centimeters, Ciceros, Agates and Pixels."
    },
    {
      "id": "keyaudit-c1-xml-structure-keys",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "XML 구조 창의 이동·펼치기·검증 오류 이동 키 16개(위/아래 이동, 선택 확장, 처음/끝 노드, 한 화면 스크롤, 요소 펼치기·접기, 검증 오류 보기)는 지원하지 않습니다. XML 구조 창 자체가 제외한 XML 계열이라 그 안의 키도 없습니다.",
      "en": "The 16 keys of the XML Structure pane (move and extend the selection, first and last node, scroll one screen, expand and collapse elements, view validation errors) are not supported. The XML family, including the Structure pane, is left out."
    },
    {
      "id": "keyaudit-c1-recompose-keys",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "\"모든 글 다시 조판\"(Ctrl+Alt+/)과 \"모든 글 즉시 다시 조판\", \"글꼴 목록 새로 고침\"은 만들지 않습니다. Folio 는 바뀐 글을 그때그때 자동으로 다시 조판하고 글꼴 목록은 열 때마다 설치된 글꼴을 읽어서 키가 할 일이 없습니다.",
      "en": "Recompose All Stories (Ctrl+Alt+/), Recompose All Stories Immediately and Update Missing Font List are not built. Folio recomposes changed text at once and reads the installed fonts each time a font list opens, so the keys have nothing to do."
    },
    {
      "id": "keyaudit-c1-unnamed-actions",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "InDesign 단축키 편집기에서 이름이 비어 있는 글·표 영역 동작 179개와 글꼴 굵기 이름(Light·Regular·Medium·SemiBold·Bold·Heavy) 6개는 지원하지 않습니다. 메뉴 항목도 기본 단축키도 없고 이름도 없는 글꼴·스타일 목록의 내부 항목이라 사용자가 하는 일로 보이지 않습니다. 글꼴 스타일은 문자 패널에서 고릅니다.",
      "en": "The 179 text and table actions with an empty name in the InDesign shortcut editor and the six font weight names (Light, Regular, Medium, SemiBold, Bold, Heavy) are not supported. They have no menu item, no default key and no name; they look like internal entries of the font and style lists, not user actions. Choose a font style in the Character panel."
    },
    {
      "id": "keyaudit-c1-view-keys",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "확대 비율·쪽 번호 상자 초점 키와 보기 최적화 무시는 만들지 않습니다. 입력칸은 Tab 으로 옮깁니다.",
      "en": "The zoom and page-number box focus keys and Suppress Overrides are not built; fields are reached with Tab."
    },
    {
      "id": "keyaudit-c1-workspace-keys",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "기본 작업 영역 9개의 단축키와 시작 화면 재설정은 지원하지 않습니다. Folio에서는 \"필수 클래식\"과 직접 저장한 작업 영역을 쓸 수 있습니다.",
      "en": "Shortcuts for the nine preset workspaces and Reset Start are unsupported. Folio provides Essentials Classic and any workspaces you save."
    },
    {
      "id": "keyaudit-c1-adobe-service-keys",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "Bridge 로 찾아보기, 버전, XML, 확장 관리, InCopy 할당 지우기, FPO 를 원본으로 바꾸기·원본 내려받기, 색 테마 도구(Shift+I), 교정 설정의 Macintosh·Windows·Monitor RGB 는 지원하지 않습니다. 온라인 서비스, InCopy·XML 기능, 모니터와 운영체제 색상을 흉내 내는 프로파일은 포함하지 않습니다.",
      "en": "Browse in Bridge, Versions, XML, Manage Extensions, Delete Assignment, Replace All FPOs With Originals, Download All Originals, the Color Theme tool (Shift+I) and the Proof Setup Macintosh, Windows and Monitor RGB entries are not supported. Online services, InCopy and XML features, and profiles for simulating monitor or operating system colors are unavailable."
    },
    {
      "id": "keyaudit-c1-live-edit",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "Live Edit 모드(4 키), Magnetic Guide, Zone Guide 는 지원하지 않습니다. 대응하는 Folio 기능이 없습니다.",
      "en": "Live Edit mode (the 4 key), Magnetic Guide and Zone Guide are not supported. Folio has nothing that corresponds."
    },
    {
      "id": "dbmerge-write-back",
      "kind": "unsupported",
      "area": "document",
      "top": null,
      "ko": "데이터 병합은 데이터베이스를 읽기만 합니다. 데이터베이스에 쓰기(되쓰기)와 저장 프로시저 실행은 지원하지 않습니다.",
      "en": "Data Merge only reads a database. Writing back to it and running stored procedures are not supported."
    },
    {
      "id": "dbmerge-schedule",
      "kind": "unsupported",
      "area": "document",
      "top": null,
      "ko": "데이터베이스 원본을 정해진 때에 저절로 다시 가져오는 예약 실행은 지원하지 않습니다. 대신 '이번 달' 같은 매개변수와 문서를 열 때 새로 고침을 씁니다.",
      "en": "Fetching a database source again on a schedule is not supported. Use parameters such as 'this month' and refresh when the document opens instead."
    },
    {
      "id": "dbmerge-sqlserver-winauth",
      "kind": "unsupported",
      "area": "document",
      "top": null,
      "ko": "SQL Server·Azure SQL 은 SQL 로그인(사용자 이름과 비밀번호)으로만 연결합니다. Windows 통합 인증과 Microsoft Entra(Azure AD) 로그인은 지원하지 않습니다.",
      "en": "SQL Server and Azure SQL connect with a SQL login (user name and password) only. Windows integrated authentication and Microsoft Entra (Azure AD) sign-in are not supported."
    },
    {
      "id": "pdfx13-solid-vector",
      "kind": "unsupported",
      "area": "output",
      "top": null,
      "ko": "PDF/X-1a·X-3에서 투명하게 겹치는 단색을 평탄화할 때 해당 부분을 벡터 조각으로 보존하는 기능은 지원하지 않습니다.",
      "en": "Keeping overlapping solid colors as vector pieces when flattening transparency for PDF/X-1a or PDF/X-3 is not supported."
    },
    {
      "id": "text-grep-find-change",
      "kind": "unsupported",
      "area": "text",
      "top": null,
      "ko": "찾기/바꾸기의 GREP 검색과 바꾸기는 지원하지 않습니다. 일반 글·글리프 찾기/바꾸기와 단락의 GREP 스타일은 사용할 수 있습니다.",
      "en": "GREP Find/Change is unsupported. Literal text and glyph Find/Change and paragraph GREP styles are available."
    },
    {
      "id": "ai-panel-withheld",
      "kind": "unsupported",
      "area": "screen",
      "top": null,
      "ko": "AI 패널은 물리 프린터로 인쇄하거나 MCP에서 차단한 명령을 실행할 수 없습니다. 연결 키와 대화 세션은 사용자가 패널에서 관리합니다.",
      "en": "The AI panel cannot print to a physical printer or run withheld MCP commands. Manage connection keys and chat sessions in the panel."
    }
  ]
};
