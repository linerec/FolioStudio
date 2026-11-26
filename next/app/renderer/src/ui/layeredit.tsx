import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';
// Layers panel editing: new layer, Layer Options, delete, the active layer, moving layers and
// moving objects to a layer. The rules (order, names, colours) are InDesign's, measured in
// next/oracle/scenarios/basic-layers.json and kept in the engine (edit/src/ops/layers.rs).
import { t as tr } from '../i18n';
import { useState } from 'react';
import { createPortal } from 'react-dom';
import type { EditCommand } from '../engine';
import type { DocumentInfo } from '../inspect';
import type { Command } from './commands';
import { LAYER_COLOR_NAMES } from './uicolors';
import { useLayerVisibility } from './useLayerVisibility';

type Layer = NonNullable<DocumentInfo['layers']>[number];
type Edit = (label: string, commands: EditCommand[]) => unknown;

/** The panel's name for a new layer: "레이어 N" / "Layer N", N the first free number from count + 1. */
export function newLayerName(layers: Layer[]): string {
  const names = new Set(layers.map((l) => l.name));
  for (let n = layers.length + 1; ; n++) {
    const name = tr('레이어 {0}', { 0: n });
    if (!names.has(name)) return name;
  }
}

export function newLayer(edit: Edit, layers: Layer[]) {
  edit(tr('새 레이어'), [{ op: 'newLayer', name: newLayerName(layers) }]);
}

/** The Layers panel's editing state and its panel menu (InDesign's rules in the engine:
 * oracle basic-layers, basic-layers2). Layers picked with Ctrl/Shift+click are what Merge
 * Layers merges into the active layer; one picked layer is what Duplicate, Delete and Layer
 * Options act on (else the active layer). */
export type LayerEditing = {
  visibility: ReturnType<typeof useLayerVisibility>;
  handlers: Record<string, Partial<Command>>;
  dialog: React.ReactNode;
  picked: Set<string>;
  pick: (id: string, add: boolean) => void;
  openOptions: (id: string) => void;
  remove: (id: string) => void;
  smallRows: boolean;
  /** The layers the panel acts on: those picked with Ctrl/Shift+click, else the active layer. */
  targetLayers: string[];
  /** Ungroup Remembers Layers (InDesign's application preference, default on). */
  ungroupRemembers: boolean;
  /** Paste Remembers Layers (InDesign's application preference, default off). */
  pasteRemembers: boolean;
};

const SMALL_ROWS = 'folio.layersSmallRows';
// InDesign keeps these two in the application's preferences, not in the document: they are
// this person's settings on this computer (never in the project folder).
const UNGROUP_REMEMBERS = 'folio.ungroupRemembersLayers';
const PASTE_REMEMBERS = 'folio.pasteRemembersLayers';
const flag = (key: string, fallback: boolean) => { try { const v = userSettings.getItem(key); return v === null ? fallback : v === '1'; } catch { return fallback; } };

export function useLayerEditing({ info, document, editable, edit: commit }: { info: DocumentInfo | null; document: string | undefined; editable: boolean; edit: Edit }): LayerEditing {
  const visibility = useLayerVisibility({layers: info?.layers, document, edit: commit});
  const edit = visibility.edit;
  const [options, setOptions] = useState<string | null>(null);
  const [confirm, setConfirm] = useState<string | null>(null);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const smallRows = useUserPreference(SMALL_ROWS) === '1';
  const [ungroupRemembers, setUngroupRemembers] = useState(() => flag(UNGROUP_REMEMBERS, true));
  const [pasteRemembers, setPasteRemembers] = useState(() => flag(PASTE_REMEMBERS, false));
  const toggle = (key: string, set: React.Dispatch<React.SetStateAction<boolean>>) => set((v) => { try { userSettings.setItem(key, v ? '0' : '1'); } catch { /* per-viewer only */ } return !v; });
  const layers = visibility.layers ?? [];
  const active = layers.find((l) => l.id === info?.activeLayer) ?? layers[layers.length - 1];
  const present = new Set(layers.map((l) => l.id));
  const chosen = [...picked].filter((id) => present.has(id));
  const target = (chosen.length === 1 ? layers.find((l) => l.id === chosen[0]) : undefined) ?? active;
  const remove = (id: string) => {
    const l = layers.find((x) => x.id === id);
    if (!l || layers.length < 2) return;
    if (l.items) setConfirm(id); else edit(tr('레이어 삭제'), [{ op: 'deleteLayer', id }]);
  };
  const merging = active ? chosen.filter((id) => id !== active.id) : [];
  const handlers: Record<string, Partial<Command>> = {
    'id.panel.layers.newLayer': { run: editable && layers.length ? () => newLayer(edit, layers) : undefined },
    'id.panel.layers.duplicateLayer': { run: editable && target ? () => edit(tr('레이어 복제'), [{ op: 'duplicateLayer', id: target.id }]) : undefined },
    'id.panel.layers.deleteLayer': { run: editable && target && layers.length > 1 ? () => remove(target.id) : undefined },
    'id.panel.layers.layerOptions': { run: editable && target ? () => setOptions(target.id) : undefined },
    'id.panel.layers.showAllLayers': { run: editable && layers.some((l) => !l.visible)
      ? () => edit(tr('모든 레이어 표시'), layers.filter((l) => !l.visible).map((l) => ({ op: 'setLayer', id: l.id, visible: true }) as EditCommand)) : undefined },
    'id.panel.layers.unlockAllLayers': { run: editable && layers.some((l) => l.locked)
      ? () => edit(tr('모든 레이어 잠금 해제'), layers.filter((l) => l.locked).map((l) => ({ op: 'setLayer', id: l.id, locked: false }) as EditCommand)) : undefined },
    // every object lock on every spread (layer locks stay): measured, oracle basic-layers3 unlock-all-items
    'id.panel.layers.unlockAll': { run: editable && layers.length ? () => edit(tr('모두 잠금 해제'), [{ op: 'unlockAll' }]) : undefined },
    'id.panel.layers.mergeLayers': { run: editable && active && merging.length
      ? () => { edit(tr('레이어 병합'), [{ op: 'mergeLayers', into: active.id, ids: merging }]); setPicked(new Set()); } : undefined },
    'id.panel.layers.deleteUnusedLayers': { run: editable && layers.length > 1 && layers.some((l) => !l.items)
      ? () => edit(tr('사용하지 않는 레이어 삭제'), [{ op: 'deleteUnusedLayers' }]) : undefined },
    'id.panel.layers.ungroupRemembersLayers': { checked: ungroupRemembers, run: () => toggle(UNGROUP_REMEMBERS, setUngroupRemembers) },
    'id.panel.layers.pasteRemembersLayers': { checked: pasteRemembers, run: () => toggle(PASTE_REMEMBERS, setPasteRemembers) },
    'id.panel.layers.smallPanelRows': { checked: smallRows, run: () => userSettings.setItem(SMALL_ROWS, userSettings.getItem(SMALL_ROWS) === '1' ? '0' : '1') },
  };
  const optionsLayer = options ? layers.find((l) => l.id === options) : undefined;
  const confirmLayer = confirm ? layers.find((l) => l.id === confirm) : undefined;
  const dialog = <>
    {optionsLayer && <LayerOptionsDialog layer={optionsLayer} layers={layers} color={optionsLayer.colorName ?? null} edit={edit} close={() => setOptions(null)} />}
    {confirmLayer && <DeleteLayerConfirm layer={confirmLayer} edit={edit} close={() => setConfirm(null)} />}
  </>;
  return {
    handlers, dialog, visibility, picked: new Set(chosen), smallRows, ungroupRemembers, pasteRemembers,
    targetLayers: chosen.length ? chosen : active ? [active.id] : [],
    pick: (id, add) => setPicked((p) => { const n = new Set(add ? p : []); if (add && n.has(id)) n.delete(id); else n.add(id); return n; }),
    openOptions: (id) => setOptions(id),
    remove,
  };
}

/** Deleting a layer with objects asks first, as InDesign does. */
export function DeleteLayerConfirm({ layer, edit, close }: { layer: Layer; edit: Edit; close: () => void }) {
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('레이어 삭제')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); edit(tr('레이어 삭제'), [{ op: 'deleteLayer', id: layer.id }]); close(); }}>
        <h2>{tr('레이어 삭제')}</h2>
        <p>{tr('레이어 "{0}"에 개체 {1}개가 있습니다. 레이어와 개체를 모두 지울까요?', { 0: layer.name, 1: layer.items })}</p>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" autoFocus>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}

export function LayerOptionsDialog({ layer, layers, color, edit, close }: { layer: Layer; layers: Layer[]; color: string | null; edit: Edit; close: () => void }) {
  const [name, setName] = useState(layer.name);
  const [hue, setHue] = useState(color ?? '');
  const [flags, setFlags] = useState({ visible: layer.visible, locked: layer.locked, printable: layer.printable, showGuides: layer.showGuides ?? true, lockGuides: layer.lockGuides ?? false });
  const taken = name.trim() !== layer.name && layers.some((l) => l.name === name.trim());
  const submit = () => {
    if (!name.trim() || taken) return;
    const changed: Record<string, unknown> = {};
    if (name.trim() !== layer.name) changed.name = name.trim();
    if (hue && hue !== color) changed.color = hue;
    for (const [k, v] of Object.entries(flags)) if (v !== (layer as Record<string, unknown>)[k] && !(k === 'showGuides' && layer.showGuides === undefined && v) && !(k === 'lockGuides' && layer.lockGuides === undefined && !v)) changed[k] = v;
    if (Object.keys(changed).length) edit(tr('레이어 옵션'), [{ op: 'setLayer', id: layer.id, ...changed } as EditCommand]);
    close();
  };
  const box = (key: keyof typeof flags, label: string) => (
    <label key={key}><input type="checkbox" checked={flags[key]} onChange={(e) => setFlags((f) => ({ ...f, [key]: e.target.checked }))} /> {label}</label>
  );
  return createPortal(
    <div className="style-modal" role="dialog" aria-label={tr('레이어 옵션')} onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') close(); }}>
      <form className="card dialog style-options" onSubmit={(e) => { e.preventDefault(); submit(); }}>
        <h2>{tr('레이어 옵션')}</h2>
        <label>{tr('이름')}<input aria-label={tr('레이어 이름')} autoFocus value={name} onChange={(e) => setName(e.target.value)} /></label>
        {taken && <p className="validation-message">{tr('같은 이름의 레이어가 있습니다.')}</p>}
        <label>{tr('색상')}<select aria-label={tr('레이어 색상')} value={hue} onChange={(e) => setHue(e.target.value)}>
          {!color && <option value="">{tr('지금 색 유지')}</option>}
          {LAYER_COLOR_NAMES().map(([v, n]) => <option key={v} value={v}>{n}</option>)}
        </select></label>
        <div className="layer-flags">
          {box('visible', tr('레이어 표시'))}{box('locked', tr('레이어 잠금'))}{box('printable', tr('레이어 인쇄'))}
          {box('showGuides', tr('안내선 표시'))}{box('lockGuides', tr('안내선 잠금'))}
        </div>
        <div className="dialog-actions"><button type="button" onClick={close}>{tr('취소')}</button><button type="submit" disabled={!name.trim() || taken}>{tr('확인')}</button></div>
      </form>
    </div>,
    document.body,
  );
}
