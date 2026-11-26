// Display Performance (View and Object menus, the context menus). One implementation behind every menu entry.
// Measured with InDesign 2025 (artifacts/displayperf-20261023/STATUS.ko.md):
//  - Object ▸ Display Performance writes the item's `LocalDisplaySetting` (and its graphic's, and what is inside a
//    group); a selected graphic (the content grabber) takes it alone. `Optimized` is Fast Display, `Default` is Use View Setting.
//  - View ▸ Display Performance ▸ Fast / Typical / High Quality is the window's own setting, not in the file; a window
//    starts with the application default (Preferences ▸ Display Performance ▸ Default View).
//  - Allow Object-Level Display Settings is the application preference "ignore" turned round, not in the file.
//  - Clear Object-Level Display Settings sets every item of every spread back to Use View Setting.
import { tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { ItemInfo, LocalDisplay, PageItems } from '../inspect';
import type { Command } from './commands';
import { findItem } from './overlay';

/** The DOM names (as Preferences ▸ Display Performance stores them). */
export type ViewDisplay = 'OPTIMIZED' | 'TYPICAL' | 'HIGH_QUALITY';

export const DEFAULT_VIEW_KEY = 'displayPerformancePreferences.defaultDisplaySettings';
export const IGNORE_LOCAL_KEY = 'displayPerformancePreferences.ignoreLocalSettings';

const VIEW_NAMES: readonly ViewDisplay[] = ['OPTIMIZED', 'TYPICAL', 'HIGH_QUALITY'];

/** The application's default view (a window opens with it); High Quality until the preferences say otherwise. */
export function defaultView(prefs: Record<string, unknown> | undefined): ViewDisplay {
  const v = prefs?.[DEFAULT_VIEW_KEY];
  return VIEW_NAMES.find((n) => n === v) ?? 'HIGH_QUALITY';
}

type Selected = { page: number; id: string; content: boolean };

type Props = {
  editable: boolean;
  hasDoc: boolean;
  /** The window's view setting (Fast / Typical / High Quality) and how to change it. */
  view: ViewDisplay;
  setView: (v: ViewDisplay) => void;
  /** The application preference "Allow Object-Level Display Settings" (the opposite of ignoring them) and how to change it. */
  allowLocal: boolean;
  setAllowLocal: (allow: boolean) => void;
  selection: Selected[];
  peek: (page: number) => PageItems | null | undefined;
  edit: (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
};

const OBJECT: [string, LocalDisplay][] = [
  ['id.object.displayPerformance.fastDisplay', 'Optimized'],
  ['id.object.displayPerformance.typicalDisplay', 'Typical'],
  ['id.object.displayPerformance.highQualityDisplay', 'HighQuality'],
  ['id.object.displayPerformance.useViewSetting', 'Default'],
];
const VIEW: [string, ViewDisplay][] = [
  ['id.view.displayPerformance.fastDisplay', 'OPTIMIZED'],
  ['id.view.displayPerformance.typicalDisplay', 'TYPICAL'],
  ['id.view.displayPerformance.highQualityDisplay', 'HIGH_QUALITY'],
];

/** The setting of what is selected: a frame's own, or its graphic's when the graphic is selected. */
function settingOf(item: ItemInfo, content: boolean): LocalDisplay {
  return (content ? item.content?.local_display : item.local_display) ?? 'Default';
}

export function displayPerformanceMenu({ editable, hasDoc, view, setView, allowLocal, setAllowLocal, selection, peek, edit }: Props): Record<string, Partial<Command>> {
  const found = selection.map((s) => ({ s, hit: findItem(peek(s.page), s.id) })).filter((x): x is { s: Selected; hit: NonNullable<typeof x.hit> } => !!x.hit);
  const settings = found.map(({ s, hit }) => settingOf(hit.item, s.content));
  // the check shows when every selected object has the setting (a mixed selection checks none)
  const same = settings.length > 0 && settings.length === selection.length && settings.every((v) => v === settings[0]) ? settings[0] : null;
  const out: Record<string, Partial<Command>> = {};
  for (const [id, setting] of OBJECT) {
    out[id] = {
      run: editable && found.length ? () => void edit(tr('표시 성능'), found.map(({ s }) => ({ op: 'setDisplayPerformance', ids: [s.id], setting, content: s.content }))) : undefined,
      checked: same === setting,
    };
  }
  for (const [id, mode] of VIEW) out[id] = { run: hasDoc ? () => setView(mode) : undefined, checked: view === mode };
  out['id.view.displayPerformance.allowObjectLevelDisplaySettings'] = { run: hasDoc ? () => setAllowLocal(!allowLocal) : undefined, checked: allowLocal };
  out['id.view.displayPerformance.clearObjectLevelDisplaySettings'] = {
    run: editable ? () => void edit(tr('개체 수준 표시 설정 지우기'), [{ op: 'clearDisplayPerformance' }]) : undefined,
  };
  return out;
}
