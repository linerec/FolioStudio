// Row editors a shared dialog's `array` field can name (`editor`) instead of the plain table of rows: an editor shows the rows and gives
// back the whole new list (the engine checks it and keeps it in order); the dialog frame sends it as the field's value.
import type { ReactNode } from 'react';
import { StopEditor, type Stop } from '../features/color/gradient';
import { SendStatus, type PrintSend, type Sent } from '../features/printing/send';
import { PdfExportStatus } from '../print';
import { PackagePlanPanel, PackageStatus } from '../package';
import type { Swatch } from './swatchedit';

export type Row = Record<string, number | boolean | string>;
export type Choice = { value: string; label: string; [more: string]: unknown };
export type RowEditor = (p: { rows: Row[]; change: (rows: Row[]) => void; choices: Record<string, Choice[]>; disabled: boolean }) => ReactNode;

export const ROW_EDITORS: Record<string, RowEditor> = {
  // a gradient's stops: the ramp, a button per stop, the picked stop's colour, place and midpoint, add, delete and reverse
  gradientStops: ({ rows, change, choices, disabled }) => <StopEditor stops={rows as unknown as Stop[]} change={(stops) => change(stops as unknown as Row[])} disabled={disabled}
    swatches={(choices.stopColors ?? []).map((c) => ({ id: c.value, name: c.label, model: c.model, rgb: c.rgb }) as unknown as Swatch)} />,
};

/** Status panels a shared dialog can name (`status`): what a button that keeps the dialog open answered, kept in the dialog's values by the
 *  engine (`then`), and the buttons that act on it (`run(action)` applies the dialog with that action). */
export type StatusPanel = (p: { values: Record<string, unknown>; running: string; busy: boolean; error: string; run: (action: string) => void;
  /** an action applied at once while another request of the dialog is out (File ▸ Package's Cancel while it runs) */
  now: (action: string) => void;
  /** a value set (File ▸ Package: back to the choices) */
  set: (field: string, value: number | boolean | string) => void;
  /** a field's label as the dialog shows it (the settings a print did not use are named by their fields) */
  labelOf: (name: string) => string }) => ReactNode;

/** Preview panels a shared dialog's `preview` of kind `panel` can name: the engine's answer drawn (File ▸ Package's plan). */
export type PreviewPanel = (p: { preview: Record<string, unknown>; values: Record<string, unknown>; set: (field: string, value: number | boolean | string) => void }) => ReactNode;

export const PREVIEW_PANELS: Record<string, PreviewPanel> = {
  // what the package would hold (packagePlan's counts), what to check, whether the folder can be made (a free name offered)
  packagePlan: ({ preview, values, set }) => <PackagePlanPanel preview={preview} values={values} set={set} />,
};

export const STATUS_PANELS: Record<string, StatusPanel> = {
  // a print sent to a printer (`sent`: printToDevice's answer, `canceled` once cancelJob took it back): the job, the settings it did not use,
  // the driver's warnings and "cancel the job" (File ▸ Print and File ▸ Print Booklet)
  printSend: ({ values, running, busy, error, run, labelOf }) => {
    const answer = values.sent as Partial<Sent> | undefined;
    // (an old success is not shown next to a new error)
    const sent = !error && answer?.job ? { ...answer, canceled: values.canceled === true } as Sent : null;
    return <SendStatus label={labelOf} state={{ sent, error: '', busy: running === 'print', cancel: async () => { if (!busy) run('cancelJob'); } } as unknown as PrintSend} />;
  },
  // File ▸ Export PDF: the pages written while it runs, then what the PDF has (`exported`)
  pdfExport: ({ values, running, error }) => <PdfExportStatus values={values} running={running} error={error} />,
  // File ▸ Package past its choices: the progress the engine keeps (`progress`, Cancel), what it made (`result`) or why not (`failure`)
  package: ({ values, now, set }) => <PackageStatus values={values} now={now} set={set} />,
};
