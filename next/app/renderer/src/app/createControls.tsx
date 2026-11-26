import { anchorOf } from './shared';
import { t as tr } from '../i18n';
import { pageOf } from '../inspect';
import { type ControlProps } from '../ui/control';
import { cornerCommands,cornerTargets } from '../ui/corners';
import { strokeTargets } from '../ui/stroke';
import { blendCommand,blendTargets } from '../ui/effects';
import { findItem } from '../ui/overlay';
import type { AppContext } from './context';
import type { Commands } from '../ui/commands';
import type { PanelCtx } from '../ui/panels';

export function createControls(context: AppContext, commands: Commands, ctx: PanelCtx) {
  const {
    selection, store, settings,
    info, runEdit, selectedPage,
    doc, effectiveTool, units,
    rulerOrigin, refPoint, setRefPoint,
    updateSettings, settingsRef, flash,
    readOnly, textMode, setTextMode,
    editable, selectedIds, transformMenu,
    framedId, setFxDialog, alignment,
    textFormatting,
  } = context;
  // fill and stroke of the selection for the tools panel proxy
  const selItem = selection[0] ? findItem(store.peek(selection[0].page), selection[0].id)?.item ?? null : null;
  const swatchy = (p: { kind: string; rgb: [number, number, number] | null; swatch: string | null } | undefined, fallbackNone: boolean) =>
    p ? { rgb: p.rgb, none: p.kind === 'none', name: p.swatch } : { rgb: fallbackNone ? null : ([0, 0, 0] as [number, number, number]), none: fallbackNone, name: null };

  // Page tool: Objects Move with Page is an application setting (InDesign: a preference of the application, not of the document)
  const OBJECTS_MOVE_WITH_PAGE_KEY = 'folio.page.objectsMoveWithPage';
  const objectsMoveWithPage = settings.preferences?.[OBJECTS_MOVE_WITH_PAGE_KEY] === true;
  /** A page dragged with the Page tool or given a new X / Y: the engine moves it against the rest of its spread (and its objects with it when the option is on). */
  const movePageBy = (index: number, dx: number, dy: number) => {
    const pg = pageOf(info, index);
    if (!pg) return;
    runEdit(tr('페이지 위치'), [{ op: 'movePage', page: pg.id, by: [dx, dy], objects_move_with_page: objectsMoveWithPage }]);
  };

  const selectedPageInfo = selectedPage !== null && doc ? { index: selectedPage, width: doc.pages[selectedPage].width, height: doc.pages[selectedPage].height } : null;
  // the control bar and the Transform panel edit the selection with the same controls
  const controlProps: ControlProps = {
    tool: effectiveTool,
    selection,
    store,
    info,
    units,
    rulerOrigin,
    refPoint,
    setRefPoint,
    selectedPage: selectedPageInfo,
    onPageResize: (width, height, start) => {
      if (!selectedPageInfo) return;
      const pg = pageOf(info, selectedPageInfo.index);
      if (!pg) return;
      if (!pg.spreadOrigin) return;
      // a value being tried is measured from the page as it was when the trial began (the page changes while it is shown)
      const x = (start ? start[2] : pg.spreadOrigin[0]) + ((start ? start[0] : selectedPageInfo.width) - width) * refPoint[0];
      const y = (start ? start[3] : pg.spreadOrigin[1]) + ((start ? start[1] : selectedPageInfo.height) - height) * refPoint[1];
      runEdit(tr('페이지 크기'), [{ op: 'setPageGeometry', pages: [pg.id], bounds: [x, y, x + width, y + height] }]);
    },
    // Page Size list: the engine owns the sizes and the way a page takes one (setPageSize, checked against Page.reframe)
    onPagePreset: (preset) => {
      if (!selectedPageInfo) return;
      const pg = pageOf(info, selectedPageInfo.index);
      if (pg) runEdit(tr('페이지 크기'), [{ op: 'setPageSize', pages: [pg.id], preset, width: undefined, height: undefined, anchor: [refPoint[0], refPoint[1]] }]);
    },
    // X / Y of the page: moves it by the difference, as dragging it with the Page tool does (`movePage`)
    onPageMove: (dx, dy) => { if (selectedPageInfo) movePageBy(selectedPageInfo.index, dx, dy); },
    objectsMoveWithPage,
    onObjectsMoveWithPage: (on) => { void updateSettings({ preferences: { ...settingsRef.current.preferences, [OBJECTS_MOVE_WITH_PAGE_KEY]: on } }).catch((e) => flash(String(e))); },
    readOnly,
    textMode,
    setTextMode,
    onCommand: (id) => commands[id]?.run?.(),
    editable,
    onMove: (dx, dy) => runEdit(tr('이동'), [{ op: 'move', ids: selectedIds(), by: [dx, dy] }]),
    onResize: (w, h) => {
      const ids = selectedIds();
      const includeStroke = transformMenu.prefs.includeStroke;
      if (ids.length > 1) runEdit(tr('크기 조정'), [{ op: 'resizeMany', ids, size: [w, h], anchor: anchorOf(refPoint), includeStroke }]);
      else if (ids[0]) runEdit(tr('크기 조정'), [{ op: 'resize', id: ids[0], size: [w, h], anchor: anchorOf(refPoint), includeStroke }]);
    },
    includeStroke: transformMenu.prefs.includeStroke,
    totals: transformMenu.prefs.totals,
    onRotate: (by) => runEdit(tr('회전'), [{ op: 'rotate', ids: selectedIds(), by, anchor: anchorOf(refPoint) }]),
    onScale: (sx, sy) => {
      const ids = selectedIds();
      // a text frame scales with the text it shows: the engine's `scaleMany` does it (the frame's options and the text's attributes)
      const text = selection.some((s) => !s.content && findItem(store.peek(s.page), s.id)?.item.kind === 'textFrame');
      if (ids.length > 1 || text) runEdit(tr('비율'), [{ op: 'scaleMany', ids, sx, sy, anchor: anchorOf(refPoint), adjust: { stroke: transformMenu.prefs.adjustStroke, effects: transformMenu.prefs.adjustEffects } }]);
      else if (ids[0]) runEdit(tr('비율'), [{ op: 'scale', id: ids[0], sx, sy, anchor: anchorOf(refPoint) }]);
    },
    onAngles: (angles) => {
      const ids = selectedIds(), grabbed = selection.find((s) => s.content);
      if (angles.content) { if (grabbed) runEdit(tr('기울이기'), [{ op: 'setAngles', id: grabbed.id, ...angles, anchor: anchorOf(refPoint) }]); return; }
      if (ids.length > 1 && angles.shear !== undefined) { runEdit(tr('기울이기'), [{ op: 'shear', ids, by: angles.shear, anchor: anchorOf(refPoint), adjust: { stroke: transformMenu.prefs.adjustStroke, effects: transformMenu.prefs.adjustEffects } }]); return; }
      if (ids[0]) runEdit(angles.shear !== undefined ? tr('기울이기') : tr('회전'), [{ op: 'setAngles', id: ids[0], ...angles, anchor: anchorOf(refPoint), totals: transformMenu.prefs.totals }]);
    },
    onFlip: (axis) => runEdit(axis === 'horizontal' ? tr('가로로 뒤집기') : tr('세로로 뒤집기'), [{ op: 'flip', ids: selectedIds(), axis, anchor: anchorOf(refPoint) }]),
    onFit: (how) => { const id = framedId(); if (id) runEdit(tr('맞춤'), [{ op: 'fit', id, how }]); },
    onStrokeWeight: (weight) => runEdit(tr('획 두께'), [{ op: 'setPaint', ids: selectedIds(), strokeWeight: weight }]),
    onPaint: (kind, swatch) => runEdit(kind === 'fill' ? tr('칠 적용') : tr('획 적용'), [{ op: 'setPaint', ids: selectedIds(), ...(kind === 'fill' ? { fill: swatch } : { stroke: swatch }) }]),
    onTextWrap: (mode) => { const id = selectedIds()[0]; if (id) runEdit(tr('텍스트 감싸기'), [{ op: 'textWrap', id, mode }]); },
    onObjectStyle: (style) => runEdit(tr('개체 스타일 적용'), [{ op: 'applyObjectStyle', ids: selectedIds(), style, clear: false }]),
    onOpacity: (v) => { const t = blendTargets(selection, (pg) => store.peek(pg)); if (t.length) runEdit(tr('불투명도'), [blendCommand(t, t.every((x) => x.graphic) ? 'graphic' : 'object', { opacity: v })]); },
    onEffects: editable && selection.length && blendTargets(selection, (pg) => store.peek(pg)).length === selection.length ? () => setFxDialog({ page: 'transparency' }) : undefined,
    onDropShadow: editable && selection.length && blendTargets(selection, (pg) => store.peek(pg)).length === selection.length ? async () => {
      // on when the first object has none, off when it has one (the object level, or the graphic's)
      const t = blendTargets(selection, (pg) => store.peek(pg));
      const level = t.every((x) => x.graphic) ? 'graphic' : 'object';
      const { header } = await window.folio.request({ cmd: 'effects', item: t[0].id });
      const on = !header.ok || !header.effects?.levels?.[level]?.effects?.dropShadow?.on;
      runEdit(tr('그림자'), [{ op: 'setEffect', ids: t.map((x) => x.id), level, effect: 'dropShadow', on, values: {} }]);
    } : undefined,
    onStroke: (change, label) => { const t = strokeTargets(selection, (pg) => store.peek(pg)); if (t.length) runEdit(label, [{ op: 'setStroke', ids: t.map((x) => x.id), change }]); },
    onCorners: (change, label) => { const t = cornerTargets(selection, (pg) => store.peek(pg)); if (t.length) runEdit(label, cornerCommands(t, change)); },
    alignment,
    textFormatting,
  };
  ctx.control = controlProps;
  return {
    controlProps, swatchy, selItem,
    movePageBy,
  };
}
