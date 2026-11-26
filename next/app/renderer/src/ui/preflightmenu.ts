import { userSettings } from '../user-settings';
import { useUserPreference } from '../use-user-preference';
// The Preflight panel menu: Save Report (the findings as text), Limit Number of Rows Per Error
// (25, 50, 100 or all) and Enable Preflight for All Documents (preflight on opening a document).
// Settings are kept on this computer.
import { useState } from 'react';
import type { PreflightReport } from '../engine';
import { preflightReportText } from '../print';
import type { Command } from './commands';
import { tr } from '../i18n';

const LIMIT = 'folio.preflight.rowLimit';
const ALL = 'folio.preflight.all';

/** Whether a document is checked when it opens (Enable Preflight for All Documents). */
export function preflightOnOpen(): boolean {
  try { return userSettings.getItem(ALL) !== '0'; } catch { return true; }
}

function rowLimit(value: string | null): number | null {
  if (value === 'none') return null;
  const n = Number(value);
  return [25, 50, 100].includes(n) ? n : 100;
}

export type PreflightMenu = { limit: number | null; handlers: Record<string, Partial<Command>> };

export function usePreflightMenu({ report, document, note }: { report: PreflightReport | null; document: string | null; note: (text: string) => void }): PreflightMenu {
  const limit = rowLimit(useUserPreference(LIMIT));
  const [all, setAllState] = useState(preflightOnOpen);
  const setLimit = (v: number | null) => userSettings.setItem(LIMIT, v === null ? 'none' : String(v));
  const name = (document ?? '').split(/[\\/]/).pop()?.replace(/\.[^.]*$/, '') ?? 'document';
  const save = async () => {
    if (!report || !document) return;
    const saved = await window.folio.saveText(`${name} ${tr('사전 검사 보고서')}.txt`, preflightReportText(report, document));
    if (saved) note(tr('보고서를 저장했습니다: {0}', { 0: saved }));
  };
  const rows = (v: number | null) => ({ run: () => setLimit(v), checked: limit === v });
  return {
    limit,
    handlers: {
      'id.panel.preflight.saveReport': { run: report && document ? () => void save() : undefined },
      'id.panel.preflight.limitNumberOfRowsPerError.25': rows(25),
      'id.panel.preflight.limitNumberOfRowsPerError.50': rows(50),
      'id.panel.preflight.limitNumberOfRowsPerError.100': rows(100),
      'id.panel.preflight.limitNumberOfRowsPerError.noLimit': rows(null),
      'id.panel.preflight.enablePreflightForAllDocuments': {
        run: () => setAllState((v) => { try { userSettings.setItem(ALL, v ? '0' : '1'); } catch { /* this computer only */ } return !v; }),
        checked: all,
      },
    },
  };
}
