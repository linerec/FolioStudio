// The limits of Folio. The list is next/oracle/rules/limits.toml; the engine answers it with the query `knownLimits` and puts what a
// document uses into the import plan, the import report and the IDML export report (`limits`: principle 2, item 3, what InDesign will
// show differently). Help ▸ 아직 지원하지 않는 것 shows only what is not supported or left out (i18n/limits.generated.ts, written by
// next/tools/limits-report.py; user decision 2026-11-21: the screen does not explain how Folio differs from InDesign). Nothing here
// decides anything: it only shows.
import { useState } from 'react';
import { currentLanguage, t as tr, useLanguage } from '../i18n';
import { LIMITS, type LimitDifference } from '../i18n/limits.generated';

/** A difference as the engine reports it for a document (`count`: the places in the document that use it). */
export type LimitRow = { id: string; kind: string; area: string; ko: string; en: string; count?: number };
/** The `limits` block of the import plan / import report / IDML export report. */
export type LimitsReport = { used: LimitRow[]; fixed: LimitRow[]; doc?: string };

const kindLabel = (k: string): string => {
  switch (k) {
    case 'approx': return tr('근사');
    case 'unsupported': return tr('지원하지 않음');
    case 'unverified': return tr('검증하지 못함');
    case 'excluded': return tr('제외');
    default: return k;
  }
};
const areaName = (id: string): string => {
  const a = LIMITS.areas.find((x) => x.id === id);
  return a ? (currentLanguage() === 'ko' ? a.ko : a.en) : id;
};
const words = (d: { ko: string; en: string }): string => (currentLanguage() === 'ko' ? d.ko : d.en);

/** The block the import plan, and the IDML export dialog, show: what this document uses, then the fixed differences of this version. */
export function LimitsBlock({ limits }: { limits: LimitsReport | undefined | null }) {
  useLanguage();
  if (!limits) return null;
  const { used, fixed } = limits;
  return (
    <section className="limits-block" data-limits="block">
      {used.length > 0 && (
        <>
          <h3>{tr('InDesign과 달라질 수 있는 것 (이 문서가 쓰는 기능)')}</h3>
          <ul className="pkg-lines" data-limits="used">
            {used.map((d) => (
              <li key={d.id} className="warn" data-limit={d.id}>
                <b>{kindLabel(d.kind)}</b> · {areaName(d.area)} — {words(d)}{d.count ? ` (${d.count})` : ''}
              </li>
            ))}
          </ul>
        </>
      )}
      {fixed.length > 0 && (
        <details data-limits="fixed">
          <summary className="small">{tr('이 버전의 알려진 차이 (문서 안에서 확인할 수 없는 것 {0}가지)', { 0: fixed.length })}</summary>
          <ul className="pkg-lines">
            {fixed.map((d) => <li key={d.id} data-limit={d.id}><b>{kindLabel(d.kind)}</b> · {areaName(d.area)} — {words(d)}</li>)}
          </ul>
          <p className="muted small">{tr('지원하지 않는 기능 목록은 도움말 ▸ 아직 지원하지 않는 것에 있습니다.')}</p>
        </details>
      )}
    </section>
  );
}

/** Help ▸ 아직 지원하지 않는 것…: what Folio does not support yet (`unsupported`), by area, and what it left out on purpose (`excluded`).
 *  It never compares Folio with InDesign and never lists what was not checked against it (user decision 2026-11-21): limits-report.py
 *  gives the screen only these two kinds; the approximations stay in the engine's `knownLimits`, the import and export reports and
 *  artifacts/finish-20261113/LIMITS.ko.md. */
export function LimitsDialog({ onClose }: { onClose: () => void }) {
  useLanguage();
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const unsupported = LIMITS.differences.filter((d) => d.kind === 'unsupported');
  const excluded = LIMITS.differences.filter((d) => d.kind === 'excluded');
  const line = (d: LimitDifference) => <li key={d.id} data-limit={d.id}>{words(d)}</li>;
  return (
    <div className="overlay" onClick={onClose}>
      <div className="card dialog limits-dialog" data-limits="dialog" onClick={(e) => e.stopPropagation()}>
        <h2>{tr('아직 지원하지 않는 것')}</h2>
        <div className="limits-body">
          <p>{tr('Folio Studio 가 아직 지원하지 않는 기능과 일부러 뺀 기능입니다. 영역을 누르면 목록이 열립니다.')}</p>
          <section>
            <h3>{tr('지원하지 않음 ({0}가지)', { 0: unsupported.length })}</h3>
            {LIMITS.areas.filter((a) => unsupported.some((d) => d.area === a.id)).map((a) => {
              const rows = unsupported.filter((d) => d.area === a.id);
              return (
                <div key={a.id} className="limits-area">
                  <button className="tl-head" onClick={() => setOpen((o) => ({ ...o, [a.id]: !o[a.id] }))} aria-expanded={!!open[a.id]} data-limits-area={a.id}>
                    <span>{open[a.id] ? '▾' : '▸'}</span> {areaName(a.id)}<em>{rows.length}</em>
                  </button>
                  {open[a.id] && <ul className="small">{rows.map(line)}</ul>}
                </div>
              );
            })}
          </section>
          {excluded.length > 0 && (
            <section data-limits="excluded">
              <h3>{tr('일부러 뺀 기능 ({0}가지)', { 0: excluded.length })}</h3>
              <ul className="small">{excluded.map(line)}</ul>
            </section>
          )}
        </div>
        <div className="actions"><button className="primary" onClick={onClose}>{tr('닫기')}</button></div>
      </div>
    </div>
  );
}
