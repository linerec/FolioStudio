import { tr } from '../../i18n';

/** Sending a print to a printer. Every rule (which printers exist, what a setting means for the device, the checks, the job id) lives in the
 *  engine queries `listPrinters`, `printToDevice` and `cancelPrintJob`, which the shared dialogs `printing` and `booklet` send; this is
 *  only the line that shows what the one job last sent became. */
export type Sent = { job: { id: string; printer: string; warnings: string[] }; sheets: number; copies: number; request: Record<string, unknown>; ignored: string[]; canceled?: boolean };
/** The job last sent, an error, whether one is being sent, and taking it back. */
export type PrintSend = { sent: Sent | null; error: string; busy: boolean; cancel: () => Promise<void> };

/** What happened to the job last sent: where it went, the settings that were not used (named by `label`, the dialog's field labels),
 *  a warning of the driver, an error; a sent job can be taken back. */
export function SendStatus({ state, label = (key) => key }: { state: PrintSend; label?: (key: string) => string }) {
  const { sent, error, cancel, busy } = state;
  return <div className="print-send-status">
    {busy && <p role="status" data-print-sending>{tr('프린터로 보내는 중입니다…')}</p>}
    {sent && <p role="status" data-print-sent data-print-job={sent.job.id} data-print-request={JSON.stringify(sent.request)}>
      {sent.canceled ? tr('작업 {0}을(를) 취소했습니다.', { 0: sent.job.id })
        : tr('{0}(으)로 보냈습니다. 작업 {1}, {2}장, {3}부.', { 0: sent.job.printer, 1: sent.job.id, 2: sent.sheets, 3: sent.copies })}
      {/* (a plain button: in the shared dialog frame it sits in the dialog's form, which it must not submit) */}
      {!sent.canceled && <button type="button" data-print-cancel-job disabled={busy} onClick={() => void cancel()}>{tr('작업 취소')}</button>}
    </p>}
    {sent?.job.warnings.map((w, i) => <p key={i} role="note" data-print-warning>{w}</p>)}
    {sent && sent.ignored.length > 0 && <p role="note" data-print-ignored>{tr('이 설정은 프린터로 보낼 때 쓰이지 않았습니다: {0}', { 0: sent.ignored.map(label).join(', ') })}</p>}
    {error && <p role="alert" data-print-send-error>{error}</p>}
  </div>;
}
