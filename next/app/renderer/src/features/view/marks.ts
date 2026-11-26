// View ▸ Extras ▸ Live Corners / Anchored Object Control and View ▸ Grids & Guides ▸ Smart Guides.
// These are application settings (InDesign keeps them in app.generalPreferences / app.smartGuidePreferences,
// not in the document; measured: one value for every document and window, kept after closing and reopening).
// The keys are the same ones the Preferences dialog saves (features/preferences/application-fields.ts), so the menus,
// the dialog and the context menus read and write one place: window.folio.settings() / saveSettings().
// The drawing (position and conditions measured against InDesign's window) is `markPlacement`; rules in
// next/oracle/rules/viewmarks2.ko.md.
import type { ItemInfo } from '../../inspect';
import { corners } from '../../inspect';
import { tr } from '../../i18n';
import type { SmartPrefs } from '../../ui/smartguides';
import type { Command } from '../../ui/commands';

export const KEYS = {
  liveCorners: 'generalPreferences.showLiveCorners',
  anchorControl: 'generalPreferences.showAnchorObjectAdornment',
  conveyor: 'generalPreferences.showConveyor',
  smartGuides: 'smartGuidePreferences.enabled',
  // Preferences ▸ Guides & Pasteboard ▸ Smart Guide Options (InDesign: app.smartGuidePreferences), all on by default
  smartCenter: 'smartGuidePreferences.alignToObjectCenter',
  smartEdges: 'smartGuidePreferences.alignToObjectEdges',
  smartDimensions: 'smartGuidePreferences.smartDimensions',
  smartSpacing: 'smartGuidePreferences.smartSpacing',
  // Preferences ▸ Guides & Pasteboard ▸ Color ▸ Smart Guides (app.smartGuidePreferences.guideColor, GRID_GREEN by default)
  smartColor: 'smartGuidePreferences.guideColor',
} as const;

export type MarkPrefs = Record<string, unknown> | undefined;

/** The current state of the four settings (the session values InDesign showed; the factory defaults were not measured). */
export function viewMarksState(prefs: MarkPrefs) {
  return {
    liveCorners: prefs?.[KEYS.liveCorners] !== false,
    anchorControl: prefs?.[KEYS.anchorControl] !== false,
    conveyor: prefs?.[KEYS.conveyor] === true,
    smartGuides: prefs?.[KEYS.smartGuides] !== false,
    smart: {
      enabled: prefs?.[KEYS.smartGuides] !== false,
      center: prefs?.[KEYS.smartCenter] !== false,
      edges: prefs?.[KEYS.smartEdges] !== false,
      dimensions: prefs?.[KEYS.smartDimensions] !== false,
      spacing: prefs?.[KEYS.smartSpacing] !== false,
      color: typeof prefs?.[KEYS.smartColor] === 'string' ? prefs[KEYS.smartColor] as string : undefined,
    } as SmartPrefs,
  };
}
export type ViewMarksState = ReturnType<typeof viewMarksState>;

/** What the canvas draws: the state plus the tool (InDesign draws the live corner only for the Selection tool, the anchor control for it and the Direct Selection tool). */
export type ViewMarks = { liveCorners: boolean; anchorControl: boolean; tool: string };

export function viewMarksFor(state: ViewMarksState, tool: string): ViewMarks {
  return { liveCorners: state.liveCorners && tool === 'selection', anchorControl: state.anchorControl && (tool === 'selection' || tool === 'direct'), tool };
}

/** The menu items. Show/Hide switches with the setting and the item never shows a check mark (measured: enabled, unchecked in every state). */
export function viewMarkCommands(p: {
  prefs: MarkPrefs;
  set: (key: string, value: boolean) => void;
}): Record<string, Partial<Command>> {
  const s = viewMarksState(p.prefs);
  return {
    'id.view.extras.hideLiveCorners': {
      label: s.liveCorners ? tr('라이브 모퉁이 숨기기') : tr('라이브 모퉁이 표시'),
      run: () => p.set(KEYS.liveCorners, !s.liveCorners),
      // the square, the four diamonds, the drag (size) and Alt-click (shape) are built and measured against InDesign 2025
      // (features/view/livecorners.ts, next/oracle/rules/liveinput-c1.ko.md, next/oracle/measured/live-corners-2025.json)
    },
    'id.view.extras.hideAnchoredObjectControl': {
      label: s.anchorControl ? tr('고정 개체 컨트롤 숨기기') : tr('고정 개체 컨트롤 표시'),
      run: () => p.set(KEYS.anchorControl, !s.anchorControl),
      // the control is drawn like InDesign's and its drag into text anchors the object (features/anchored/useAnchorControl.ts, engine `anchorObject`;
      // next/oracle/rules/liveinput-c2.ko.md); a text frame that is turned, flipped or scaled takes the object with it about its origin, as InDesign does
      // (next/oracle/rules/anchorturn-c1.ko.md, 30 measured drops). The square of a plain object and the anchor symbol box of an object set in text are drawn where
      // InDesign draws them on plain, turned, flipped and narrow objects (`markPlacement`, next/oracle/rules/anchorbox-c1.ko.md, 127 measured cases within 1.5 px);
      // hiding the control hides both, and the setting is the Preferences key `generalPreferences.showAnchorObjectAdornment` (the menu item and the key follow each other)
    },
    // View ▸ Extras ▸ Show / Hide Conveyor: the bar of the Content Collector / Placer tools (ui/conveyorbar.tsx); the item is enabled and never checked
    'id.view.extras.hideConveyor': {
      label: s.conveyor ? tr('컨베이어 숨기기') : tr('컨베이어 표시'),
      run: () => p.set(KEYS.conveyor, !s.conveyor),
    },
    // checked (measured: the menu item stays "Smart Guides" and carries the check); the pull and the lines are ui/smartguides.ts
    // (next/oracle/rules/smartguides-c2.ko.md: alignment, equal spacing, equal size, the four options and Shift / Alt measured against InDesign 2025)
    'view.smartGuides': {
      run: () => p.set(KEYS.smartGuides, !s.smartGuides), checked: s.smartGuides,
    },
  };
}

// ---- the drawing -------------------------------------------------------------------------------------------------
/** Screen pixels, measured in InDesign's window (3821 px wide capture). */
export const LIVE_SIZE = 9;           // yellow square, 1 px border in the layer colour
export const LIVE_MIN_HEIGHT = 28;    // hidden below this frame height (25 px hidden, 31 px shown)
export const ANCHOR_BOX = 19;         // the anchor symbol box (an object already anchored in text)
export const ANCHOR_SQUARE = 9;       // the plain square (any other object)
export const ANCHOR_MIN_WIDTH = 19;   // hidden below this frame width (17 px hidden, 21 px shown)
/** Where the anchored object control sits, measured again with a calibrated screenshot rig (next/oracle/rules/anchorbox-c1.ko.md; 127 cases in
 *  next/oracle/measured/anchor-box-2025.json: frames and objects turned 10-330 degrees, flipped across / down / both, turned and flipped, other sizes and zoom levels,
 *  objects set in text inline / above the line / custom; the scene built by DOM scripts, the window only read, no input):
 *  the point E is on the object's own top edge, `fraction` (20 %) of its width in from its own right end; the widget (always upright on the screen, 9 x 8 for a
 *  plain object, 19 x 19 for an object set in text) has its centre at `E + (x, y) - back * u`, u = the screen direction of the object's own x axis, (x, y) a fixed
 *  screen offset in px. On a narrow object (screen width under `narrowBase + narrowPer * s`, s = px per pt) InDesign takes 25 % instead of 20 % and keeps the
 *  box at least `minD` from the right end. This is the earlier "4.4 px to the own left" rule with the constants measured again: the plain square within 1.1 px
 *  and the box within 1.4 px of InDesign (rms 0.5; the earlier limit of 1.5 px came from a calibration that was about 1 px off). */
export const ANCHOR_SQUARE_AT = { x: 3.34, y: -0.33, back: 4.45, narrowBase: 24.8, narrowPer: 14.2, minD: 0 };
export const ANCHOR_BOX_AT = { x: 8.47, y: -0.21, back: 8.71, narrowBase: 39.5, narrowPer: 17.5, minD: 9.06 };
export const ANCHOR_FRACTION = 0.2, ANCHOR_FRACTION_NARROW = 0.25;

export type MarkSpot = { x: number; y: number };
export type MarkPlacement = { live: MarkSpot | null; anchor: MarkSpot | null; anchored: boolean };

/** Frames that have live corners: rectangles and text frames (an oval, a polygon, a line and a group have none). */
export const hasLiveCorners = (it: ItemInfo) => (it.kind === 'rectangle' || it.kind === 'textFrame') && !it.locked;

/**
 * Where the two marks of a single selected object go, in screen pixels (`s` = pixels per point), along the frame's own axes
 * (a rotated frame carries them with it, measured at 30 degrees):
 * - live corner: on the right edge, `min(30.5, height / 2 - 9)` below the top edge (hidden when the frame is under 28 px tall);
 * - anchor control: at the point of the top edge 20 % of the width in from the right edge (25 % on a narrow object), moved by a fixed screen offset
 *   (`ANCHOR_*_AT`, measured on plain, turned and flipped objects; hidden when the frame is under 19 px wide).
 */
export function markPlacement(it: ItemInfo, s: number, marks: Pick<ViewMarks, 'liveCorners' | 'anchorControl'>): MarkPlacement {
  const [tl, tr_, , bl] = corners(it.transform, it.inner);
  const wx = (tr_[0] - tl[0]) * s, wy = (tr_[1] - tl[1]) * s, hx = (bl[0] - tl[0]) * s, hy = (bl[1] - tl[1]) * s;
  const w = Math.hypot(wx, wy), h = Math.hypot(hx, hy);
  const out: MarkPlacement = { live: null, anchor: null, anchored: !!it.anchored };
  if (w < 1 || h < 1) return out;
  const ux = wx / w, uy = wy / w, vx = hx / h, vy = hy / h;
  if (marks.liveCorners && hasLiveCorners(it) && h >= LIVE_MIN_HEIGHT) {
    const d = Math.min(30.5, h / 2 - 9);
    out.live = { x: tr_[0] * s + vx * d, y: tr_[1] * s + vy * d };
  }
  if (marks.anchorControl && w >= ANCHOR_MIN_WIDTH) {
    const k = it.anchored ? ANCHOR_BOX_AT : ANCHOR_SQUARE_AT;
    const d = w < k.narrowBase + k.narrowPer * s ? Math.max(ANCHOR_FRACTION_NARROW * w, k.minD) : ANCHOR_FRACTION * w;
    const ex = tr_[0] * s - ux * d, ey = tr_[1] * s - uy * d;   // the point E on the own top edge, 20 % (a narrow object: 25 %) in from the own right end
    out.anchor = { x: ex + k.x - k.back * ux, y: ey + k.y - k.back * uy };
  }
  return out;
}
