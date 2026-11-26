# Folio MCP

`folio-mcp`는 표준입출력 MCP 서버다. 두 가지로 쓴다.

- **실행 중인 앱에 붙기 (`folio-mcp --attach`, mcp-c3).** LLM이 *지금 화면에 열린 Folio*를 조작한다. 열린 문서·선택·현재 페이지·공통 대화상자·패널·최근 목록을 다루고, 화면은 바뀐 것을 바로 그린다. 아래 "실행 중인 앱에 붙기".
- **프로젝트 폴더 하나 (`folio-mcp <프로젝트 폴더>`, mcp-c2 첫판).** 앱 없이 이 프로세스 안에서 엔진 라이브러리를 부른다. 그 아래 절들.

사용자 MCP 설정은 자동으로 바꾸지 않는다.

## 실행 중인 앱에 붙기 (`--attach`)

### 열린 문서의 글과 스타일

글과 표의 칸은 `textStory`와 `tableInfo.table.cells`로 ID를 확인한 뒤 편집한다. 글 범위의 시작은 포함하고 끝은 제외한다. 위치는 유니코드 문자 수로 센다. `textFind`·`edit.findReplace`는 일반 글과 서식을, `glyphFind`·`edit.glyphReplace`는 지정한 글꼴의 글리프를 다룬다. 단락·문자·표·칸 스타일을 고치면 열린 문서 안에서 그 스타일을 쓰는 곳에 함께 반영된다.

`edit.placeText`는 프로젝트 안의 TXT·RTF·DOCX 상대 경로를 받으며 파일 쓰기 스위치가 필요하다. 목차·색인·목록 번호를 만들 때 쪽 번호와 위치는 열린 문서의 조판 결과에서 얻는다. 책의 다른 문서까지 포함하는 목차는 거절한다. 편집 응답에는 만든 ID 목록과 `LLM: 명령이름` 실행 취소 이름이 들어간다. 각 도구의 인수와 예제는 `tools/list`에서 확인할 수 있다.

### 열린 문서의 개체·페이지·색·출력

`pageItems`로 개체 ID를, `documentInfo`로 레이어·견본·스타일·페이지·부모 ID를 얻는다. 개체를 복제·묶기·정렬·분포하거나 값으로 옮길 수 있고, 칠·획·스타일·효과를 바꿀 수 있다. 새 레이어·견본·색조·그라디언트·부모를 만들면 응답의 `created`에 ID가 들어간다. 페이지는 한 쪽씩 추가·삭제할 수 있다. 복제 단위는 앱과 같은 펼침면이며 `edit.duplicateSpread`에는 `open` 결과의 `document.pages[].spread`를 준다. 구역에 지정한 번호는 `pageRange`로 찾을 수 있다.

그림을 바꿀 때는 `links` 결과에서 대상의 `report.links[].owner`를 읽어 `edit.relinkGraphic`의 `id`로 주고, `file`에는 프로젝트 상대 경로를 준다. 원본 파일이 같은 이름으로 수정됐을 때도 그 경로로 다시 연결한다. 다음 편집에 쓸 대상 ID는 `links`에서 다시 읽는다. 프로젝트가 관리하는 그림에는 저장된 `sha256`과 프로젝트 상대 경로 `projectPath`가 함께 나온다.

`preflight`로 오류를 확인하고 `preflightSettings`로 설정과 활성 프로파일을 읽는다. 프로파일과 범위 변경은 각각 `edit.setPreflightProfiles`와 `edit.setPreflightSettings`로 한다. PDF/X 출력은 `export`, 데이터 병합 문서 만들기는 `dataMergeCreate`, 인쇄소용 패키지는 `packagePlan`으로 내용을 확인한 뒤 `package`를 쓴다.

### 켜는 법

1. Folio에서 **환경 설정 ▸ 일반 ▸ "LLM 연결(MCP) 허용"** 을 켠다(기본은 꺼짐). 사용자 설정 `folio.mcp.allowConnection`이며, 화면 없이 엔진 명령 `{"cmd":"userSettings","values":{"folio.mcp.allowConnection":true}}`로도 읽고 쓴다.
2. 켜는 순간 앱의 엔진이 **이 컴퓨터 안에서만** 열리는 연결점을 연다. 끄면 바로 닫는다(열려 있던 연결도 다음 요청에서 끊는다).
   - 윈도우: 이름 있는 파이프 `\\.\pipe\folio-mcp-<임의 값>`. 원격(네트워크) 클라이언트는 거절하고(`PIPE_REJECT_REMOTE_CLIENTS`), 같은 이름의 첫 인스턴스만 만든다(가로채기 방지).
   - 맥·리눅스: 유닉스 소켓 `<사용자 데이터>/folio-mcp-<pid>.sock`(경로가 너무 길면 사용자 임시 폴더), 권한 0600.
   - iOS·Android: 연결점이 없다(cfg로 뺐다). TCP는 어느 OS에서도 쓰지 않는다.
3. 열 때마다 **새 임의 토큰**(OS 난수 244비트)을 만들어 연결점과 함께 사용자 데이터 폴더의 `mcp-attach.json`에 적는다(`{version, endpoint, token, pid}`). 맥·리눅스는 파일을 0600으로 만들고, 윈도우는 사용자 프로필 아래 앱 데이터 폴더(`%APPDATA%\folio-studio-next`, 그 사용자·SYSTEM·관리자만 읽는 상속 권한)에 둔다. 앱이 끝나거나 설정을 끄면 지운다(다른 앱이 덮어쓴 파일은 건드리지 않는다).
4. 데이터베이스까지 맡기려면 **"LLM 이 데이터베이스 원본을 새로 고칠 수 있음"**(`folio.mcp.allowDatabaseRefresh`, 기본 끔)도 켠다. 아래 "비밀번호와 데이터베이스".
5. 저장·내보내기·그림 놓기까지 맡기려면 **"LLM 이 파일을 쓸 수 있음"**(`folio.mcp.allowFileWrites`, 기본 끔)도 켠다. 열린 프로젝트 안에서만 쓴다. 아래 "경로를 받는 도구와 경로 규칙".

### 붙는 법

```sh
folio-mcp --attach                       # 앱의 사용자 데이터 폴더에서 mcp-attach.json 을 읽는다
folio-mcp --attach --connection FILE     # 연결 파일을 직접 준다
folio-mcp --attach PROJECT_FOLDER        # 앱이 없으면 그 프로젝트 폴더로(첫판처럼) 돈다
```

연결 파일은 `FOLIO_USER_DATA`가 있으면 그 폴더에서, 없으면 Electron의 사용자 데이터 폴더(윈도우 `%APPDATA%\folio-studio-next`, 맥 `~/Library/Application Support/folio-studio-next`, 리눅스 `$XDG_CONFIG_HOME` 또는 `~/.config/folio-studio-next`)에서 찾는다. Claude Code 예:

```sh
claude mcp add --transport stdio folio -- C:/path/to/folio-mcp.exe --attach
```

```json
{ "mcpServers": { "folio": { "command": "C:/path/to/folio-mcp.exe", "args": ["--attach"] } } }
```

**앱이 없거나 설정이 꺼져 있으면** 서버는 그래도 뜨고, 도구 `appConnection`이 `{attached:false, code, reason, howTo}`로 이유(`notRunning`·`refused`·`badFile`)와 켜는 법을 알려 준다. 프로젝트 폴더를 함께 줬으면 그 폴더의 첫판 도구가 이 프로세스에서 돈다(화면과는 무관하다고 `instructions`에 적는다). 안 줬으면 `coverage`·`appConnection`만 있다. 붙는 것은 서버가 시작할 때 한 번 정하므로, 앱을 켠 뒤에는 MCP 서버(클라이언트)를 다시 시작한다.

### 붙으면 되는 것

도구 목록은 `src/bin/mcp/attached.rs`의 `ATTACHED`·`DATABASE` 한 곳에서 나온다. 엔진도 같은 파일로 **모든 요청을 다시 검사**한다(클라이언트를 믿지 않는다).

| 묶음 | 도구 |
| --- | --- |
| 열린 문서·이력·선택·페이지 | `documentInfo`, `pageItems`, `editState`, `undo`, `redo`, `viewState`, `setSelection`, `goToPage` |
| 공통 대화상자(`next/shared/dialogs.json`) | `dialogState`, `openDialog`, `setDialogField`, `applyDialog`, `cancelDialog` |
| 패널·최근 목록 | `panelState`, `setPanelState`, `recentFiles`, `setRecentFiles`, `rememberRecent` |
| 데이터 병합 | `dataMergeSource`(프로젝트 안 CSV·TXT), `dataMergeEvaluate`, `dataMergeSourceStatus`(`check:true`는 데이터베이스 설정 필요) |
| 검토한 편집 `edit.*` | 만들기(`createByDrag`·`createPolygon`·`createInView`), 변형(`scaleAbout`·`shearAbout`·`rotateByDrag`·`resizeByHandle`·`stepRepeat`), 속성(`setObjectAttributes`·`formatFrameText`·`setFrameFitting`·`clearFrameFitting`), 패스(`joinPaths`·`pathfinder`·`createOutlines`), 글(`typeText`·`insertNote`·`deleteNote`), 조건(`createCondition`·`setConditionVisible`), 페이지(`setPageSize`·`movePage`), 데이터 병합 필드(`insertDataMergeTextField`·`insertDataMergeImageField`·`insertDataMergeQrField`·`setDataMergeComputedFields`·`setDataMergeRefreshMode`·`removeDataSource`·`mergeRecords`) |
| 데이터베이스(설정이 켜졌을 때만) | `edit.dataMergeSelectDatabase`, `edit.dataMergeRefresh` |
| 프로젝트 경로(mcp-c4, 아래 규칙) | `open`, `save`, `saveCopy`, `exportIdml`, `export`, `dataMergeCreate`, `edit.placeGraphic`, `edit.selectDataSource`, `edit.importPdfComments` |
| 가져오기·패키지·스타일·책·라이브러리(mcp-c5, 같은 규칙) | `importPlan`, `importRun`, `importCancel`, `packagePlan`, `package`, `packageCancel`, `stylesFile`, `edit.loadStyles`, `book`·`library`(`read`·`new`·`create`) |
| 책·라이브러리 편집, 새 문서·새 프로젝트(mcp-c6, 같은 규칙) | `book`(`add`·`synchronize`·`exportPdf`), `library`(`store`·`place`), 앱 대화상자 `newDocument`·`newBook`·`newLibrary`·`newProject`(`openDialog`·`setDialogField`·`applyDialog`로) |
| 본문·표·페이지 검사 | `textStory`, `textFind`, `styleInfo`, `tableCells`, `render`, `inks`, `preflight`, `links` 등. 조회 98개는 `coverage.attached.exposed`에서 확인 |
| 프로젝트 파일 검사 | `placeInfo`, `fileMetadata`, `textImportInfo` 등. 열린 프로젝트 안의 기존 파일만 읽음 |
| MCP 전용 | `coverage`, `appConnection` |

검토한 편집의 기준: 인수가 열린 문서 안의 id와 값뿐이고, 경로·다른 파일·미리보기 세션·컨베이어를 받지 않는다. 경로를 받는 것은 아래 "경로를 받는 도구와 경로 규칙"을 따르는 위 두 줄의 도구만 연다. 나머지 경로 명령(책의 옮기기·빼기·쪽 번호·패키지, 라이브러리 자산 고치기·빼기, 부모 페이지·획 스타일 파일 등)·프린터·엔진 프로세스 제어는 붙은 연결에서도 열지 않는다. `coverage`의 답에 `attached` 칸이 있어 이름마다 `exposed`·`needsDatabaseSetting`·`needsFileSetting`·`withheld`를 알려 준다(목록 전체의 답에는 `attached.paths`에 도구마다 받는 인수와 쓰기 설정 필요 여부).

**같은 세션.** 붙은 연결의 요청은 화면의 요청과 같은 엔진 요청 줄(`main.rs`의 한 고리)로 들어가 같은 문서 세션에 닿는다. 답은 요청한 연결에만 돌아가고(id로 나눈다), 알림은 늘 화면으로 간다. 그래서

- LLM이 만든 개체·편집·실행 취소는 `external-edit` 알림으로 화면이 바로 다시 그린다(화면이 자기 편집 뒤에 하는 일과 같다). 상태 줄에 "LLM 이 문서를 바꿨습니다: …"가 뜬다.
- 실행 취소 기록에 **"LLM: …"** 이 붙는다(편집 ▸ 실행 취소 "LLM: createByDrag", 대화상자 적용은 "LLM: 단계 및 반복").
- 대화상자·선택·패널은 기존 `dialog-state`·`view-state`·`panel-state` 알림으로 화면에 보인다. LLM이 연 대화상자는 화면에 열리고, 채운 값이 칸에 보이고, 적용하면 닫힌다.

### 붙은 앱에서 읽기

| 호출 예 | 결과 |
| --- | --- |
| `textStory {"story":"walk"}` | 스토리의 본문과 서식 |
| `styleInfo {"kind":"paragraph","style":"Masthead"}` | 지정한 스타일의 속성 |
| `render {"page":0,"longEdge":1200,"selection":true}` | MCP 이미지 블록의 base64 PNG, 실제 너비와 높이 |
| `render {"page":0,"region":[10,10,70,50],"scale":2,"guides":true}` | 쪽 왼쪽 위 기준 포인트 좌표로 자른 그림. 배율은 포인트당 픽셀 수 |
| `links {}` · `preflight {}` | 문서의 링크 목록 · 프리플라이트 검사 결과 |

그림의 쪽 번호는 0부터 센다. 긴 변은 기본 1,200px, 최대 2,000px다. 압축한 PNG가 응답 한도를 넘으면 긴 변을 줄인다. 선택 강조와 안내선은 기본으로 끄며 표시 옵션은 앱의 선택이나 보기를 바꾸지 않는다.

조회 결과가 96 KiB를 넘거나 `resultOffset` 또는 `resultLimit`을 주면 직렬화한 JSON을 나눠 준다. `chunk`를 차례로 이어 붙인 뒤 JSON으로 해석한다. 같은 인수로 `nextOffset`을 다음 `resultOffset`에 넣고, 값이 null이면 끝이다. 단위는 유니코드 문자(스칼라)이고 한 조각은 최대 32,768자다. 여러 번 읽는 동안 문서를 편집하면 결과가 달라질 수 있다.

파일 입력에는 프로젝트 상대 경로를 쓴다. 결과의 프로젝트 경로는 상대 경로로 표시하고 외부 로컬 주소는 가린다. 파일 메타데이터에는 크기·수정 시각·해석한 XMP 필드만 담는다. 읽기와 쓰기를 함께 제공하는 도구도 등록된 조회 인수만 받는다. 보류 사유와 이름은 `coverage` 결과의 `attached.withheld`에 있다.

### 경로를 받는 도구와 경로 규칙 (mcp-c4, 2026-11-20)

붙은 연결도 프로젝트 안 파일을 열고 저장하고 내보낼 수 있다. 대원칙 1(프로젝트 상대 경로)과 "LLM 조작"(엔진 권한을 넓히지 않는다)을 지키려고 **규칙을 먼저 정하고 그대로 만들었다.** 규칙은 엔진(`src/attach_paths.rs`)이 요청마다 검사하고, 도구 목록·인수는 `src/bin/mcp/attached.rs`의 `PATHS` 한 곳에서 나온다.

**기준 폴더.** 경로는 **지금 앱에 열린 프로젝트**의 루트 기준이다. 열린 `.folio`가 있으면 그 문서의 프로젝트, 없으면 화면이 마지막으로 연 프로젝트(화면의 `projectInfo`). 둘 다 없으면 경로를 받는 도구는 전부 거절한다. 클라이언트가 루트를 고를 수는 없다.

**경로 모양** (mcp-c1의 `RelPath::parse`와 `Boundary`를 그대로 쓴다):

1. `/` 구분, 이름은 NFC로 바꿔 쓴다(NFD로 줘도 같은 파일). 모든 OS에서 쓸 수 있는 이름만.
2. 거절: 빈 경로, `.`·`..`(어디에 있든), `/…`, 드라이브 문자(`C:…`), UNC(`\\server\…`), 역슬래시, URI(`file://…`), `:`가 든 이름(대체 데이터 흐름 `a.idml:x`), 예약 이름(`CON`·`NUL`…), 이름 끝의 점·공백, 빈 이름(`a//b`), 제어 문자.
3. 붙은 연결만의 추가 거절: 첫 이름이 `@`로 시작하는 경로(이름 붙인 위치 `@사진창고/…`는 사람이 앱에서 고른 것만), 앱 내부 폴더 `.folio/`(잠금·캐시) 아래, `folio.project`.
4. 링크: 프로젝트 루트 자체가 링크면 거절한다. 요청한 경로의 이름을 **루트에서 대상까지 하나씩** 보고, 어느 단계가 심볼릭 링크·정션·재분석 지점이면 거절하고, 있는 단계의 실제 경로(파일을 열어 OS에 묻는 최종 경로)가 루트 밖이면 거절한다. 명령이 그 경로를 쓴 뒤 답을 돌려주기 전에 **같은 검사를 한 번 더** 한다: 그 사이에 폴더가 링크로 바뀌었으면 답 대신 거절("the project changed while Folio used this path")을 돌려준다(이미 쓴 파일을 지우지는 않는다). (mcp-c5, 2026-11-20: 예전에는 요청마다 트리 전체(10만 항목까지)를 훑어 링크가 하나라도 있으면 전부 거절했다. 파일 5만 개 프로젝트에서 경로 한 번 검사가 990 ms → 0.35 ms. 트리 다른 곳의 링크는 이제 막지 않는다: 프로젝트의 자체 훑기(해시 색인·문서 찾기·트리)는 링크를 따라가지 않으므로 경로 도구로 닿지 않는다.)

**읽기와 쓰기.**

5. 읽는 파일은 이미 있는 프로젝트 파일만이고 종류가 맞아야 한다: `open`은 `.folio`, 데이터 원본은 `.csv`·`.txt`, PDF 주석은 `.pdf`, 그림 놓기는 엔진이 형식을 다시 본다.
6. 파일을 쓰는 도구(저장·사본·IDML·PDF 내보내기·병합 문서 만들기·그림 놓기)는 설정 **"LLM 이 파일을 쓸 수 있음"**(`folio.mcp.allowFileWrites`, 기본 끔)이 켜졌을 때만 된다. 꺼져 있으면 엔진이 거절하고 켜는 곳을 알려 준다.
7. 사본(`.folio`)과 내보내기(`.idml`·`.pdf`)는 프로젝트 안 **새 경로**에만 쓴다(부모 폴더는 이미 있어야 한다). 있는 파일은 덮어쓰지 않는다. 예외 하나: 이 앱 세션에서 **사람이 화면으로 내보낸 바로 그 파일**(IDML·PDF)은 같은 종류의 내보내기로 다시 쓸 수 있다(사람이 고른 내보내기 대상). `.indd`·`.folio`·`folio.project`는 이 예외에도 들지 않는다.
8. 원본 INDD·IDML은 읽기만 한다. 덮어쓰기는 7번이 막고, 지우는 도구는 없다. 저장(`save`)은 지금 열린 `.folio` 자신만 다시 쓴다(경로 인수 없음).
9. `open`은 저장하지 않은 변경이 있으면 거절한다(사람의 변경을 버리지 않는다). 연 문서는 화면이 바로 보여 준다(`external-open` 알림).
10. 병합 문서(`dataMergeCreate`): 데이터 원본은 프로젝트 안 CSV·TXT. 새 문서 이름 `name`은 이름 하나(`/` 없이 쓸 수 있는 이름)이고 문서는 열린 문서 옆에 생긴다. 원본의 그림 열이 프로젝트 밖(절대 경로·`..`)을 가리키면 그 그림은 없는 그림(빈 프레임)으로 친다.
11. 경로 검사를 건너뛰는 인수는 받지 않는다: `selectDataSource`의 `source`·`file_type`, `importPdfComments`의 `source`·`comments`(미리 읽은 값), `placeGraphic`의 `source`(그림 정보와 주소), `export`의 `printingJob`·`booklet`.
12. 엔진이 마지막에 다시 검사하고 바꾼다: 붙은 요청의 상대 경로를 엔진이 그 프로젝트 루트 아래 이 컴퓨터 경로로 바꿔 앱 명령과 같은 함수에 넘긴다. 그래서 결과는 화면에서 한 것과 같고, 문서에 저장되는 참조는 원래대로 프로젝트 상대 경로 + SHA-256이다. 답에 나오는 경로도 프로젝트 상대 경로다.

**공개한 경로 도구.**

| 도구 | 경로 | 쓰기 설정 |
| --- | --- | --- |
| `open` | 프로젝트 안 `.folio` | 필요 없음 |
| `save` | 없음(열린 `.folio`) | 필요 |
| `saveCopy` | 새 `.folio` | 필요 |
| `exportIdml` | 새 `.idml`(또는 사람이 내보낸 그 파일) | 필요 |
| `export` (PDF) | 새 `.pdf`(또는 사람이 내보낸 그 파일) | 필요 |
| `dataMergeCreate` | 프로젝트 안 CSV·TXT, 새 문서 이름 | 필요 |
| `edit.placeGraphic` (MCP가 만든 도구) | 프로젝트 안 그림 `file` | 필요 |
| `edit.relinkGraphic` | 그림 대상 `id`, 프로젝트 안 교체 그림 `file` | 필요 |
| `edit.selectDataSource` | 프로젝트 안 CSV·TXT | 필요 없음(문서 편집) |
| `edit.importPdfComments` | 프로젝트 안 PDF | 필요 없음(문서 편집) |
| `importPlan` (mcp-c5) | 원본 `source`: 프로젝트 안 `.indd`·`.indt`·`.idml`, `document`: 새 `.folio`(기본: 원본 이름으로 루트에) | 필요 없음(계획만) |
| `importRun` | 같음. 프로젝트는 엔진이 열린 프로젝트로 정한다. 원본의 링크가 프로젝트 밖을 가리키면 들이지 않는다(없는 링크 + 원래 주소 힌트) | 필요 |
| `packagePlan`·`package` | `dir`: 프로젝트 안 **새 폴더**(부모는 있어야 한다). 이 컴퓨터의 검색 폴더·보호 폴더·PDF 설정 개체는 받지 않는다 | `package`만 필요 |
| `importCancel`·`packageCancel` | 없음 | 필요 없음 |
| `stylesFile` | 프로젝트 안 `.folio`·`.idml`(스타일과 겹치는지 보기) | 필요 없음 |
| `edit.loadStyles` (MCP가 만든 도구) | 프로젝트 안 `.folio`·`.idml` `path`. 엔진이 대화상자처럼 그 파일의 스타일 부분을 읽는다(클라이언트는 `parts`를 줄 수 없다) | 필요 없음(문서 편집) |
| `book`·`library` | 동작 `read`(`path`의 `.foliobook`·`.foliolibrary` 열기), `create`(새 `path` + `name`), `new`(프로젝트 안 폴더 `folder` + 이름 `fileName`). `root`는 엔진이 채운다. mcp-c6: 책 `add`(`document`: 프로젝트 안 `.folio`·`.idml`), `synchronize`(책의 문서마다 경로 검사, 문서를 쓴다), `exportPdf`(책의 문서마다 경로 검사, `output`: 프로젝트 안 새 `.pdf`, PDF 인수는 `export`와 같은 것만), 라이브러리 `store`(열린 문서의 `ids`, 그림은 `assets/library/`로), `place`(`asset`을 `page`에, 또는 `story`·`start`·`end` 글 안에; 엔진이 화면과 같은 편집 `placeLibraryAsset`을 앱 세션에서 바로 돌린다, 실행 취소 "LLM: placeLibraryAsset"). 다른 동작은 거절 | `read`만 필요 없음 |

엔진 등록표에 없는 `placeGraphic`은 MCP 쪽 도구 `edit.placeGraphic`(인수 `file`·`page`·`point`·`rect`·`layer`·`id`)으로 만들었다. 앱이 그림을 놓을 때 쓰는 같은 편집 명령 `placeGraphic`으로 바뀐다. 등록표의 `edit.loadStyles`는 대화상자가 미리 읽은 `parts`를 받으므로, MCP 쪽 도구 `edit.loadStyles`(인수 `path`·`kinds`·`clash`·`only`)로 만들었다. 엔진이 `path`를 검사하고 그 파일의 `Resources/Styles.xml`·`Graphic.xml`을 `parts`로 바꿔 같은 편집 명령으로 보낸다.

가져오기·패키지·책·라이브러리로 파일을 만들면 화면에 `external-file` 알림이 가서 상태 줄에 "LLM 이 파일을 썼습니다: …"가 뜨고 프로젝트 트리가 새로 고쳐진다(저장·내보내기와 같다).

여전히 열지 않는 것: 책의 옮기기·빼기·다시 연결·쪽 번호·장 번호·사전 검사·상호 참조·패키지, 라이브러리 자산 빼기·고치기·레이블·다시 연결, 부모 페이지·획 스타일·사전 검사 프로파일·인쇄 설정 파일, 지우기, 프린터, 엔진 프로세스 제어, 직접 `projectCreate`·`documentNew`(대화상자로만).

### 새 문서·새 프로젝트와 앱 대화상자 (mcp-c6, 2026-11-20)

붙은 연결도 `openDialog`·`setDialogField`·`applyDialog`로 화면의 대화상자를 다룬다. 앱 대화상자(문서 없이 여는 것)는 파일·폴더를 만들 수 있어서 규칙을 더했다. 엔진이 요청마다 검사한다(`attach_paths.rs` `dialog`, `attached.rs` `APP_DIALOGS`).

1. 붙은 연결이 여는 앱 대화상자는 새 문서(`newDocument`)·새 책(`newBook`)·새 라이브러리(`newLibrary`)·새 프로젝트(`newProject`)·사용자(`user`)·언어(`language`)·문서 사전 설정(`documentPreset`)뿐이다.
2. **새 문서·새 책·새 라이브러리**는 열린 프로젝트 안에만 만든다. `project` 칸은 엔진이 열린 프로젝트로 채운다. 클라이언트가 `openDialog` 값이나 `setDialogField`로 주면 거절한다.
3. **새 프로젝트**는 사람이 고른 폴더 안에만 만든다. 이 폴더는 새 사용자 설정 **환경 설정 ▸ 일반 ▸ "LLM 이 새 프로젝트를 만들 수 있는 폴더"**(`folio.mcp.newProjectFolder`, 이 컴퓨터의 경로라 사용자 설정에만 있다)에 둔다. 비어 있으면 `openDialog newProject`를 `{reason: "noNewProjectFolder", setting, howTo}`로 거절한다. 폴더가 있으면 엔진이 `parent`를 그 폴더로 채운다(클라이언트는 이름 `name`만 정한다).
4. 2·3의 대화상자를 적용하려면 "LLM 이 파일을 쓸 수 있음"이 켜져 있어야 한다. 열린 문서에 저장하지 않은 변경이 있으면 거절한다(화면이 사람의 대화상자처럼 만든 문서·프로젝트를 연다). 바꾸기 단추(`replace`)도 거절한다(덮어쓰지 않는다).
5. 적용할 때는 대화상자가 실제로 보낼 앱 요청(`dialog_state::attached_dialog`)을 엔진이 먼저 검사한다. 사람이 화면에서 열어 채운 대화상자라도 같은 규칙이다. 예를 들어 밖의 폴더를 채운 새 프로젝트 대화상자는 붙은 연결이 적용하지 못한다.
6. **LLM 설정은 사람만 바꾼다.** `folio.mcp.*` 칸(LLM 연결 허용·데이터베이스·파일 쓰기·새 프로젝트 폴더)은 `openDialog` 값과 `setDialogField`로 줄 수 없다. 환경 설정을 적용할 때 `folio.mcp.*` 값이 지금과 달라지면(사람이 화면에서 바꿨어도) 붙은 연결의 적용을 거절한다. (mcp-c6 전에는 붙은 LLM이 환경 설정 대화상자로 "LLM 이 파일을 쓸 수 있음"을 스스로 켤 수 있었다.)
7. 대화상자 답의 경로는 열린 프로젝트(새 프로젝트는 그 폴더) 기준이다: `project: "."`, 새 문서 `제목 없음-1.folio`, 새 프로젝트 `Mag`.
8. 거절 답에는 엔진이 준 `reason`·`setting`·`howTo`(대화상자 칸 오류는 `field`)가 그대로 실린다.

### 보안

- 연결점은 이 컴퓨터 안에서만 열린다(파이프는 원격 거절, 소켓은 0600, TCP 없음). 설정이 꺼져 있으면 연결점이 아예 없다.
- 첫 줄은 `{"attach":1,"token":…}`이어야 한다. 토큰이 없거나 틀리면 아무 요청도 읽지 않고 거절한 뒤 닫는다(비교는 시간이 위치에 따라 달라지지 않게 한다). 토큰은 앱 세션마다 새로 만든다.
- 엔진이 요청마다 허용 목록·인수 이름을 다시 검사한다. 프로젝트 권한(원본 읽기 전용, 프로젝트 안 상대 경로)은 붙어도 넓어지지 않는다: 경로는 위 규칙대로 엔진이 열린 프로젝트에 맞춰 검사하고 바꾸며, 파일 쓰기는 따로 켜는 설정이 있어야 하고, `dataMergeSource`는 원래부터 프로젝트 안 상대 경로만 읽는다.
- 같은 사용자로 도는 다른 프로그램은 연결 파일을 읽을 수 있다(사용자 권한 경계). 그래서 기본이 꺼짐이고, 쓰지 않을 때는 끈다.

### 비밀번호와 데이터베이스

비밀번호는 여전히 MCP로 다니지 않는다. 키 이름에 password·passphrase·secret·credential·apikey·accesstoken·bearer가 들어간 인수는 folio-mcp가 `-32602`로, 엔진이 한 번 더 거절한다(값은 되풀이하지 않는다). 데이터베이스 연결은 사람이 앱 화면에서 잠금 해제한다. 붙은 LLM은 **설정 "LLM 이 데이터베이스 원본을 새로 고칠 수 있음"이 켜졌을 때만** 그 잠금 해제된 연결로 가져오기(`edit.dataMergeSelectDatabase`)·새로 고침(`edit.dataMergeRefresh`)·변경 확인(`dataMergeSourceStatus` `check:true`)을 할 수 있다. 꺼져 있으면 엔진이 거절하고 켜는 곳을 알려 준다. 연결 저장·잠금 해제·시험(`dbSaveConnection`·`dbUnlockConnection`·`dbTestConnection`)은 붙어도 열지 않는다.

### 시험과 실제 연결 (mcp-c3, 2026-11-20)

- 엔진 시험 `cargo test --release --target-dir target -p folio-engine --test mcp_attach`(4개): 허용 목록·데이터베이스 설정·비밀 거절, 도구가 등록표에서 나오고 `coverage`가 이름마다 붙은 상태를 알려 줌, 설정 끔이면 연결 파일·연결점 없음 → 켜면 바로 열림 → 토큰 없음·틀림 거절 → 끄면 닫힘, 붙은 연결의 만들기·선택·대화상자 열기→채우기→적용·실행 취소가 앱 세션의 상태(`editState`·`viewState`)와 같고 화면 알림(`external-edit`·`dialog-state`·`view-state`)이 나감.
- 화면 검사 `next/oracle/mcp-attach-ui-check.py`: 숨긴 격리 앱(설정을 켠 사용자 데이터)에 붙어 사각형 만들기 → 선택 → 단계 및 반복 열기 → 개수 2 → 적용 → 실행 취소. 화면의 개체 목록·열린 대화상자와 칸 값·편집 ▸ 실행 취소 이름("LLM: …")·상태 줄을 영수증으로 남긴다. 기본 클라이언트는 이 파일 안의 작은 표준입출력 클라이언트(파이프라인용), `--sdk`는 공식 TypeScript SDK 클라이언트 2.4.0(`artifacts/mcp-c3-20261023/tools/node`, 처음 쓸 때 그 폴더에 설치), `--claude`는 Claude Code 한 번(haiku, `--strict-mcp-config`, 내장 도구 끔, 사용자 설정 안 바꿈). 결과는 `artifacts/mcp-c3-20261023/results/`.

### 경로 시험과 실제 연결 (mcp-c4, 2026-11-20)

- 엔진 시험(같은 `--test mcp_attach`, 이제 6개): `attached_path_policy` — 경로 도구는 이름과 검토한 인수로만 들어오고, 쓰기는 설정이 꺼져 있으면 거절, 경로 검사를 건너뛰는 인수(`source`·`comments`·`printingJob`·`settings`·`_projectImages` 등) 거절, 도구 목록·`coverage`. `attached_paths_stay_in_the_open_project` — 실제 앱 엔진에 붙어 탈출 시도 27가지(`..`·절대·드라이브·UNC·역슬래시·URI·`@이름`·`.folio/`·`:`·예약 이름·끝 점·빈 이름·없는 폴더·프로젝트 밖 그림·병합 문서 이름의 `/`) 거절, 원본 IDML·있는 `.folio`에 쓰기 거절(해시 그대로), 화면의 `exportIdml`(이 컴퓨터 경로)과 붙은 연결의 `exportIdml`(프로젝트 경로)이 같은 스프레드·스토리·스타일과 같은 보고서, 사람이 화면으로 내보낸 파일만 다시 내보내기 허용, PDF 내보내기, 그림 놓기, 사본→열기(저장 안 한 변경이 있으면 거절)→편집→저장→다시 열기에 편집이 남음, 설정을 끄면 쓰기 5종 거절, 프로젝트 안 정션이 있으면 경로 도구 전부 거절(지우면 다시 됨).
- 화면 검사 `next/oracle/mcp-attach-paths-ui-check.py`: 숨긴 격리 앱(두 설정을 켬)이 `basic.folio`를 열고, 붙은 클라이언트가 `second.folio` 열기 → 사각형 → 저장 → `second-export.idml` 내보내기 → 원본 덮어쓰기·`../` 거절 → 다시 열기. 화면 영수증: 보이는 문서가 `second.folio`로 바뀜, "LLM 이 문서를 열었습니다", 사각형이 화면 개체 목록에 나타남, 저장 뒤 변경 표시 꺼짐과 "LLM 이 저장했습니다", "LLM 이 파일을 썼습니다: second-export.idml", 다시 연 문서에 사각형. 끝난 뒤 원본 해시 그대로, 저장한 `.folio`와 내보낸 IDML의 스프레드에 사각형 id. 표준입출력 클라이언트(기본)·공식 SDK 2.4.0(`--sdk`, `artifacts/mcp-c4-20261023/tools/node`)·Claude Code(`--claude`, haiku 1회: 도구 9번, 10턴, 11초, 0.024달러) 모두 PASS. 영수증은 `artifacts/mcp-c4-20261023/results/`.
- SDK가 찾은 것: 등록표의 `pageItems` 출력 스키마가 `items`를 배열로 적어 SDK가 답을 거절했다(실제는 개체 `{items, guides, …}`). 등록표(`registered_queries.catalog.tsv`)를 고치고 생성 파일을 다시 만들었다.

### 가져오기·스타일·패키지 시험 (mcp-c5, 2026-11-20)

- 엔진 시험 `cargo test --release --target-dir target -p folio-engine --test mcp --test mcp_attach`: `attached_boundary_checks_the_requested_names_only`(트리 다른 곳의 정션은 통과, 정션을 지나는 경로 거절, 검사 뒤 폴더를 정션으로 바꾸면 다시 검사가 거절, 루트 자체가 링크면 거절), `attached_path_policy`(새 도구의 인수·쓰기 설정·거절 인수 `project`·`roots`·`newDocument`·`protectedRoots`·`export`·`parts`·`root`·`replace`, 책·라이브러리의 다른 동작), `attached_import_styles_package_book_and_library_stay_in_the_project`(가져오기 탈출 9가지·`project` 인수 거절·계획→가져오기·있는 문서에 다시 가져오기 거절·프로젝트 밖 그림은 들이지 않음, 스타일 탈출 4가지·`parts` 거절·불러온 스타일이 문서에, 패키지 탈출 6가지·새 폴더로만, 책·라이브러리 만들기·열기·탈출·다른 동작 거절, 설정을 끄면 쓰기 4종 거절, 원본 해시 그대로). 기존 탈출 27가지와 정션 시험도 그대로(정션 시험은 예전에 `mklink`에 `/` 섞인 경로를 줘 정션이 안 만들어져 건너뛰고 있었다; 고쳐서 늘 돈다). 측정 `cargo test … --test mcp -- --ignored path_check_speed --nocapture`(결과 `artifacts/mcp-c5-20261023/results/path-speed.json`).
- 화면 검사 `next/oracle/mcp-attach-import-ui-check.py`: 숨긴 격리 앱(두 설정을 켬)이 `basic.folio`를 열고, 붙은 클라이언트가 `originals/basic.idml` 가져오기 → `imported.folio` 열기 → `styles.idml`의 스타일 보기·불러오기 → 저장 → 새 폴더 `pkg`로 패키지 → 밖에서 가져오기·밖으로 패키지·밖의 스타일 거절. 화면 영수증: 상태 줄에 가져온 문서 이름, 보이는 문서가 `imported.folio`, 편집 ▸ 실행 취소 "LLM: loadStyles", 저장 뒤 변경 표시 꺼짐, "LLM 이 파일을 썼습니다: pkg". 끝난 뒤 원본 해시 그대로, 저장한 `imported.folio`에 불러온 스타일 `Grep 0`, `pkg`에 파일. 표준입출력 클라이언트(기본)·공식 SDK 2.4.0(`--sdk`, `artifacts/mcp-c5-20261023/tools/node`: 책 만들기·열기와 라이브러리 만들기·열기까지) 모두 PASS.
- SDK가 찾은 것: 등록표의 `stylesFile` 출력 스키마가 `styles`를 개체로 적어 SDK가 답을 거절했다(실제는 배열). 등록표를 고치고 생성 파일을 다시 만들었다.

### 새 문서·책·라이브러리·새 프로젝트 시험 (mcp-c6, 2026-11-20)

- 엔진 시험 `cargo test --release --target-dir target -p folio-engine --test mcp --test mcp_attach`: `attached_new_files_books_and_libraries_stay_in_the_project`(라이브러리 저장→배치, 새 문서 대화상자(저장 안 한 변경·`project` 칸 거절), 책 추가→동기화→PDF 내보내기와 탈출, 새 프로젝트: 설정 없으면 `noNewProjectFolder`, 사람이 설정한 폴더 안에만, 사람이 밖 폴더로 채운 대화상자는 적용 못 함, 환경 설정으로 `folio.mcp.*` 못 바꿈, 쓰기 설정 끄면 거절, 원본 해시 그대로), `attached_path_policy`(새 책·라이브러리 동작의 쓰기 설정과 남은 동작 거절, `folio.mcp.*` 칸 거절). 독립 서버: `links_cannot_escape_or_hide_in_implicit_cache_paths`(다른 곳 정션은 통과, 지나는 경로·`.folio` 정션·`assets/<문서>` 정션 거절). 측정 `… --test mcp -- --ignored project_server_speed --nocapture`(결과 `artifacts/mcp-c6-20261023/results/project-server-speed-*.json`).
- 화면 검사 `next/oracle/mcp-attach-books-ui-check.py [--sdk]`: 숨긴 격리 앱(LLM 연결·파일 쓰기·새 프로젝트 폴더를 켬)이 `basic.folio`를 보여 주고, 붙은 클라이언트가 새 문서 대화상자로 2쪽 문서를 만들고(화면이 연다) → 라이브러리에 사각형 저장 → 배치 → 저장 → 책 만들기·추가·동기화·`out/issue.pdf` 내보내기 → 새 프로젝트 `Mag`(화면이 연다). 밖의 책 문서·라이브러리·PDF와 새 문서의 `project` 칸은 거절. 화면 영수증: 새 문서가 보임, 편집 ▸ 실행 취소 "LLM: placeLibraryAsset", 저장 뒤 변경 표시 꺼짐, "LLM 이 파일을 썼습니다: issue.pdf", 화면의 프로젝트가 `…/projects/Mag`. 표준입출력 클라이언트(기본)·공식 SDK 2.4.0(`--sdk`, `artifacts/mcp-c6-20261023/tools/node`) 모두 PASS(`artifacts/mcp-c6-20261023/results/books-*-receipt.json`).

## 실행

`next/engine`에서 빌드한다. 새 외부 의존성은 없다.

```sh
cargo build --release --target-dir target-edit -p folio-engine --bin folio-mcp
target-edit/release/folio-mcp PROJECT_FOLDER
```

Windows 바이너리 이름에는 `.exe`가 붙는다. `PROJECT_FOLDER`는 **기존 `folio.project`가 있는 폴더**로 바꾼다. 시작 인수는 그 폴더 하나뿐이다. 실행 파일을 PATH에 둔 뒤 Claude Code에 연결하는 예:

```sh
claude mcp add --transport stdio folio -- folio-mcp PROJECT_FOLDER
```

공백이 있는 폴더는 셸에 맞게 따옴표로 감싼다. 이 문서의 명령을 사용자가 실행할 때만 Claude Code 설정이 바뀐다. [Claude Code MCP 설정 설명](https://code.claude.com/docs/en/mcp).

## 공개 도구

`commands.generated.json`의 명령 등록표에서 설명·입력·출력 스키마를 가져온다. 별도의 허용 목록으로 공개 범위를 제한한다. 엔진에 명령이 추가되어도 MCP에 자동 공개하지 않는다.

| 도구 | 동작 |
| --- | --- |
| `open` | 프로젝트 상대 경로의 `.idml` 또는 `.folio` 열기. 편집하려면 `edit: true`. 답은 작은 요약(`summary`) |
| `documentInfo` | 현재 패키지를 파싱한 문서 모델. 기본은 요약, 전체는 `detail: "full"` |
| `coverage` | 엔진 명령·질의 전부에 대해 "공개 / 비공개 + 이유"를 알려 준다(아래). 문서를 열지 않는다 |
| `edit.createByDrag` | 페이지·레이어 id와 좌표로 사각형·타원·다각형·선 또는 도형 프레임 생성 |
| `editState` | 편집 이력·변경 상태·리비전 조회 |
| `undo`, `redo` | 현재 세션의 실행 취소·다시 실행 |
| `saveCopy` | 변경 내용을 **새 `.folio` 파일**로 저장 |
| `exportIdml` | 열린 `.folio`를 **새 `.idml` 파일**로 내보내기 |

**요약과 전체.** 작은 페이지 하나의 전체 문서 모델도 JSON 150 KB(토큰 4만 안팎)라서, 실제 클라이언트가 받을 수 없다(Claude Code 의 MCP 도구 출력 기본 한도는 2만 5천 토큰이다). 그래서 `open`·`documentInfo`는 기본으로 `summary`(레이어·스프레드·페이지·개체의 id와 종류, 페이지 상자·여백, 개수; 스프레드당 개체 100개까지)를 돌려주고, 모델 전체가 필요할 때만 `detail: "full"`을 쓴다(그러면 `document`/`info` 키에 담긴다). `edit.createByDrag`의 `page`·`layer`에는 이 요약의 **id**를 넣는다(이름이 아니다: 실연결에서 모델이 레이어 이름 `Base`를 넣었다가 `layer not found`를 받고 id `ub8`로 고쳐 다시 불렀다).

문서 하나를 메모리에 연다. 다른 문서를 `open`하면 저장하지 않은 변경과 이력은 버려진다. `saveCopy`는 현재 문서를 바꾸거나 저장 기준을 초기화하지 않는다. `dirty`는 열고 나서 편집 리비전이 바뀌었는지를 나타내므로 실행 취소 후에도 참일 수 있다. 저장한 사본에서 계속 작업하려면 그 파일을 다시 연다.

IDML은 `saveCopy`로 가져와 `.folio`를 만든 뒤, 그 파일을 열어 `exportIdml`을 호출한다. 원본의 모르는 XML과 패키지 항목은 기존 저장 라이브러리로 보존한다. 외부 링크와 시스템 글꼴은 가져오지 않는다. 프로젝트에 없는 자료는 누락 상태와 원래 URI 힌트를 유지한다. `.folio` 안의 프로젝트 참조는 기존 해석·내보내기 라이브러리를 사용한다.

## 요청과 결과

[MCP 2025-06-18](https://modelcontextprotocol.io/specification/2025-06-18)을 지원한다. JSON-RPC 2.0 메시지 하나를 UTF-8 한 줄로 보낸다. stdout에는 응답 JSON만, 시작 오류는 stderr에 쓴다. 클라이언트가 다른 프로토콜 버전을 제안하면 서버가 지원 버전을 반환하므로 클라이언트는 호환 여부를 확인해야 한다.

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"example","version":"1"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"open","arguments":{"path":"source.idml","edit":true}}}
```

도구 인수는 `tools/list`의 스키마를 따른다. 개체 생성의 `rect`는 스프레드 좌표의 `[x0, y0, x1, y1]`(pt)이다. `page`와 `layer`는 열린 문서에서 얻은 id를 넣는다. 결과는 `structuredContent`와 같은 JSON을 담은 텍스트 `content`로 반환한다. 만든 개체 id는 `created`에 있다.

잘못된 JSON은 `-32700`, 잘못된 요청은 `-32600`, 없는 메서드는 `-32601`, 없는 도구·잘못된 인수는 `-32602`, 초기화 전 도구 사용은 `-32002`다. 권한 거부나 엔진 오류는 도구 결과의 `isError: true`와 `{ "ok": false, "code": "refused", "error": "…" }`로 돌려준다. 알림에는 응답하지 않는다. 요청 한 줄은 줄바꿈을 포함해 최대 1 MiB이며 초과 시 오류 응답 후 전송을 닫는다.

## 권한 경계

- 시작할 때 프로젝트 루트를 실제 경로로 정규화한다. 도구의 모든 경로는 그 루트 기준 상대 경로다. `..`, 절대 경로, UNC, 역슬래시, 드라이브 경로, ADS는 거부한다. 경로 해석 후에도 루트 안인지 확인한다.
- 심볼릭 링크·Windows 정션·재분석 지점은 프로젝트 안의 대상을 가리켜도, 경로가 지나가면 거부한다. 프로젝트 루트 자체가 링크여도 거부한다. 시작 시와 도구 호출 전에는 요청한 경로의 이름(루트→대상)과 Folio가 이름 없이 쓰는 폴더(`.folio`·`.folio/cache`·`.folio/tmp`·`.folio/locks`, 프로젝트 설정의 자산·글꼴 폴더)와 열린 `.folio`의 참조 경로만 검사한다. IDML을 `.folio`로 저장할 때는 그 가져오기가 쓸 `assets/<문서>/`·`previews`·`embedded`도 본다. (mcp-c6, 2026-11-20: 예전에는 전체 트리(100,000 항목까지)를 훑어 링크가 하나라도 있으면 거부했다. 파일 5만 개 프로젝트에서 시작 1394 ms → 0.40 ms, 도구 호출 하나 3036 ms → 0.66 ms. 트리 다른 곳의 링크는 이제 막지 않는다: 프로젝트의 자체 훑기는 링크를 따라가지 않는다.)
- 기존 파일을 저장 대상으로 삼을 수 없다. 원본 INDD/IDML을 덮어쓰지 않는다. 새 파일의 부모 폴더는 미리 있어야 한다. 열린 원본이 외부에서 바뀌면 다시 열기 전까지 작업을 거부한다.
- 파일 삭제, 프린터 전송, 임의 엔진 명령, 프로젝트 밖 내보내기 도구를 공개하지 않는다. 클라이언트의 roots나 도구 인수로 경계를 넓힐 수 없다.
- 참조 검색·캐시·임시 파일은 프로젝트 안에만 둔다. 외부 경로를 포함한 원본 XML/URI는 보존 대상 텍스트이며 외부 파일을 읽는 권한이 아니다.
- 이는 도구 호출의 경로 제한이다. OS 샌드박스는 아니며, 검사와 사용 사이에 다른 프로세스가 폴더를 바꾸는 경쟁까지 막지는 않는다. 서버가 작업하는 동안 프로젝트의 링크·폴더 구조를 외부에서 바꾸지 않는다.

## 공개하지 않은 것과 이유 (`coverage`)

등록표의 명령·질의마다 연결 모드의 결정을 이름으로 기록한다. `coverage`의 `attached.registryOpen`은 권한 스위치가 필요한 도구까지 포함한다. `attached.withheld`에는 영구 보류 이유별 이름과 수가 있다. `coverage`에 `name`을 주면 해당 도구의 이유(`attachedWhy`)와 제스처의 대체 명령(`valueTool`)을 볼 수 있다. 프로젝트 단독 서버의 결정은 최상위 `exposed`·`withheld`에 있다.

| 연결 모드 영구 보류 이유 | 예 |
| --- | --- |
| `secret` | 비밀번호와 개인 설정을 다루는 `userSettings` |
| `printer` | 장치 목록과 인쇄 대기열을 다루는 `listPrinters` |
| `engine-process` | 공유 엔진을 종료하는 `quit` |
| `external-data` | 외부 DB를 직접 조회하는 `dbPreview` |
| `gesture-only` | `edit.adjustGap`은 같은 좌표로 `edit.adjustGapKeys`를 호출한다. `resize`와 `move_items`에 모두 `false`를 준다. |

`every_registry_name_is_reviewed`는 새 질의와 새 편집 명령에 명시적인 결정이 있어야 통과한다. 공개 목록과 보류 목록의 중복, 사유 누락, 사용할 수 없는 제스처 대체 명령도 검사한다.

**비밀.** 어떤 도구든 인수의 키(깊이 상관없이)에 `password`·`passphrase`·`secret`·`credential`·`apikey`·`accesstoken`·`bearer`가 들어 있으면 JSON-RPC `-32602`("Secrets are not accepted over MCP")로 거절한다. 응답은 값을 되풀이하지 않고 서버는 로그를 쓰지 않는다(시험 `secrets_do_not_reach_output_logs_or_files`가 표준출력·표준오류·사용자 데이터·프로젝트 파일에 값이 없음을 본다). 공개 도구의 입력·출력 스키마에 그런 이름이 생기면 서버가 시작하지 못한다. 데이터베이스 비밀번호는 사람이 앱에서 풀거나 OS 자격 증명 저장소에 저장한다.

## 클라이언트 연결 설정 (사용자가 붙여 넣을 조각)

먼저 `folio-mcp` 실행 파일과 **프로젝트 폴더**(`folio.project`가 있는 곳)의 절대 경로를 정한다. JSON 안의 윈도우 경로는 `/`로 쓰거나 역슬래시를 두 개씩(`\\`) 겹쳐 쓴다. 이 문서는 사용자 설정을 바꾸지 않는다. 아래를 사용자가 직접 붙여 넣는다.

**Claude Code** (실연결 확인, 2.1.329):

```sh
claude mcp add --transport stdio folio -- C:/path/to/folio-mcp.exe C:/path/to/MyProject
```

또는 프로젝트의 `.mcp.json`:

```json
{ "mcpServers": { "folio": { "command": "C:/path/to/folio-mcp.exe", "args": ["C:/path/to/MyProject"] } } }
```

Claude Code는 도구 이름의 `.`를 `_`로 바꾼다: `edit.createByDrag` → `mcp__folio__edit_createByDrag`. 허용 목록에는 서버 전체 `mcp__folio`를 쓰면 된다. 한 번만 쓰려면 `claude -p "…" --mcp-config <파일> --strict-mcp-config --tools "" --allowedTools mcp__folio`(`tools/run-claude.py`가 이 형태).

**Claude Desktop** (실행하지 않음, 형식만): 설정 파일 `claude_desktop_config.json`(윈도우 `%APPDATA%\Claude\`, 맥 `~/Library/Application Support/Claude/`)에 같은 `mcpServers` 조각을 넣고 앱을 다시 시작한다.

```json
{ "mcpServers": { "folio": { "command": "C:/path/to/folio-mcp.exe", "args": ["C:/path/to/MyProject"] } } }
```

**Codex** (실행하지 않음 — 사용량 예비선이라 시험하지 않았다, 형식만): `~/.codex/config.toml`에

```toml
[mcp_servers.folio]
command = "C:/path/to/folio-mcp.exe"
args = ["C:/path/to/MyProject"]
```

**MCP Inspector** (실연결 확인, 2.16.0): 서버를 명령으로 주고 `--cli`로 부른다. 호출 하나가 연결 하나라서 한 번에 한 메서드만 된다(열기→만들기→저장 같은 상태가 있는 흐름은 못 한다).

```sh
npx @modelcontextprotocol/inspector@2.16.0 --cli C:/path/to/folio-mcp.exe C:/path/to/MyProject --method tools/list
npx @modelcontextprotocol/inspector@2.16.0 --cli C:/path/to/folio-mcp.exe C:/path/to/MyProject --method tools/call --tool-name open --tool-arg path=source.idml --tool-arg edit=true
```

서버가 쓰는 앱 데이터(`FOLIO_USER_DATA` 등)를 따로 두려면 `-e FOLIO_USER_DATA=…`를 앞에 붙인다.

## 실제 클라이언트 연결 기록 (mcp-c2, 2026-11-19)

같은 프로젝트 사본(`next/oracle/fixtures/basic.idml`)으로 세 클라이언트를 돌렸다. 기록·다시 돌리는 스크립트는 `artifacts/mcp-c2-20261023/`(`tools/`, `results/`).

| 클라이언트 | 한 일 | 결과 |
| --- | --- | --- |
| MCP Inspector 2.16.0 `--cli` (`tools/run-inspector.py`) | 초기화·도구 목록(9개), `coverage`, `open`, 프로젝트 밖 경로·열지 않은 저장·비밀번호가 든 호출의 거절 | 9/9 기대대로. 비밀번호 값은 출력 어디에도 없다. 프로젝트 폴더에 `folio.project`·`source.idml`만 남고 원본 해시 같음. (Inspector는 도구 목록에 없는 이름을 서버로 보내기 전에 막는다. 그래서 `dbUnlockConnection`은 클라이언트가 먼저 거절하고, 서버의 비밀 거절은 `open`에 `password`를 붙인 호출로 확인한다.) |
| 공식 TypeScript SDK 클라이언트 `@modelcontextprotocol/client` 2.4.0 (Inspector가 쓰는 같은 패키지, `tools/node/flow.mjs`) | 한 연결에서 열기→사각형 만들기→`.folio` 저장→다시 열기→IDML 내보내기→다시 열기, 거절 4개, `coverage` | 전부 통과. 만든 개체 id가 `.folio`와 내보낸 IDML에 있다. 원본 해시 불변. SDK 2.x는 스펙 2026-07-28을 쓰지만 서버의 2025-06-18 응답을 받아들였다 |
| Claude Code CLI 2.1.329, 모델 haiku (`tools/run-claude.py`) | 같은 흐름을 지시문 하나로(`--strict-mcp-config`, 내장 도구 끔, 세션 저장 안 함) | 8턴, 9초, 약 0.013달러. 개체 id `u16d`가 내보낸 IDML의 스프레드에 있다. 모델이 처음에 레이어 이름을 넣어 거절을 받고 id로 고쳤다 |

실연결에서 나온 고침: `open`의 전체 모델이 150 KB를 넘어 LLM 클라이언트가 못 받았다 → 기본 응답을 요약으로 바꿨다(위). 시험·실연결 모두 사용자 MCP 설정·`~/.claude`·`~/.codex`·`~/.mcp-inspector`를 바꾸지 않았다. 만든 파일의 해시는 `results/*.json`에 있다(`.folio`·`.idml`에는 만든 시각과 id가 들어가 실행마다 해시가 다르다).

## 한계

- **프로젝트 폴더 모드는 앱과 다른 프로세스다.** 앱 화면의 선택·대화상자·열린 문서·잠금 해제한 데이터베이스 비밀번호를 공유하지 않는다. 앱이 열어 둔 문서를 같이 고치려면 `--attach`를 쓴다(위).
- 프로젝트 폴더 모드의 도구는 9개(공개 8 + `coverage`)다. 앱에 연결한 뒤 `tools/list`로 전체 도구 목록을 보고, `coverage`로 도구별 허용 범위와 필요한 설정을 확인한다.
- 붙는 것은 서버가 시작할 때 한 번 정한다. 앱을 다시 켜면(토큰이 바뀐다) MCP 서버도 다시 시작해야 한다. 한 컴퓨터에서 앱을 둘 켜면 나중에 켠 앱의 연결 파일이 이긴다.
- 전송은 표준입출력뿐이다. 클라이언트마다 연결 설정을 사용자가 직접 한다. Claude Desktop·Codex는 형식만 적었고 실행해 보지 않았다.
- `open`의 요약은 스프레드당 개체 100개까지 보여 준다(`itemsTruncated`). 더 보려면 `detail: "full"`을 쓴다(매우 크다).
- Inspector 2.16.0은 Node `^22.23.4 || ^24.22.1 || >=26.12.1`을 요구한다. 이 PC의 Node 23.10에서는 경고만 나오고 동작했다.

## 프로젝트 폴더 모드에서 아직 없는 것

INDD 직접 가져오기, 새 프로젝트 만들기, 텍스트 편집·조판, 렌더링·PDF·인쇄, 외부 자료 가져오기, 모든 엔진 명령 공개는 지원하지 않는다. 텍스트 프레임 생성도 공개하지 않는다. HTTP 전송·인증·리소스·프롬프트·샘플링·진행률·실행 중 취소·페이지 나눔은 없다. 이 도구 목록에는 완성된 범위만 들어 있다.

## 검증

```sh
cargo test --release --target-dir target-edit -p folio-engine --test mcp
python -I ../tools/check-portability.py
```

시험은 저장소 `artifacts/mcp-c2-20261023/` 아래에 시험 프로젝트를 만들고, 실행하는 MCP 프로세스의 Folio 사용자 데이터(`FOLIO_USER_DATA`·`FOLIO_CACHE_DIR`·`FOLIO_COLOR_CACHE`)도 그 아래로 격리한다(`LOCALAPPDATA`·`APPDATA`는 바꾸지 않는다: 사용자 글꼴과 색 프로파일이 그 아래에 있다). 프로토콜 오류, 경로·링크 우회 거부, 원본 보호, 등록표 검토 잠금, 비밀 거절, 요약 응답, 실제 표준입출력의 열기→생성→저장→재열기→내보내기를 검사한다(15개). 새 InDesign 알고리즘을 구현하지 않으며, 기존 `basic.idml` 오라클 픽스처를 사용한다.

실제 클라이언트로 다시 돌리려면(`artifacts/mcp-c2-20261023/`에서; 새 프로젝트 사본을 만들고 결과를 `results/`에 쓴다):

```sh
python -X utf8 -B -I tools/run-inspector.py <folio-mcp 실행 파일>
(cd tools/node && npm install && node flow.mjs <folio-mcp 실행 파일> <새 프로젝트 폴더> <결과.json> <사용자 데이터 폴더>)
python -X utf8 -B -I tools/run-claude.py <folio-mcp 실행 파일> <저장소 밖 작업 폴더>
```

(새 프로젝트 폴더는 `python -X utf8 -I tools/make-project.py <새 폴더>`로 만든다. `run-claude.py`는 사용량을 쓴다: 작은 모델 한 번, 약 0.01달러.)
