// Tool help (uix-e1, 2026-11-21): what every tool of the Tools panel, the small buttons around the fill and stroke, the screen modes and the
// control bar's reference point do, in one table. The words follow Folio's own code (what a click, a drag and Shift / Alt / Ctrl do there),
// checked against ui/docview.tsx, ui/drawing.tsx, ui/pathtools.tsx, ui/transformtools.ts, ui/gradienttools.ts, ui/eyedropper.tsx,
// ui/conveyor.ts, features/tools/coordinates.ts and app/useAppEvents.tsx (artifacts/uix-e1-20261023/STATUS.ko.md).
// Korean is the source (tr), English is in i18n/messages.en.ts; a tool's name and key are its row in ui/tools.tsx. The screen shows it in the
// tools view's help section and the instant card (ui/toolhelpview.tsx); next/shared/tool-help.json is written from it for readers without the
// screen (node next/app/scripts/gen-tool-help.cjs). Plain TypeScript (no JSX): the generator loads it.
import { tr } from '../i18n';
import type { ToolId } from './tools';

export type ControlHelpId =
  | 'fill' | 'stroke' | 'swapFillStroke' | 'defaultFillStroke' | 'formatContainer' | 'formatText' | 'applyColor' | 'applyGradient' | 'applyNone'
  | 'screenMode' | 'screenNormal' | 'screenPreview' | 'screenBleed' | 'screenSlug' | 'screenPresentation' | 'refPoint';
export type HelpId = ToolId | ControlHelpId;

export type HelpEntry = {
  /** The name of a control (a tool's is its label in ui/tools.tsx). */
  name?: () => string;
  /** Fixed keys of a toolbox control (app/useAppEvents.tsx). */
  keys?: string[];
  /** Commands whose keys (the person's keymap, Edit ▸ Keyboard Shortcuts) are shown, the first key of each. */
  command?: string[];
  /** One line: what it is for. */
  summary: () => string;
  /** Two to four sentences: what it does, how to use it, what Shift / Alt / Ctrl do, the panel that goes with it. */
  detail: () => string;
};

const TOOLS: Record<ToolId, HelpEntry> = {
  selection: {
    summary: () => tr('개체를 골라 옮기고 크기를 바꿉니다.'),
    detail: () => tr('개체를 누르면 고르고, 끌면 옮깁니다. 빈 곳에서 끌면 닿는 개체를 모두 고르고, 손잡이를 끌면 크기가, 모서리 바로 바깥을 끌면 각도가 바뀝니다. Shift 는 여러 개 고르기·45° 단위 이동·비율 유지이고, Alt 는 복사본 옮기기·가운데 기준 크기 바꾸기입니다. 겹친 곳을 Ctrl 을 누르고 누르면 고른 개체 바로 아래 개체를 고릅니다(다시 누르면 더 아래). 텍스트 프레임을 두 번 누르면 글을 고치고, 그림 프레임을 두 번 누르면 안의 그림을 고릅니다. Alt 를 누르고 끄는 중에 화살표 키를 누르면 끈 넓이에 복사본을 격자로 늘어놓습니다(←→ 열, ↑↓ 행, Ctrl+화살표 칸 사이).'),
  },
  direct: {
    summary: () => tr('그룹이나 프레임 안의 개체, 패스의 고정점을 바로 고릅니다.'),
    detail: () => tr('누르면 가장 안쪽 개체나 고정점을 고르고, 고정점·방향선·선분을 끌어 모양을 바꿉니다(곧은 선분은 두 점이 함께 움직입니다). Shift 를 누르고 누르면 점을 더하거나 빼고, 끄는 중에는 45° 단위로 맞춥니다. Alt 를 누르고 방향선을 끌면 반대쪽 방향선은 그대로 둡니다. Delete 는 고른 점을 지우고, 화살표 키는 점을 옮깁니다(Shift 는 10배).'),
  },
  page: {
    summary: () => tr('페이지를 골라 크기를 바꾸거나 옮깁니다.'),
    detail: () => tr('페이지를 누르면 그 페이지를 고르고, 조절 막대에서 크기와 사전 설정을 바꿉니다. 크기를 바꿀 때는 기준점이 고정됩니다. 끌면 페이지를 옮기고, 개체도 함께 옮길지는 조절 막대의 개체 함께 이동을 따릅니다.'),
  },
  gap: {
    summary: () => tr('개체 사이의 빈 간격을 끌어 옮깁니다.'),
    detail: () => tr('두 개체 사이 빈 곳을 끌면 간격 폭은 그대로 두고 자리만 옮기며, 양옆 개체의 크기가 함께 바뀝니다(개체마다 8pt 아래로는 줄지 않습니다). Shift 를 누르면 가장 가까운 개체만 바뀝니다. 끄는 중에 Esc 를 누르면 취소합니다. Ctrl 을 누르면 간격 폭이 바뀌고(양옆 개체 크기도 바뀜), Alt 는 간격과 양옆 개체를 함께 옮기며, Ctrl+Alt 는 개체를 옮겨 간격 폭을 바꿉니다.'),
  },
  collector: {
    summary: () => tr('개체를 눌러 컨베이어에 담습니다.'),
    detail: () => tr('개체(그룹이면 그룹 전체)를 누르면 화면 아래 컨베이어에 담습니다. 담은 것은 내용 배치 도구로 다른 곳이나 다른 문서에 놓습니다. B 를 다시 누르면 내용 배치 도구로 바뀝니다.'),
  },
  placer: {
    summary: () => tr('컨베이어에 담은 개체를 눌러 놓습니다.'),
    detail: () => tr('누른 곳에 컨베이어의 지금 항목을 놓습니다. 화살표 키로 앞뒤 항목을 고르고, 놓은 뒤 컨베이어에서 뺄지, 링크를 만들지, 스타일을 대응할지는 컨베이어 막대에서 정합니다. B 를 누르면 내용 수집 도구로 돌아갑니다.'),
  },
  type: {
    summary: () => tr('텍스트 프레임을 만들고 글을 입력합니다.'),
    detail: () => tr('텍스트 프레임 안을 누르면 커서를 두고, 빈 곳에서 끌면 새 텍스트 프레임을 만듭니다(한 번 누르면 크기를 숫자로 넣습니다). 끄는 중에 화살표 키를 누르면 프레임을 격자로 여러 개 만듭니다(←→ 열, ↑↓ 행). Shift 를 누르고 누르면 선택을 늘리고, 두 번·세 번 누르면 낱말·줄을 고릅니다. Esc 는 글 고치기를 마치고, 프레임을 고른 채 선택 도구로 돌아갑니다. 환경 설정 ▸ 문자에서 글 끌어 놓기를 켜면 고른 글을 끌어 옮깁니다(Ctrl 이나 Alt 는 복사).'),
  },
  typePath: {
    summary: () => tr('선이나 도형의 패스를 따라 글을 씁니다.'),
    detail: () => tr('패스를 누르면 그 패스 위에 글을 넣을 자리를 만듭니다. 열린 패스는 처음부터 끝까지, 닫힌 패스는 누른 곳부터 글이 놓입니다. 이미 패스 위에 있는 글을 누르면 커서를 둡니다.'),
  },
  typeVertical: {
    summary: () => tr('세로로 쓰는 텍스트 프레임을 만들고 글을 입력합니다.'),
    detail: () => tr('빈 곳에서 끌면 세로 텍스트 프레임을 만듭니다. 세로 글은 줄이 오른쪽에서 왼쪽으로 쌓이고 글자가 위에서 아래로 갑니다. 한글·한자는 똑바로 서고 로마자·숫자는 90° 돕니다. 글 안에서 ↑↓ 는 글자를, ←→ 는 줄을 옮깁니다. 이미 있는 글을 누르면 커서를 둡니다.'),
  },
  line: {
    summary: () => tr('곧은 선을 그립니다.'),
    detail: () => tr('끌어서 선을 그리고, 끌지 않고 한 번 누르면 크기를 숫자로 넣습니다. Shift 는 45° 단위로 맞추고, Alt 는 누른 곳을 가운데로 그립니다. 선의 두께와 모양은 획 패널과 조절 막대에서 바꿉니다.'),
  },
  pen: {
    summary: () => tr('고정점을 하나씩 찍어 패스를 그립니다.'),
    detail: () => tr('누르면 모퉁이 점, 끌면 방향선이 있는 매끄러운 점이 생깁니다. 끄는 중 Alt 는 방향선을 끊어 나가는 쪽만 움직이고, Space 를 누르고 있으면 점을 옮기며, Shift 는 점과 방향선을 45° 단위로 맞춥니다. 첫 점을 누르면(끌면 방향선을 만들며) 닫히고, Enter·Esc·두 번 누르기로 열린 채 끝내며, Backspace 는 마지막 점을 지우고 마지막 점을 다시 누르면 나가는 방향선을 거둡니다. 고른 패스 위에서는 선분을 누르면 점을 더하고 점을 누르면 지우며(Shift 를 누르면 하지 않음), Alt 로 점을 누르면 점을 변환합니다. 열린 패스의 끝점을 누르면 이어 그리거나 합치고, Ctrl 을 누르고 있으면 잠시 직접 선택 도구가 됩니다. 그리는 중 Ctrl+Z 는 마지막 점을 되돌립니다. 고급 안내선이 켜져 있으면 점이 다른 패스의 고정점과 개체 가장자리·가운데에 붙습니다.'),
  },
  penAdd: {
    summary: () => tr('패스 위에 고정점을 더합니다.'),
    detail: () => tr('고른 패스나 포인터 아래 패스의 선분을 누르면 그 자리에 고정점을 더합니다. 더한 점은 직접 선택 도구로 옮깁니다.'),
  },
  penDelete: {
    summary: () => tr('패스의 고정점을 지웁니다.'),
    detail: () => tr('고정점을 누르면 그 점을 지우고 양옆을 잇습니다. 패스에는 점이 두 개 이상 남습니다.'),
  },
  convert: {
    summary: () => tr('고정점을 모퉁이 점과 매끄러운 점 사이에서 바꿉니다.'),
    detail: () => tr('고정점을 누르면 방향선을 없애 모퉁이 점으로, 점에서 끌어내면 양쪽 방향선이 같은 매끄러운 점으로 바꿉니다. 바꾼 점의 방향선 하나를 끌면 그쪽만 움직여 방향선이 끊깁니다. Shift 를 누르고 끌면 45° 단위로 맞춥니다.'),
  },
  pencil: {
    summary: () => tr('손으로 그리듯 끌어서 패스를 그립니다.'),
    detail: () => tr('끌면 지나간 자리를 따라 패스가 생깁니다. 놓을 때 Alt 를 누르고 있으면 패스를 닫고, 고른 열린 패스 위에서 시작하면 그 패스를 이어 그리거나 고쳐 그립니다. 도구를 두 번 누르면 연필 도구 환경 설정이 열리고, 충실도와 부드러움(0~100%, 높을수록 점이 적고 매끄러움)을 정합니다.'),
  },
  smooth: {
    summary: () => tr('고른 패스의 울퉁불퉁한 곳을 매끄럽게 합니다.'),
    detail: () => tr('고른 패스를 따라 끌면 그 부분을 매끄럽게 다시 그립니다. 닫힌 패스는 닫힌 채로, 복합 패스는 끈 곳에 가까운 하위 패스를 다듬습니다. 패스를 고르지 않았으면 안내만 보입니다.'),
  },
  erase: {
    summary: () => tr('고른 패스의 일부를 지웁니다.'),
    detail: () => tr('고른 패스를 따라 끌면 지나간 부분을 지웁니다. 닫힌 패스는 지운 곳에서 열려 열린 패스 하나가 되고(지운 곳 끝에서 시작), 복합 패스는 조각이 같은 개체에 남습니다. 패스를 고르지 않았으면 안내만 보입니다.'),
  },
  rectFrame: {
    summary: () => tr('그림이나 글을 담을 사각형 프레임을 그립니다.'),
    detail: () => tr('끌어서 그리고, 한 번 누르면 크기를 숫자로 넣습니다. Shift 는 정사각형, Alt 는 누른 곳을 가운데로 그립니다. 끄는 중에 ←→ 는 열, ↑↓ 는 행을 더하거나 빼서 격자로 그리고, Ctrl 과 화살표는 칸 사이를 넓히거나 좁힙니다. Space 를 누른 채 움직이면 그리던 것이 따라 옮겨집니다. 빈 프레임에는 나중에 그림을 가져와 넣습니다.'),
  },
  ellipseFrame: {
    summary: () => tr('그림이나 글을 담을 타원 프레임을 그립니다.'),
    detail: () => tr('끌어서 그리고, 한 번 누르면 크기를 숫자로 넣습니다. Shift 는 원, Alt 는 누른 곳을 가운데로 그립니다. 끄는 중에 ←→ 는 열, ↑↓ 는 행을 더하거나 빼서 격자로 그리고, Ctrl 과 화살표는 칸 사이를 넓히거나 좁힙니다. Space 를 누른 채 움직이면 그리던 것이 따라 옮겨집니다.'),
  },
  polygonFrame: {
    summary: () => tr('그림이나 글을 담을 다각형 프레임을 그립니다.'),
    detail: () => tr('끌어서 그리고, 한 번 누르면 크기와 변의 수·별 안쪽 비율을 숫자로 넣습니다. Shift 는 비율을 지키고, Alt 는 누른 곳을 가운데로 그립니다. 끄는 중에 화살표 키는 격자로 그리고, Space 를 누른 채 ↑↓ 는 변의 수, ←→ 는 별 안쪽 비율을 바꿉니다. 도구를 두 번 누르면 다각형 설정(변의 수·별 모양 안쪽 깊이)이 열립니다.'),
  },
  rect: {
    summary: () => tr('칠과 획이 있는 사각형을 그립니다.'),
    detail: () => tr('끌어서 그리고, 한 번 누르면 크기를 숫자로 넣습니다. Shift 는 정사각형, Alt 는 누른 곳을 가운데로 그립니다. 끄는 중에 ←→ 는 열, ↑↓ 는 행을 더하거나 빼서 격자로 그리고, Ctrl 과 화살표는 칸 사이를 넓히거나 좁힙니다. Space 를 누른 채 움직이면 그리던 것이 따라 옮겨집니다. 모퉁이 모양은 선택 도구로 모퉁이의 마름모를 끌어 바꿉니다.'),
  },
  ellipse: {
    summary: () => tr('칠과 획이 있는 타원을 그립니다.'),
    detail: () => tr('끌어서 그리고, 한 번 누르면 크기를 숫자로 넣습니다. Shift 는 원, Alt 는 누른 곳을 가운데로 그립니다. 끄는 중에 ←→ 는 열, ↑↓ 는 행을 더하거나 빼서 격자로 그리고, Ctrl 과 화살표는 칸 사이를 넓히거나 좁힙니다. Space 를 누른 채 움직이면 그리던 것이 따라 옮겨집니다.'),
  },
  polygon: {
    summary: () => tr('칠과 획이 있는 다각형이나 별을 그립니다.'),
    detail: () => tr('끌어서 그리고, 한 번 누르면 크기와 변의 수·별 안쪽 비율을 숫자로 넣습니다. Shift 는 비율을 지키고, Alt 는 누른 곳을 가운데로 그립니다. 끄는 중에 화살표 키는 격자로 그리고, Space 를 누른 채 ↑↓ 는 변의 수, ←→ 는 별 안쪽 비율을 바꿉니다. 도구를 두 번 누르면 다각형 설정(변의 수·별 모양 안쪽 깊이)이 열립니다.'),
  },
  scissors: {
    summary: () => tr('패스를 누른 곳에서 자릅니다.'),
    detail: () => tr('고정점을 누르면 그 점에서, 선분을 누르면 그 자리에 점을 더해 자릅니다. 자른 조각은 직접 선택 도구로 옮깁니다.'),
  },
  freeTransform: {
    summary: () => tr('한 도구로 옮기고, 크기를 바꾸고, 돌리고, 기울입니다.'),
    detail: () => tr('상자 안을 끌면 옮기고, 손잡이를 끌면 반대쪽 손잡이를 기준으로 크기가 바뀌고, 상자 밖을 끌면 돌립니다. Ctrl 을 누르고 손잡이를 끌면 기울입니다. Shift 는 비율 유지(옮길 때는 가로·세로로만)이고, Alt 는 손잡이에서는 가운데를 기준으로 크기를 바꾸고 옮기거나 돌릴 때는 복사본을 바꿉니다. 도구를 두 번 누르면 크기 조정 창이 열립니다.'),
  },
  rotate: {
    summary: () => tr('고른 개체를 한 점을 중심으로 돌립니다.'),
    detail: () => tr('중심은 처음에 선택의 가운데이고, 한 번 누르거나 중심 가까이를 끌면 옮겨집니다. 다른 곳을 끌면 돌립니다. Shift 는 45° 단위이고, 끄는 중에 Alt 를 누르면 복사본을 돌립니다. Alt 를 누르고 한 번 누르면 그 점을 중심으로 회전 창이 열립니다. 도구를 두 번 누르면 조절 막대의 기준점을 중심으로 쓰는 회전 창이 열립니다.'),
  },
  scale: {
    summary: () => tr('고른 개체를 한 점을 기준으로 키우거나 줄입니다.'),
    detail: () => tr('기준은 처음에 선택의 가운데이고, 한 번 누르거나 기준 가까이를 끌면 옮겨집니다. 다른 곳을 끌면 크기가 바뀝니다. Shift 는 가로·세로를 같은 비율로 하고, 끄는 중에 Alt 를 누르면 복사본을 바꿉니다. Alt 를 누르고 한 번 누르면 그 점을 기준으로 크기 조정 창이 열립니다. 도구를 두 번 누르면(개체 하나일 때) 크기 조정 창이 열립니다.'),
  },
  shear: {
    summary: () => tr('고른 개체를 한 점을 기준으로 기울입니다.'),
    detail: () => tr('기준은 처음에 선택의 가운데이고, 한 번 누르거나 기준 가까이를 끌면 옮겨집니다. 다른 곳을 끌면 기울입니다. Shift 는 45° 단위이고, 끄는 중에 Alt 를 누르면 복사본을 기울입니다. Alt 를 누르고 한 번 누르면 그 점을 기준으로 기울이기 창이 열립니다. 도구를 두 번 누르면 기울이기 창이 열립니다.'),
  },
  gradient: {
    summary: () => tr('끌어서 그레이디언트의 방향과 길이를 정합니다.'),
    detail: () => tr('고른 개체 위를 끌면 시작점·각도·길이가 정해지고, 그레이디언트가 아니던 칠은 기본 그레이디언트가 됩니다. 도구 상자에서 앞에 있는 칠이나 획에 들어가고, 텍스트에 서식 적용이 켜져 있거나 글자를 골랐으면 글자에 들어갑니다. Shift 는 45° 단위, Esc 는 취소입니다. 도구를 두 번 누르면 그레이디언트 패널이 열립니다.'),
  },
  gradientFeather: {
    summary: () => tr('끌어서 개체가 한쪽으로 점점 투명해지게 합니다.'),
    detail: () => tr('고른 개체 위를 끈 방향으로 서서히 사라지는 그레이디언트 페더 효과가 생깁니다. Shift 는 45° 단위, Esc 는 취소입니다. 세부 값은 효과 창에서 바꾸고, 도구를 두 번 누르면 효과 창의 그레이디언트 페더가 열립니다.'),
  },
  note: {
    summary: () => tr('글 사이에 편집 메모를 넣습니다.'),
    detail: () => tr('텍스트 프레임의 글 사이를 누르면 그 자리에 메모를 넣고 메모 패널을 엽니다.'),
  },
  eyedropper: {
    summary: () => tr('한 개체의 칠·획·글자 서식을 집어 다른 개체에 옮깁니다.'),
    detail: () => tr('처음 누른 개체에서 서식을 집고, 그다음부터 누르는 개체에 적용합니다. Alt(또는 Shift)를 누르고 누르면 다시 집고, Esc 는 집은 것을 비웁니다. 다른 도구로 바꿔도 집은 것은 비워집니다. 도구를 두 번 누르면 무엇을 집을지 고르는 스포이트 옵션이 열립니다.'),
  },
  measure: {
    summary: () => tr('두 점 사이의 거리와 각도를 잽니다.'),
    detail: () => tr('끌어서 잴 선을 그으면 거리(D)와 가로·세로(W·H)와 각도가 정보 패널에 나옵니다. Shift 는 45° 단위로 맞춥니다. 선의 끝점에서 Alt 를 누르고 끌면 두 번째 선(각도기)이 생기고, 두 길이(D1·D2)와 사이 각도가 나옵니다. 도구를 두 번 누르면 정보 패널이 열립니다.'),
  },
  hand: {
    summary: () => tr('끌어서 화면을 움직입니다.'),
    detail: () => tr('끌면 문서가 손을 따라 움직입니다. 다른 도구를 쓰다가 Space 를 누르고 있으면 잠시 손 도구가 됩니다(글을 입력하는 중에는 빼고). 도구를 두 번 누르면 페이지를 창에 맞춥니다.'),
  },
  zoom: {
    summary: () => tr('누르거나 끌어서 화면을 확대·축소합니다.'),
    detail: () => tr('누르면 한 단계 확대하고, Alt 를 누르고 누르면 축소합니다. 끌어서 상자를 그리면 그 부분이 창에 맞게 커집니다. 다른 도구를 쓰다가 Ctrl+Space 를 누르고 있으면 잠시 확대/축소 도구가 됩니다. 도구를 두 번 누르면 100%(실제 크기)가 됩니다. Ctrl+Alt+2 는 이전 보기와 지금 보기를 오갑니다.'),
  },
};

const CONTROLS: Record<ControlHelpId, HelpEntry> = {
  fill: {
    name: () => tr('칠'), keys: ['X'],
    summary: () => tr('개체 안쪽(또는 글자)의 색입니다. 누르면 앞으로 와서 색을 받습니다.'),
    detail: () => tr('견본·색상·그레이디언트 패널에서 고른 색은 칠과 획 중 앞에 있는 쪽에 들어갑니다. X 로 앞을 바꾸고, 두 번 누르면 색상 피커가 열립니다. 아무것도 고르지 않았으면 다음에 그릴 개체의 기본값을 보입니다.'),
  },
  stroke: {
    name: () => tr('획'), keys: ['X'],
    summary: () => tr('개체 테두리(또는 글자 윤곽)의 색입니다. 누르면 앞으로 와서 색을 받습니다.'),
    detail: () => tr('획의 두께와 모양은 획 패널과 조절 막대에서 바꿉니다. X 로 칠과 획 중 앞을 바꾸고, 두 번 누르면 색상 피커가 열립니다.'),
  },
  swapFillStroke: {
    name: () => tr('칠과 획 맞바꾸기'), keys: ['Shift+X'],
    summary: () => tr('칠 색과 획 색을 서로 바꿉니다.'),
    detail: () => tr('고른 개체의 칠과 획 색을 맞바꾸고, 획 두께는 그대로 둡니다. 글자를 골랐으면 글자의 칠과 획을 바꿉니다(색이 여러 개면 구간마다).'),
  },
  defaultFillStroke: {
    name: () => tr('기본 칠과 획'), keys: ['D'],
    summary: () => tr('칠은 없음, 획은 검정으로 되돌립니다.'),
    detail: () => tr('고른 개체의 칠을 [없음], 획을 [검정]으로 바꿉니다. 글자에는 칠 [검정], 획 [없음]이 들어갑니다.'),
  },
  formatContainer: {
    name: () => tr('컨테이너에 서식 적용'), keys: ['J'],
    summary: () => tr('색을 텍스트 프레임 상자 자체에 넣습니다.'),
    detail: () => tr('켜져 있으면 칠과 획 색이 프레임의 바탕과 테두리에 들어갑니다. J 로 텍스트에 서식 적용과 번갈아 바꿉니다. 글자를 골라 둔 동안에는 늘 글자에 들어갑니다.'),
  },
  formatText: {
    name: () => tr('텍스트에 서식 적용'), keys: ['J'],
    summary: () => tr('색을 프레임 안의 글자에 넣습니다.'),
    detail: () => tr('켜져 있으면 텍스트 프레임을 골라도 칠과 획 색이 글 전체에 들어가고, 칠과 획 상자에 T 가 보입니다. J 로 컨테이너와 번갈아 바꾸고, 고른 것이 바뀌면 다시 꺼집니다.'),
  },
  applyColor: {
    name: () => tr('색상 적용'), keys: [','],
    summary: () => tr('마지막에 쓴 색을 앞에 있는 칠이나 획에 넣습니다.'),
    detail: () => tr('칠과 획 중 앞에 있는 쪽에 마지막으로 쓴 단색을 넣습니다. 넣을 개체가 없으면 회색입니다.'),
  },
  applyGradient: {
    name: () => tr('그레이디언트 적용'), keys: ['.'],
    summary: () => tr('마지막에 쓴 그레이디언트를 앞에 있는 칠이나 획에 넣습니다.'),
    detail: () => tr('쓴 그레이디언트가 없으면 기본 그레이디언트를 넣습니다. 방향과 길이는 그레이디언트 색상 견본 도구로 끌어 정합니다. 넣을 개체가 없으면 회색입니다.'),
  },
  applyNone: {
    name: () => tr('없음 적용'), keys: ['/'],
    summary: () => tr('앞에 있는 칠이나 획을 없앱니다.'),
    detail: () => tr('칠과 획 중 앞에 있는 쪽을 [없음]으로 바꿉니다. 칠을 없애면 안이 비치고, 획을 없애면 테두리가 사라집니다. 넣을 개체가 없으면 회색입니다.'),
  },
  screenMode: {
    name: () => tr('화면 모드'), command: ['view.toggleMode'],
    summary: () => tr('문서를 보는 방법을 바꿉니다. 누르면 표준과 미리 보기를 오갑니다.'),
    detail: () => tr('누르고 있거나 오른쪽 단추를 누르면 표준·미리 보기·도련·슬러그·프레젠테이션 중에서 고릅니다. 화면에서만 바뀌고 인쇄와 PDF 에는 영향이 없습니다.'),
  },
  screenNormal: {
    name: () => tr('표준'), command: ['view.toggleMode'],
    summary: () => tr('안내선·프레임 가장자리 같은 작업용 표시를 모두 보입니다.'),
    detail: () => tr('편집할 때 쓰는 기본 화면입니다. 쪽 밖 대지의 개체도 보입니다.'),
  },
  screenPreview: {
    name: () => tr('미리 보기'), command: ['view.toggleMode'],
    summary: () => tr('인쇄될 모습만 보입니다. 작업용 표시와 쪽 밖은 가립니다.'),
    detail: () => tr('안내선·프레임 가장자리·격자가 숨습니다. 편집은 그대로 할 수 있습니다.'),
  },
  screenBleed: {
    name: () => tr('도련'),
    summary: () => tr('미리 보기에 더해 도련 영역까지 보입니다.'),
    detail: () => tr('문서 설정의 도련 안에 있는 개체까지 보이고, 그 밖은 가립니다. 재단선 밖으로 넉넉히 깔았는지 확인할 때 씁니다.'),
  },
  screenSlug: {
    name: () => tr('슬러그'),
    summary: () => tr('미리 보기에 더해 도련과 슬러그 영역까지 보입니다.'),
    detail: () => tr('문서 설정의 슬러그(인쇄소에 남기는 메모 자리) 안에 있는 개체까지 보이고, 그 밖은 가립니다.'),
  },
  screenPresentation: {
    name: () => tr('프레젠테이션'), command: ['view.mode.presentation'],
    summary: () => tr('메뉴와 패널 없이 펼침면 하나를 화면 가득 보입니다.'),
    detail: () => tr('화살표 키·Page Down·Space 로 다음 펼침면으로 가고, Esc 로 나옵니다. 배경은 W·B·G 키로 흰색·검정·회색이 됩니다.'),
  },
  refPoint: {
    name: () => tr('기준점'),
    summary: () => tr('조절 막대의 X·Y 가 가리키는 점이고, 숫자로 크기·각도를 바꿀 때 고정되는 점입니다.'),
    detail: () => tr('아홉 칸 중 하나를 누르면 그 점(모서리·변의 가운데·한가운데)이 기준이 됩니다. W·H·회전·비율·기울이기 칸, 뒤집기, 회전·크기 조정 창, 페이지 도구로 페이지 크기를 바꿀 때 이 점이 제자리에 있습니다. 회전·크기 조정 도구로 끌 때의 중심은 이와 따로, 선택의 가운데에서 시작합니다.'),
  },
};

const ALL: Record<HelpId, HelpEntry> = { ...TOOLS, ...CONTROLS };
/** Every item, tools first (in no particular order beyond that; the screen finds them by id). */
export const HELP_IDS = Object.keys(ALL) as HelpId[];
export const helpEntry = (id: HelpId): HelpEntry | undefined => ALL[id];
export const isToolHelp = (id: HelpId): id is ToolId => Object.hasOwn(TOOLS, id);
