//! Titles, icons and initial sizes for the dock's panel registry.

import panelSpec from '../../../../shared/panels.json';
import { tr } from '../i18n';
import { HYPERLINK_PANELS } from '../features/hyperlinks/panels';
import { EDITORIAL_PANELS } from '../features/editorial/panels';
import { CONDITION_PANELS } from '../features/conditions/panel';
import { INDEX_PANELS } from '../features/indexes/panel';
import { TRAP_PANELS } from '../features/traps/panel';
import { FLATTENER_PANELS } from '../features/flattener/panel';
import { GLYPH_PANELS } from '../features/glyphs/panel';
import { DATA_MERGE_PANELS } from '../features/datamerge/panel';
import { PDF_COMMENT_PANELS } from '../features/pdfcomments/panel';
import { MATH_PANELS } from '../features/math/panel';

export type PanelId = keyof typeof panelSpec.panels;

/** Panel titles, icons and initial sizes. */
export const PANEL_META: Record<PanelId, { title: string; icon: string; width: number; height: number }> = {
  properties: { get title() { return tr('속성'); }, icon: 'transform', width: 310, height: 520 },
  ...EDITORIAL_PANELS,
  ...HYPERLINK_PANELS,
  pathfinder: { get title() { return tr('패스파인더'); }, icon: 'transform', width: 250, height: 240 },
  attributes: { get title() { return tr('특성'); }, icon: 'info', width: 250, height: 180 },
  scriptLabel: { get title() { return tr('스크립트 레이블'); }, icon: 'info', width: 280, height: 110 },
  toolHints: { get title() { return tr('도구 힌트'); }, icon: 'info', width: 260, height: 160 },
  backgroundTasks: { get title() { return tr('백그라운드 작업'); }, icon: 'info', width: 280, height: 160 },
  pages: { get title() { return tr("페이지"); }, icon: 'pages', width: 250, height: 520 },
  layers: { get title() { return tr("레이어"); }, icon: 'layers', width: 250, height: 340 },
  links: { get title() { return tr("링크"); }, icon: 'links', width: 300, height: 480 },
  stroke: { get title() { return tr("획"); }, icon: 'stroke', width: 250, height: 330 },
  color: { get title() { return tr("색상"); }, icon: 'color', width: 250, height: 260 },
  swatches: { get title() { return tr("색상 견본"); }, icon: 'swatches', width: 250, height: 420 },
  gradient: { get title() { return tr("그레이디언트"); }, icon: 'gradient', width: 250, height: 380 },
  project: { get title() { return tr("프로젝트"); }, icon: 'project', width: 270, height: 600 },
  preflight: { get title() { return tr("사전 검사"); }, icon: 'preflight', width: 320, height: 480 },
  separations: { get title() { return tr("분판 미리 보기"); }, icon: 'separations', width: 300, height: 380 },
  info: { get title() { return tr("정보"); }, icon: 'info', width: 250, height: 220 },
  character: { get title() { return tr("문자"); }, icon: 'character', width: 280, height: 420 },
  paragraph: { get title() { return tr("단락"); }, icon: 'paragraph', width: 280, height: 380 },
  paragraphStyles: { get title() { return tr("단락 스타일"); }, icon: 'paragraphStyles', width: 280, height: 380 },
  characterStyles: { get title() { return tr("문자 스타일"); }, icon: 'characterStyles', width: 280, height: 340 },
  objectStyles: { get title() { return tr("개체 스타일"); }, icon: 'objectStyles', width: 310, height: 420 },
  cellStyles: { get title() { return tr('셀 스타일'); }, icon: 'cellStyles', width: 250, height: 300 },
  tableStyles: { get title() { return tr('표 스타일'); }, icon: 'tableStyles', width: 250, height: 300 },
  align: { get title() { return tr("정렬"); }, icon: 'align', width: 270, height: 316 },
  transform: { get title() { return tr("변형"); }, icon: 'transform', width: 300, height: 200 },
  textWrap: { get title() { return tr("텍스트 감싸기"); }, icon: 'textWrap', width: 260, height: 230 },
  history: { get title() { return tr('작업 내역'); }, icon: 'history', width: 250, height: 300 },
  effects: { get title() { return tr("효과"); }, icon: 'effects', width: 250, height: 230 },
  tabs: { get title() { return tr('탭'); }, icon: 'paragraph', width: 430, height: 300 },
  table: { get title() { return tr('표'); }, icon: 'cellStyles', width: 270, height: 330 },
  ...CONDITION_PANELS,
  ...INDEX_PANELS,
  ...TRAP_PANELS,
  ...FLATTENER_PANELS,
  ...GLYPH_PANELS,
  ...PDF_COMMENT_PANELS,
  ...DATA_MERGE_PANELS,
  ...MATH_PANELS,
  // the Folio workspace's two views (uix-d2): docked like any panel, the explorer above the tools in the left area to begin with
  explorer: { get title() { return tr('탐색기'); }, icon: 'folder', width: 280, height: 520 },
  toolbox: { get title() { return tr('도구'); }, icon: 'selection', width: 240, height: 330 },
};

/** The Folio workspace's views (the explorer and the tools): they have a home in the left area, where Window ▸ Project, Window ▸
 *  Tools and the activity bar put them back when they were closed. */
export const VIEWS = ['explorer', 'toolbox'] as const satisfies readonly PanelId[];
export type ViewId = (typeof VIEWS)[number];
