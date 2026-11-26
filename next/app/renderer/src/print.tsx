import { t as tr, useLanguage } from './i18n';
import { engineMessage } from './i18n/engine-message';
import './pdf/pdf.css';
// Print production UI: preflight and links panels, what File ▸ Export PDF made (the dialog is the shared frame's `pdfExport`).
import { useEffect, useMemo, useState } from 'react';
import {
  type ExportResult,
  type Finding,
  type PdfPreset,
  type PreflightReport,
} from './engine';

/** The PDF/X presets, as InDesign 2025 names its built-in ones, with what each does. */
export const PRESETS: { id: PdfPreset; name: string; standard: string; summary: string; detail: string }[] = [
  {
    id: 'x4',
    name: 'PDF/X-4',
    standard: 'PDF/X-4 (ISO 15930-7)',
    get summary() { return tr("투명도·색을 그대로 둠 — 요즘 인쇄소 대부분, InDesign 기본"); },
    get detail() { return tr("CMYK·별색·오버프린트·투명도를 바꾸지 않고 그대로 둡니다(PDF 1.6)."); },
  },
  {
    id: 'x1a',
    name: 'PDF/X-1a:2001',
    standard: 'PDF/X-1a:2001 (ISO 15930-1)',
    get summary() { return tr("투명도를 합치고 CMYK·별색만 — 인쇄소가 X-1a를 요구할 때"); },
    get detail() { return tr("PDF 1.3. 투명도(불투명도·혼합 모드·그림자·페더 등)가 있는 곳만 잉크 그대로 그림으로 합칩니다(선·글자 1200 ppi, 나머지 300 ppi). 그 밖은 벡터·글자 그대로. RGB·Lab은 문서 CMYK로 바꾸고 CMYK 숫자는 그대로 둡니다."); },
  },
  {
    id: 'x3',
    name: 'PDF/X-3:2002',
    standard: 'PDF/X-3:2002 (ISO 15930-3)',
    get summary() { return tr("투명도를 합치고 색 관리된 RGB는 남김 — 인쇄소가 X-3을 요구할 때"); },
    get detail() { return tr("PDF 1.3. 투명도가 있는 곳만 잉크 그대로 그림으로 합칩니다(선·글자 1200 ppi, 나머지 300 ppi). RGB는 프로파일을 붙여 그대로 둡니다."); },
  },
];

export const presetOf = (id: PdfPreset | undefined) => PRESETS.find((p) => p.id === id) ?? PRESETS[0];

/** Where a finding or link sits, for "go to" and the page highlight. */
export type Target = { page: number; bounds: number[] | null };

// Rule ids (crates/preflight/src/rules.rs) → panel group and title.
const groupsForLanguage = (): [string, string[]][] => [
  [tr("글꼴"), ['FONT_MISSING', 'FONT_SUBSTITUTED', 'FONT_NOT_EMBEDDED', 'MISSING_GLYPH']],
  [tr("링크"), ['LINK_MISSING', 'LINK_MODIFIED', 'LINK_AMBIGUOUS', 'LINK_NAME_ONLY', 'LINK_RELOCATED', 'LINK_UNREADABLE', 'LINK_EMBEDDED', 'LINK_EPS_PREVIEW']],
  [tr("이미지"), ['IMAGE_RESOLUTION', 'COLOR_SPACE', 'IMAGE_INK_COVERAGE']],
  [tr("색·잉크"), ['NO_CMYK_PROFILE', 'INK_COVERAGE', 'SPOT_COLORS', 'SPOT_UNUSED', 'SPOT_LOOKS_PROCESS', 'SPOT_TO_PROCESS', 'INK_ALIAS', 'REGISTRATION_COLOR', 'TRANSPARENCY_SPOT']],
  [tr("오버프린트"), ['WHITE_OVERPRINT', 'OVERPRINT_COLOR', 'BLACK_TEXT_KNOCKOUT', 'SMALL_TEXT_INKS']],
  [tr("텍스트"), ['OVERSET_TEXT']],
  [tr("개체"), ['HAIRLINE', 'HIDDEN_CONTENT', 'EMPTY_FRAME', 'TRANSPARENCY_USED', 'UNSUPPORTED_EFFECT']],
  [tr("재단·문서"), ['BLEED_MISSING', 'SAFETY_MARGIN', 'BLANK_PAGE', 'PAGE_SIZES']],
];

const TITLES: Record<string, string> = {
  get FONT_MISSING() { return tr("없는 글꼴"); },
  get FONT_SUBSTITUTED() { return tr("대체된 글꼴"); },
  get FONT_NOT_EMBEDDED() { return tr("포함할 수 없는 글꼴"); },
  get MISSING_GLYPH() { return tr("없는 글리프"); },
  get LINK_MISSING() { return tr("없는 링크"); },
  get LINK_MODIFIED() { return tr("수정된 링크"); },
  get LINK_AMBIGUOUS() { return tr("같은 이름이 여러 개인 링크"); },
  get LINK_NAME_ONLY() { return tr("이름으로만 찾은 링크"); },
  get LINK_RELOCATED() { return tr("다른 위치에서 찾은 링크"); },
  get LINK_UNREADABLE() { return tr("읽을 수 없는 링크"); },
  get LINK_EPS_PREVIEW() { return tr("미리 보기로 출력되는 EPS"); },
  get LINK_EMBEDDED() { return tr("포함된 링크"); },
  get IMAGE_RESOLUTION() { return tr("이미지 해상도 부족"); },
  get COLOR_SPACE() { return tr("RGB·Lab 색 공간"); },
  get IMAGE_INK_COVERAGE() { return tr("이미지 잉크 총량 초과"); },
  get NO_CMYK_PROFILE() { return tr("CMYK 프로파일 없음"); },
  get INK_COVERAGE() { return tr("잉크 총량 초과"); },
  get SPOT_COLORS() { return tr("별색"); },
  get SPOT_UNUSED() { return tr("쓰이지 않는 별색"); },
  get SPOT_LOOKS_PROCESS() { return tr("원색 이름의 별색"); },
  get SPOT_TO_PROCESS() { return tr("원색으로 바뀌는 별색"); },
  get INK_ALIAS() { return tr("잉크 별칭"); },
  get REGISTRATION_COLOR() { return tr("맞춰찍기 색 사용"); },
  get TRANSPARENCY_SPOT() { return tr("투명도와 겹친 별색"); },
  get WHITE_OVERPRINT() { return tr("흰색 오버프린트"); },
  get OVERPRINT_COLOR() { return tr("오버프린트"); },
  get BLACK_TEXT_KNOCKOUT() { return tr("녹아웃된 검정 글자"); },
  get SMALL_TEXT_INKS() { return tr("여러 잉크로 된 작은 글자"); },
  get OVERSET_TEXT() { return tr("넘친 텍스트"); },
  get HAIRLINE() { return tr("가는 선"); },
  get HIDDEN_CONTENT() { return tr("숨은 개체"); },
  get EMPTY_FRAME() { return tr("빈 프레임"); },
  get TRANSPARENCY_USED() { return tr("투명도 사용"); },
  get UNSUPPORTED_EFFECT() { return tr("아직 그리지 못하는 효과"); },
  get BLEED_MISSING() { return tr("도련 부족"); },
  get SAFETY_MARGIN() { return tr("재단선에 가까운 개체"); },
  get BLANK_PAGE() { return tr("빈 페이지"); },
  get PAGE_SIZES() { return tr("페이지 크기가 다름"); },
};

const SEVERITY_LABEL = { get error() { return tr("오류"); }, get warning() { return tr("경고"); }, get info() { return tr("정보"); } } as const;

/** Preflight panel ▸ Limit Number of Rows Per Error: how many findings a rule lists (null: all). */
export function PreflightView({ report, busy, onRerun, onGo, limit = null }: { report: PreflightReport | null; busy: boolean; onRerun: () => void; onGo: (t: Target) => void; limit?: number | null }) {
  const [show, setShow] = useState<Record<string, boolean>>({ error: true, warning: true, info: false });
  const [open, setOpen] = useState<Record<string, boolean>>({});
  const [selected, setSelected] = useState<Finding | null>(null);

  const language = useLanguage();
  const groups = useMemo(() => {
    if (!report) return [];
    const GROUPS = groupsForLanguage();
    const known = new Set(GROUPS.flatMap(([, ids]) => ids));
    const all: [string, string[]][] = [...GROUPS, [tr("기타"), [...new Set(report.findings.map((f) => f.rule))].filter((r) => !known.has(r))]];
    return all
      .map(([name, ids]) => {
        const rules = ids
          .map((rule) => ({ rule, findings: report.findings.filter((f) => f.rule === rule && show[f.severity]) }))
          .filter((r) => r.findings.length);
        return { name, rules, count: rules.reduce((n, r) => n + r.findings.length, 0) };
      })
      .filter((g) => g.count);
  }, [report, show, language]);

  const s = report?.summary;
  return (
    <div className="panel-body">
      <div className="panel-head">
        <div className="counts">
          {(['error', 'warning', 'info'] as const).map((k) => (
            <label key={k} className={`sev ${k}`}>
              <input type="checkbox" checked={show[k]} onChange={(e) => setShow({ ...show, [k]: e.target.checked })} />
              {SEVERITY_LABEL[k]} {s ? (k === 'error' ? s.errors : k === 'warning' ? s.warnings : s.infos) : '–'}
            </label>
          ))}
        </div>
        <button onClick={onRerun} disabled={busy}>{busy ? tr("검사 중…") : tr("다시 검사")}</button>
      </div>
      {!report && !busy && <p className="empty">{tr("문서를 열면 검사합니다.")}</p>}
      {report && groups.length === 0 && <p className="empty ok">{tr("표시할 문제가 없습니다.")}</p>}
      <div className="tree">
        {groups.map((g) => (
          <div key={g.name} className="group-block">
            <button className="group-title" onClick={() => setOpen({ ...open, [g.name]: !(open[g.name] ?? true) })}>
              <span className="twisty">{open[g.name] ?? true ? '▾' : '▸'}</span>
              {g.name} <span className="n">{g.count}</span>
            </button>
            {(open[g.name] ?? true) &&
              g.rules.map((r) => (
                <RuleBlock key={r.rule} rule={r.rule} findings={r.findings} limit={limit} selected={selected} onSelect={(f) => { setSelected(f); if (f.page !== null) onGo({ page: f.page, bounds: f.bounds }); }} />
              ))}
          </div>
        ))}
      </div>
      {selected && (
        <div className="detail-box">
          <b>{TITLES[selected.rule] ?? selected.rule}</b>
          <p>{engineMessage(selected)}</p>
          <span>{selected.pageName ? tr('{0}쪽', {0:selected.pageName}) : tr("문서 전체")}{selected.item ? ` · ${tr('개체 {0}', {0:selected.item})}` : ''}</span>
        </div>
      )}
      {s && <div className="foot">{tr('검사 {0}초', {0:(s.elapsedMs / 1000).toFixed(2)})}</div>}
    </div>
  );
}

function RuleBlock({ rule, findings, limit, selected, onSelect }: { rule: string; findings: Finding[]; limit: number | null; selected: Finding | null; onSelect: (f: Finding) => void }) {
  const [open, setOpen] = useState(false);
  const worst = findings.some((f) => f.severity === 'error') ? 'error' : findings.some((f) => f.severity === 'warning') ? 'warning' : 'info';
  const LIMIT = limit ?? Infinity;
  return (
    <div className="rule">
      <button className="rule-title" onClick={() => setOpen(!open)}>
        <span className="twisty">{open ? '▾' : '▸'}</span>
        <i className={`dot ${worst}`} />
        {TITLES[rule] ?? rule}
        <span className="n">{findings.length}</span>
      </button>
      {open && (
        <ul>
          {findings.slice(0, LIMIT).map((f, i) => (
            <li key={i}>
              <button className={selected === f ? 'finding on' : 'finding'} onClick={() => onSelect(f)} title={engineMessage(f)}>
                <span className="msg">{engineMessage(f)}</span>
                <span className="where">{f.pageName ?? '—'}</span>
              </button>
            </li>
          ))}
          {findings.length > LIMIT && <li className="more">{tr('… 외 {0}개', {0:findings.length - LIMIT})}</li>}
        </ul>
      )}
    </div>
  );
}

/** File ▸ Export PDF's status panel (the shared frame's `status: pdfExport`): the pages written while the export runs (the engine's
 *  `progress` events of job export), then what the PDF has (`exported`, the export request's answer the engine kept in the dialog). */
export function PdfExportStatus({ values, running, error }: { values: Record<string, unknown>; running: string; error: string }) {
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  useEffect(() => { if (running !== 'ok') setProgress(null); }, [running]);
  useEffect(() => window.folio.onEvent((e) => {
    if (e.event === 'progress' && e.job === 'export') setProgress((p) => ({ done: e.done ?? p?.done ?? 0, total: e.total ?? p?.total ?? 0 }));
  }), []);
  const result = values.exported as ExportResult | undefined;
  if (running === 'ok') return <div data-pdf-progress>
    <p>{tr('내보내는 중… {0}', { 0: progress?.total ? tr('{0} / {1}쪽', { 0: progress.done, 1: progress.total }) : '' })}</p>
    <div className="bar"><div style={{ width: `${progress?.total ? (100 * progress.done) / progress.total : 5}%` }} /></div>
  </div>;
  return !error && result?.path ? <div data-pdf-exported><ExportDone result={result} /></div> : null;
}

function ExportDone({ result }: { result: ExportResult }) {
  // the engine's words: "own plate", "converted to process", "alias of X" (+ " (unused)")
  const spots = result.inks.filter((i) => i.kind === 'spot' && i.output === 'own plate');
  const converted = result.inks.filter((i) => i.kind === 'spot' && i.output.endsWith('converted to process'));
  return (
    <>
      <p>
        <b>{result.path.split(/[\/]/).pop()}</b> — {result.pages}{tr('쪽, ')}{(result.bytes / 1048576).toFixed(1)} MB, {(result.ms / 1000).toFixed(1)}{tr('초')}</p>
      <p className="muted small">
        {result.standard ?? 'PDF/X-4'} {tr(' · 출력 의도 ')}{result.outputIntent || tr('(없음)')} {tr(' · 글꼴 ')}{result.fontsEmbedded}{tr('개 포함')}{result.fontsSubstituted.length > 0 && <span className="bad">{tr(' · 대체 글꼴 {0}개', {0: result.fontsSubstituted.length})}</span>}
      </p>
      {result.flatten && (
        <p className="small">
          {result.flatten.pages > 0
            ? tr('투명도 병합: {0}쪽 (투명 개체 {1}개, 선·글자 {2} ppi · 그라디언트·효과 {3} ppi, 나머지는 벡터 그대로)', {0: result.flatten.pages, 1: result.flatten.objects, 2: Math.round(result.flatten.lineArtPpi), 3: Math.round(result.flatten.smoothPpi)})
            : tr('투명도 병합: 투명도가 있는 쪽 없음 — 모두 벡터 그대로')}
          {result.flatten.previewGraphics > 0 && <span className="bad">{tr(' · 배치 PDF {0}개는 미리보기로 합침', {0: result.flatten.previewGraphics})}</span>}
        </p>
      )}
      {result.converted && (result.converted.rgbImages > 0 || (result.converted.placedPdfs ?? 0) > 0) && (
        <p className="muted small">{tr('{0}를 {1}로 바꿈', {0: [
            result.converted.rgbImages > 0 && tr('RGB 이미지 {0}개', {0: result.converted.rgbImages}),
            (result.converted.placedPdfs ?? 0) > 0 && tr('배치 PDF {0}개의 RGB', {0: result.converted.placedPdfs}),
          ]
            .filter(Boolean)
            .join(', '), 1: result.converted.destination})}</p>
      )}
      <p className="small">
        {tr('판: C · M · Y · K')}{spots.length > 0 && tr(' + 별색 {0}', {0: spots.map((i) => i.name).join(', ')})}
        {converted.length > 0 && <span className="muted">{tr(' (원색으로 바뀐 별색 {0}개)', {0: converted.length})}</span>}
      </p>
      {result.fontsSubstituted.length > 0 && (
        <ul className="warnings">
          {result.fontsSubstituted.map((f, i) => (
            <li key={i}>{f.family} {f.style} → {f.embedded ?? tr('그리지 않음')}</li>
          ))}
        </ul>
      )}
      {result.warnings.length > 0 && (
        <ul className="warnings">
          {result.warnings.map((w, i) => (
            <li key={i}>{w.message}{w.count > 1 ? ` (${w.count})` : ''}</li>
          ))}
        </ul>
      )}
      <p><button type="button" data-pdf-action="reveal" onClick={() => window.folio.reveal(result.path)}>{tr('폴더에서 보기')}</button></p>
    </>
  );
}


/** Preflight panel ▸ Save Report: the findings as text, one line each under its rule. */
export function preflightReportText(report: PreflightReport, document: string): string {
  const s = report.summary;
  const out = [`Folio Studio — ${tr('사전 검사 보고서')}`, document, new Date().toLocaleString(),
    `${SEVERITY_LABEL.error} ${s.errors} · ${SEVERITY_LABEL.warning} ${s.warnings} · ${SEVERITY_LABEL.info} ${s.infos}`, ''];
  const rules = [...new Set(report.findings.map((f) => f.rule))];
  for (const rule of rules) {
    const list = report.findings.filter((f) => f.rule === rule);
    out.push(`${TITLES[rule] ?? rule} (${list.length})`);
    for (const f of list) out.push(`  [${SEVERITY_LABEL[f.severity]}] ${f.pageName ? tr('{0}쪽', { 0: f.pageName }) : tr('문서 전체')}: ${engineMessage(f)}`);
    out.push('');
  }
  return out.join(String.fromCharCode(10));
}
