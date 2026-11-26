// A NumberField wired to the engine's preview session: the panel gives only the undo step's name and what a value means
// (`build`: the edit commands for it). The common case; a panel with its own needs uses NumberField and usePreviewEdit.
import type { EditCommand } from '../../engine';
import { NumberField, type NumberFieldProps } from './numberfield';
import { type PreviewPort, usePreviewEdit } from './usePreviewEdit';

export type PreviewNumberFieldProps = Omit<NumberFieldProps, 'onCommit' | 'onPreview' | 'onCancel'> & {
  /** The undo step's name (and what the history lists). */
  undoLabel: string;
  /** The edit commands that give the field this value (in the field's own unit: convert here). */
  /** `base` is the value the trial began from (null for a typed value): needed only by relative edits. */
  build: (value: number, base?: number | null) => EditCommand[];
  edit: (label: string, commands: EditCommand[]) => unknown;
  /** A preview session of the caller's own (default: the window's); `null`: no preview. */
  port?: PreviewPort | null;
};

export function PreviewNumberField({ undoLabel, build, edit, port, ...field }: PreviewNumberFieldProps) {
  const handlers = usePreviewEdit(undoLabel, build, edit, port);
  return <NumberField {...field} {...handlers} />;
}
