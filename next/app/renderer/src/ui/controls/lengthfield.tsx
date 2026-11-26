// Small helpers for the panels and dialogs that move to the shared number field (numberfield.tsx) from plain text boxes:
//   lengthSpec(unit)    the `format` / `parse` / `step` of a length kept in points and shown in the document's unit
//                       (the same text the old boxes showed and took: "12 mm", "3p6", "10mm+2", picas, ciceros...);
//   parseInteger        a whole number, refusing "3.5" and "" (the old count boxes did);
//   TextNumberField     the field for a dialog that keeps its entries as text (`useState('12 pt')`): it shows and gives text,
//                       so the dialog's own checks and submit need no change.
import { formatLength, parseLength, unitOf } from '../../inspect';
import { NumberField, type NumberFieldProps } from './numberfield';
import './numberfield.dialog.css';

export { formatInteger, formatNumeric, parseInteger, parseNumeric, stepOfLength } from './entries';

/** What a length field needs, for lengths in points shown in `unitName` (a ViewPreference unit name; none is points). */
export function lengthSpec(unitName: string | undefined) {
  const u = unitOf(unitName);
  return {
    format: (pt: number) => formatLength(pt, unitName),
    parse: (text: string) => parseLength(text, unitName),
    /** One arrow press: a pica or cicero, else one of the unit (a millimetre, an inch). */
    step: u.kind ? 1 : 1 / u.perPoint,
  };
}

type TextNumberProps = Omit<NumberFieldProps, 'value' | 'onCommit' | 'onPreview' | 'onCancel' | 'format' | 'parse' | 'min' | 'max'> & {
  /** The entry as the dialog keeps it. */
  text: string;
  /** The entry changed (a typed value, a step, a pick): the text to keep. */
  onText: (text: string) => void;
  /** Writes a number as the dialog keeps its entries; `parse` reads it back. */
  format: (value: number) => string;
  parse: (text: string) => number | null;
  min?: number;
  max?: number;
};

/**
 * A number field for a dialog whose entries are texts. The box shows the number the text means (blank when it means none),
 * and a value chosen in it (typed, a step, a drag, a list pick) becomes the text again. There is no preview: the dialog applies
 * its entries when it is confirmed. Enter in the box applies the entry and then the dialog's default button works as before; Esc takes the typing back.
 */
export function TextNumberField({ text, onText, format, parse, min = -1e9, max = 1e9, width = 120, ...field }: TextNumberProps) {
  return <NumberField {...field} width={width} value={parse(text)} format={format} parse={parse} min={min} max={max} inDialog onCommit={(v) => onText(format(v))} />;
}
