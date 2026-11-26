import { t as tr, hasTranslation } from './i18n';
import { engineMessage } from './i18n/engine-message';
// File ▸ Package (패키지, Ctrl+Alt+Shift+P) is the shared dialog frame's `package` (next/shared/dialogs/package.json): the engine holds
// its choices, plans it (packagePlan), makes it on its own thread with the progress in the dialog's values, and keeps what it made.
// These are only the two panels that draw what the engine says: the plan beside the choices, and the stages after OK.
import { type PackagePlan, type PackageResult, type PdfPreset } from './engine';
import { presetOf } from './print';
import { presetName } from './pdf/api';

const STAGE: Record<string, string> = { get plan() { return tr('준비하는 중'); }, get copy() { return tr('파일 복사 중'); }, get pdf() { return tr('PDF 만드는 중'); }, get report() { return tr('보고서 쓰는 중'); }, get finish() { return tr('마무리하는 중'); } };

const NOT_COPIED: Record<string, string> = {
  get adobeFonts() { return tr('구독 글꼴'); },
  get nonAdobeCjk() { return tr('공급업체 제한 한중일'); },
  get fsTypeRestricted() { return tr('사용권 제한'); },
  get fsTypePreviewPrint() { return tr('인쇄·미리보기만 허용'); },
  get fsTypeBitmapOnly() { return tr('비트맵만 허용'); },
  get indesignOnly() { return tr('InDesign 전용'); },
  get unreadable() { return tr('읽을 수 없음'); },
};

type Export = { preset?: PdfPreset; settings?: import('./pdf/types').PdfPreset; bleed: boolean; marks: { crop: boolean; bleed: boolean; registration: boolean; pageInfo: boolean }; spreads: boolean };
type Progress = { stage?: string; done?: number; total?: number; files?: number; filesTotal?: number; pdf?: string | null };
type Failure = { code?: string; error?: string; messageId?: string; messageArgs?: Record<string, unknown> };

/** What the package would hold (the engine's packagePlan for the dialog's values), what to check, and whether the folder can be made. */
export function PackagePlanPanel({ preview, values, set }: { preview: Record<string, unknown>; values: Record<string, unknown>; set: (field: string, value: string) => void }) {
  const plan = preview.plan as PackagePlan;
  const pages = Number(preview.pages ?? 0);
  const c = plan.counts;
  const missing = c.links.missing + c.links.ambiguous;
  const notCopied = Object.entries(plan.preview.notCopiedByCode ?? {});
  const pf = plan.preview.preflight;
  const names = plan.preview.missingLinks.map((l) => l.name);
  const shown = [...new Set(names)].slice(0, 5);
  const dirState = plan.dir;
  const pdf = values.pdf === true && plan.pdfPossible;
  return (
    <div data-package-plan>
      <ul className="pkg-lines">
        <li data-package-item="document"><b>{tr('문서')}</b> {tr('{0} · {1} · {2}쪽 (원본을 그대로 복사)', { 0: c.document.name, 1: size(c.document.bytes), 2: pages })}</li>
        <li data-package-item="links"><b>{tr('링크')}</b> {values.links === true ? tr('파일 {0}개 · {1}', { 0: c.links.files, 1: size(c.links.bytes) }) : tr('복사하지 않음')}
          {missing > 0 && <b className="bad">{tr(' · 없음 {0}곳', { 0: missing })}</b>}
          {c.links.renamed > 0 && <>{tr(' · 이름 바꿈 {0}', { 0: c.links.renamed })}</>}
          {c.links.embedded > 0 && <>{tr(' · 포함됨 {0}', { 0: c.links.embedded })}</>}</li>
        <li data-package-item="fonts"><b>{tr('글꼴')}</b> {values.fonts === true ? tr('파일 {0}개 · {1} (쓰인 글꼴 {2}개 중 {3}개)', { 0: c.fonts.files, 1: size(c.fonts.bytes), 2: c.fonts.used, 3: c.fonts.copied }) : tr('복사하지 않음')}
          {c.fonts.missing + c.fonts.substituted > 0 && <b className="bad">{tr(' · 없음 {0}{1}', { 0: c.fonts.missing, 1: c.fonts.substituted ? tr(' · 대체 {0}', { 0: c.fonts.substituted }) : '' })}</b>}</li>
        <li data-package-item="pdf"><b>PDF</b> {plan.pdfPossible ? (pdf ? tr('{0} · 내보내기 설정: {1}', { 0: c.pdfName, 1: exportText(values.export as Export) }) : tr('복사하지 않음')) : <span className="bad">{tr('문서 CMYK 프로파일이 이 컴퓨터에 없어 만들 수 없음')}</span>}</li>
        <li data-package-item="report"><b>{tr('보고서')}</b> {tr('Instructions.txt (인쇄소용, 한국어) · package-report.json')}</li>
      </ul>
      {(missing > 0 || notCopied.length > 0 || c.fonts.missing + c.fonts.substituted > 0 || pf.errors > 0) && (
        <section>
          <h3>{tr('확인할 것')}</h3>
          <ul className="pkg-alerts">
            {missing > 0 && <li>{tr('없는 링크 {0}곳 — 패키지에 넣지 못하고 보고서에 적습니다: {1}{2}', { 0: missing, 1: shown.join(', '), 2: new Set(names).size > shown.length ? tr(' 외 {0}개', { 0: new Set(names).size - shown.length }) : '' })}</li>}
            {c.fonts.missing + c.fonts.substituted > 0 && <li>{tr('이 컴퓨터에 없는 글꼴 {0}개{1} — 인쇄소에 그 글꼴이 있어야 합니다.', { 0: c.fonts.missing, 1: c.fonts.substituted ? tr(', 대체된 글꼴 {0}개', { 0: c.fonts.substituted }) : '' })}</li>}
            {notCopied.length > 0 && <li>{tr('복사하지 않는 글꼴 {0}개 ({1}) — InDesign과 같은 규칙입니다.', { 0: c.fonts.notCopied, 1: notCopied.map(([k, n]) => `${NOT_COPIED[k] ?? k} ${n}`).join(' · ') })}</li>}
            {pf.errors > 0 && <li>{tr('사전 검사 오류 ')}{pf.errors}{tr('개 (')}{pf.errorRules.slice(0, 4).map((r) => `${hasTranslation(`preflight.${r.rule}`) ? tr(`preflight.${r.rule}`) : r.title} ${r.count}`).join(', ')})</li>}
          </ul>
        </section>
      )}
      {dirState && !dirState.ok && (
        <p className="bad small" data-package-dir="refused">
          {dirState.code === 'exists' ? tr('같은 이름의 폴더가 이미 있습니다. 다른 이름을 입력하세요.') : engineMessage({ ...dirState, message: dirState.error })}
          {dirState.suggestion && <> <button type="button" className="link-button" data-package-suggestion onClick={() => set('name', dirState.suggestion!)}>{tr('"{0}"(으)로', { 0: dirState.suggestion })}</button></>}
        </p>
      )}
      {dirState?.ok && dirState.warnings?.map((w, i) => <p key={w} className="warnline small">{engineMessage({ message: w, ...dirState.warningMessages?.[i] })}</p>)}
    </div>
  );
}

/** File ▸ Package past its choices: the progress the engine keeps in the dialog (with Cancel), what it made, or why not. */
export function PackageStatus({ values, now, set }: { values: Record<string, unknown>; now: (action: string) => void; set: (field: string, value: string) => void }) {
  const stage = values.stage;
  if (stage === 'running') {
    const p = (values.progress ?? {}) as Progress;
    const cancelling = values.cancelling === true;
    const total = p.total ?? 0, done = p.done ?? 0;
    const pct = total ? Math.min(100, (100 * done) / total) : 3;
    const withPdf = values.pdf === true && values.pdfPossible === true;
    return (
      <div className="pkg-progress" data-package-progress={p.stage ?? ''}>
        <p className="stage">
          <span>{cancelling ? tr('취소하는 중…') : `${STAGE[p.stage ?? 'plan'] ?? p.stage}…`}</span>
          <span className="muted">{p.filesTotal ? tr('파일 {0} / {1} · ', { 0: p.files ?? 0, 1: p.filesTotal }) : ''}{size(done)} / {size(total)}</span>
        </p>
        <div className="bar"><div style={{ width: `${pct}%` }} /></div>
        {withPdf && <p className="muted small">{tr('PDF: {0} (복사와 함께 진행)', { 0: p.pdf === 'done' ? tr('완료') : tr('만드는 중') })}</p>}
        <p className="muted small">{tr('복사하는 동안에도 화면을 넘겨 볼 수 있습니다.')}</p>
        <div className="actions"><button type="button" data-package-cancel onClick={() => now('cancel')} disabled={cancelling}>{tr('취소')}</button></div>
      </div>
    );
  }
  if (stage === 'done') return <Done result={values.result as PackageResult} />;
  const f = (values.failure ?? {}) as Failure;
  const cancelled = f.code === 'cancelled';
  return (
    <div data-package-failed={f.code ?? ''}>
      <p className={cancelled ? '' : 'bad'}>
        {cancelled ? tr('취소했습니다. 폴더를 만들지 않았습니다.') : tr('패키지를 만들지 못했습니다: {0}', { 0: engineMessage({ ...f, message: f.error ?? '' }) })}
      </p>
      {!cancelled && <p className="muted small">{tr('임시 폴더는 지웠습니다. 선택한 위치에는 아무것도 남지 않았습니다.')}</p>}
      <div className="actions"><button type="button" data-package-back onClick={() => set('stage', 'choose')}>{tr('설정으로')}</button></div>
    </div>
  );
}

function Done({ result: r }: { result: PackageResult }) {
  const missing = r.links.missing + r.links.ambiguous;
  const pdf = r.pdf;
  return (
    <div className="pkg-result" data-package-done>
      <p>
        <b>{baseName(r.dir)}</b> {tr(' — 파일 ')}{r.files}{tr('개, ')}{size(r.bytes)}, {(r.ms / 1000).toFixed(1)}{tr('초')}</p>
      <ul className="pkg-lines">
        <li>{tr('문서 ')}{r.document.name} {r.document.sha256 && <span className="muted">· SHA-256 {r.document.sha256.slice(0, 12)}…</span>}</li>
        <li>
          {tr('링크 ')}{r.links.copied}{tr('개 복사 (')}{size(r.links.bytes)})
          {missing > 0 && <span className="bad">{tr(' · 없음 {0}곳 (보고서에 적음)', { 0: missing })}</span>}
          {r.links.renamed > 0 && <span className="warn">{tr(' · 이름 바꿈 {0}', { 0: r.links.renamed })}</span>}
          {r.links.failed > 0 && <span className="bad">{tr(' · 복사 실패 {0}', { 0: r.links.failed })}</span>}
        </li>
        <li>
          {tr('글꼴 파일 ')}{r.fonts.files}{tr('개 복사 (쓰인 글꼴 ')}{r.fonts.used}{tr('개 중 ')}{r.fonts.copied}{tr('개)')}{r.fonts.notCopied > 0 && <span className="warn">{tr(' · 복사하지 않음 {0}', { 0: r.fonts.notCopied })}</span>}
          {r.fonts.missing + r.fonts.substituted > 0 && <span className="bad">{tr(' · 없음 {0}{1}', { 0: r.fonts.missing, 1: r.fonts.substituted ? tr(' · 대체 {0}', { 0: r.fonts.substituted }) : '' })}</span>}
        </li>
        <li>
          {pdf === null ? tr('PDF 넣지 않음') : 'error' in pdf ? <span className="bad">{tr('PDF를 만들지 못했습니다: {0}', { 0: pdf.error })}</span> : <>{tr('{0} {1}쪽, {2}{3}', { 0: (pdf.standard ?? 'PDF/X-4').split(' (')[0], 1: pdf.pages, 2: size(pdf.bytes), 3: pdf.flatten && pdf.flatten.pages > 0 ? tr(' · 투명도 병합 {0}쪽', { 0: pdf.flatten.pages }) : '' })}</>}
        </li>
        {r.preflight.errors > 0 && <li className="warn">{tr('사전 검사 오류 {0}개 (보고서에 적음)', { 0: r.preflight.errors })}</li>}
      </ul>
      {r.failures.length > 0 && (
        <ul className="warnings">
          {r.failures.map((f, i) => (
            <li key={i}>{f.dest}: {f.error}</li>
          ))}
        </ul>
      )}
      <p className="muted small" title={r.dir}>{r.dir}</p>
      <div className="actions">
        {window.folio.openPath && <button type="button" onClick={() => window.folio.openPath(r.report)}>{tr('보고서 열기')}</button>}
        <button type="button" onClick={() => window.folio.reveal(r.report)}>{tr('폴더에서 보기')}</button>
      </div>
    </div>
  );
}

function exportText(e: Export | undefined) {
  if (!e || !e.marks) return '';
  const marks = [e.marks.crop && tr('재단선'), e.marks.bleed && tr('도련 표시'), e.marks.registration && tr('맞춰찍기'), e.marks.pageInfo && tr('페이지 정보')].filter(Boolean);
  const bleed = e.bleed ? tr('문서 도련') : e.settings?.bleed.some(v => v > 0) ? tr('도련') + ': ' + e.settings.bleed.join(' / ') + ' pt' : tr('도련 없음');
  return [e.settings ? presetName(e.settings) : presetOf(e.preset).name, bleed, e.spreads ? tr('펼침면') : tr('낱쪽'), marks.length ? marks.join('·') : tr('표시 없음')].join(' · ');
}

function baseName(p: string) {
  return p.split(/[\\/]/).pop() ?? p;
}

function size(bytes: number) {
  if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(2)} GB`;
  if (bytes >= 1 << 20) return `${(bytes / (1 << 20)).toFixed(1)} MB`;
  return `${Math.ceil(bytes / 1024)} KB`;
}
