// InDesign's interface colours (`UIColors`, as IDML names them, in the order its menus list them) with the names the
// screen shows. Layers use the first 26; grids and guides can use all of them (measured: oracle/indesign/probe-grid-prefs.jsx).
import { tr } from '../i18n';

export const UI_COLOR_NAMES = (): [string, string][] => [
  ['LightBlue', tr('연한 파랑')], ['Red', tr('빨강')], ['Green', tr('초록')], ['Blue', tr('파랑')], ['Yellow', tr('노랑')],
  ['Magenta', tr('자홍')], ['Cyan', tr('청록')], ['Gray', tr('회색')], ['Black', tr('검정')], ['Orange', tr('주황')],
  ['DarkGreen', tr('진한 초록')], ['Teal', tr('청록 회색')], ['Tan', tr('황갈')], ['Brown', tr('갈색')], ['Violet', tr('보라')],
  ['Gold', tr('금색')], ['DarkBlue', tr('진한 파랑')], ['Pink', tr('분홍')], ['Lavender', tr('연보라')], ['BrickRed', tr('벽돌색')],
  ['OliveGreen', tr('올리브 초록')], ['Peach', tr('복숭아색')], ['Burgundy', tr('버건디')], ['GrassGreen', tr('풀색')],
  ['Ochre', tr('황토')], ['Purple', tr('자주')],
  ['LightGray', tr('연한 회색')], ['Charcoal', tr('숯색')], ['GridBlue', tr('격자 파랑')], ['GridOrange', tr('격자 주황')],
  ['Fiesta', tr('피에스타')], ['LightOlive', tr('연한 올리브')], ['Lipstick', tr('립스틱')], ['CuteTeal', tr('큐트 틸')],
  ['Sulphur', tr('유황색')], ['GridGreen', tr('격자 초록')], ['White', tr('흰색')],
];

/** Colours a layer can have: InDesign's Layer Options menu stops at Purple. */
export const LAYER_COLOR_NAMES = () => UI_COLOR_NAMES().slice(0, 26);
