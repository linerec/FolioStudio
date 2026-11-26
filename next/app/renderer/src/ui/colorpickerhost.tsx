// Opens the Color Picker from a Fill or Stroke chip (toolbox, Color panel) and applies what it returns the way the toolbox's paint commands do
// (ui/toolboxpaint.ts): characters or a caret in text take the colour as character fill or stroke; with T on, the text frames' stories do
// (formatFrameText); otherwise the selected objects' fill or stroke (setPaintColor, one history step); with nothing selected it sets the document's default
// fill or stroke of the objects drawn from now on (setDefaultFillStroke, one history step; measured in InDesign 2025: colorpicker-c2). The colour is the unnamed hidden colour
// the Color panel's sliders make (engine setPaintColor, newUnnamedColor). Cancel changes nothing. Add Swatch makes a named swatch of the value (RGB, CMYK, Lab or HSB).
import { useState } from 'react';
import { t as tr } from '../i18n';
import type { EditCommand, EditResult } from '../engine';
import type { DocumentInfo, ItemInfo, Paint } from '../inspect';
import type { TextFormatting } from './textformat';
import { ColorPickerDialog, pickedName, type PickedColor, type PickerInitial, type PickSpace } from './colorpicker';

type Edit = (label: string, commands: EditCommand[]) => Promise<EditResult | null>;
type Swatches = DocumentInfo['swatches'] | undefined;

const spaceOf = (s: string | undefined): PickSpace | null => (s === 'CMYK' ? 'CMYK' : s === 'RGB' ? 'RGB' : s === 'LAB' || s === 'Lab' ? 'LAB' : null);

/** What the picker starts from: the paint's screen colour, and its own values when it is not tinted. */
export function initialOf(paint: Paint | null | undefined, swatches: Swatches): PickerInitial {
  if (!paint || paint.kind === 'none') return { rgb: [0, 0, 0], exact: null, none: true };
  const sw = paint.swatch ? swatches?.find((s) => s.id === paint.swatch || s.name === paint.swatch) : undefined;
  const space = spaceOf(sw?.space ?? paint.space), values = sw?.values ?? paint.values;
  const tinted = paint.tint != null && paint.tint >= 0 && paint.tint < 100;
  const exact = paint.kind !== 'gradient' && !tinted && space && values && values.length === (space === 'CMYK' ? 4 : 3) ? { space, value: values } : null;
  return { rgb: paint.rgb ?? sw?.rgb ?? [0, 0, 0], exact, none: false };
}

export function useColorPicker(p: { editable: boolean; textMode: boolean; text: TextFormatting; ids: string[]; items: ItemInfo[]; swatches: Swatches; strokeFocus: boolean; edit: Edit; note?: (message: string) => void; /** The document's default fill and stroke (what the chips show with nothing selected). */ defaults?: DocumentInfo['defaultFillStroke'] }) {
  const [open, setOpen] = useState<null | { stroke: boolean }>(null);
  const selection = p.editable ? p.text.selection : null;
  const swatches = p.swatches ?? [];
  const textPaint = (stroke: boolean): Paint | null => {
    if (!selection) return null;
    const id = selection.format[stroke ? 'strokeColor' : 'fillColor'];
    const tint = selection.format[stroke ? 'strokeTint' : 'fillTint'];
    const swatch = swatches.find((s) => s.id === id);
    return { kind: id === 'Swatch/None' ? 'none' : swatch?.model === 'gradient' ? 'gradient' : id ? 'swatch' : 'other', swatch: typeof id === 'string' ? id : null, tint: typeof tint === 'number' ? tint : null, rgb: swatch?.rgb ?? null };
  };
  const frameIds = p.editable && !selection && p.textMode ? p.items.filter((it) => it.text?.story).map((it) => it.id) : [];
  const objects = p.editable && !selection && !p.textMode && p.items.length > 0 && p.items.length === p.ids.length;
  // nothing selected: OK changes the default paint of new objects (the existing ones stay as they are)
  const noSelection = p.editable && !selection && p.ids.length === 0 && p.items.length === 0;
  const canApply = !!selection || frameIds.length > 0 || objects || noSelection;

  const apply = async (c: PickedColor, stroke: boolean) => {
    const label = tr('색상 선택기');
    const key = stroke ? 'strokeColor' : 'fillColor';
    if (selection) {
      // characters (or a caret) take the colour by id; InDesign makes ONE history step named Color Picker for it (measured), so the colour and the text format go in one edit
      const { header } = await window.folio.request({ cmd: 'colorPick', space: c.space, value: c.value });
      const id = typeof header.unnamedColorId === 'string' ? header.unnamedColorId : null;
      if (id) await p.edit(label, [{ op: 'newUnnamedColor', space: c.space, value: c.value }, { op: 'textFormat', story: selection.story, start: selection.start, end: selection.end, props: { [key]: id } }]);
    } else if (frameIds.length) {
      const { header } = await window.folio.request({ cmd: 'colorPick', space: c.space, value: c.value });
      const id = typeof header.unnamedColorId === 'string' ? header.unnamedColorId : null;
      if (id) await p.edit(label, [{ op: 'newUnnamedColor', space: c.space, value: c.value }, { op: 'formatFrameText', ids: frameIds, props: { [key]: id } }]);
    } else if (objects) {
      await p.edit(label, [{ op: 'setPaintColor', ids: p.ids, stroke, space: c.space, value: c.value }]);
    } else if (noSelection) {
      await p.edit(label, [{ op: 'setDefaultFillStroke', [key === 'strokeColor' ? 'stroke' : 'fill']: { space: c.space, value: c.value } }]);
    }
  };
  // Add ... Swatch: a swatch named by its value (InDesign rounds every number); the same name again makes "name 2", "name 3" (measured), the dialog stays open
  const addSwatch = async (c: PickedColor): Promise<string | null> => {
    const base = c.name ?? pickedName(c);
    // the names the document has right now (the panel's list may not have caught up with a swatch made a moment ago)
    const fresh = await window.folio.request({ cmd: 'documentInfo' }).then(({ header }) => ((header as { info?: { swatches?: { name: string }[] } })?.info?.swatches ?? []).map((s) => s.name)).catch(() => [] as string[]);
    const taken = new Set([...swatches.map((s) => s.name), ...fresh]);
    let name = base;
    for (let n = 2; taken.has(name); n++) name = `${base} ${n}`;
    const r = await p.edit(tr('새 색상 견본'), [{ op: 'newSwatch', name, spot: false, space: c.space.toLowerCase(), value: c.value }]);
    return r ? name : null;
  };
  const element = open ? (
    <ColorPickerDialog
      key={`${open.stroke}`}
      target={open.stroke ? 'stroke' : 'fill'}
      initial={initialOf(selection ? textPaint(open.stroke) : p.items[0] ? (open.stroke ? p.items[0].stroke : p.items[0].fill) : noSelection ? (open.stroke ? p.defaults?.stroke : p.defaults?.fill) : null, p.swatches)}
      canApply={canApply}
      onOk={(c) => { const stroke = open.stroke; setOpen(null); void apply(c, stroke); }}
      onCancel={() => setOpen(null)}
      onAddSwatch={addSwatch}
    />
  ) : null;
  return { open: (stroke: boolean) => setOpen({ stroke }), element, isOpen: !!open };
}
