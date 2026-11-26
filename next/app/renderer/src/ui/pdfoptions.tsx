import { useCallback, useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { t as tr } from '../i18n';
import type { PdfOptions, PlaceInfo } from '../engine';

type Choice = { pages: number[]; options: Omit<PdfOptions, 'pageNumber'> };
type Request = { file: string; info: PlaceInfo; done: (choice: Choice | null) => void };

export function parsePdfPages(text: string, count: number): number[] | null {
  if (!Number.isInteger(count) || count < 1) return null;
  const pages: number[] = [];
  for (const token of text.split(',')) {
    const match = /^\s*(\d+)\s*(?:-\s*(\d+)\s*)?$/.exec(token);
    if (!match) return null;
    const first = Number(match[1]), last = Number(match[2] ?? match[1]);
    if (!Number.isSafeInteger(first) || !Number.isSafeInteger(last) || first < 1 || last < first || last > count) return null;
    for (let n = first; n <= last; n++) if (!pages.includes(n)) pages.push(n);
  }
  return pages.length ? pages : null;
}

const crops = (): [PdfOptions['crop'], string][] => [
  ['CropPDF', tr('자르기 상자')], ['CropContentVisibleLayers', tr('보이는 내용 경계')],
  ['CropContentAllLayers', tr('모든 내용 경계')], ['CropArt', tr('아트 상자')],
  ['CropTrim', tr('재단 상자')], ['CropBleed', tr('도련 상자')], ['CropMedia', tr('매체 상자')],
];

function PdfImportDialog({ request }: { request: Request }) {
  const [mode, setMode] = useState('one');
  const [range, setRange] = useState('1');
  const [crop, setCrop] = useState<PdfOptions['crop']>('CropPDF');
  const [transparent, setTransparent] = useState(true);
  const count = request.info.pages ?? 0;
  // fonts the PDF does not carry: shown with this computer's fonts, not as InDesign shows them
  const loose = (request.info.fonts ?? []).filter((f) => !f.embedded).map((f) => f.name);
  const pages = mode === 'all' ? Array.from({ length: count }, (_, i) => i + 1) : parsePdfPages(range, count);
  const valid = pages && (mode !== 'one' || pages.length === 1);
  const title = tr('PDF 가져오기 옵션');
  return createPortal(<div className="style-modal" role="dialog" aria-label={title}
    onPointerDown={(e) => e.stopPropagation()} onKeyDown={(e) => { e.stopPropagation(); if (e.key === 'Escape') request.done(null); }}>
    <form className="card dialog style-options" style={{ width: 420 }} onSubmit={(e) => {
      e.preventDefault(); if (valid) request.done({ pages, options: { crop, transparentBackground: transparent } });
    }}>
      <h2>{title}</h2>
      <p style={{ overflowWrap: 'anywhere' }}>{request.file.split(/[/\\]/).pop()}</p>
      <p>{tr('전체 {count}쪽', { count })}</p>
      <label>{tr('페이지 선택')}<select aria-label={tr('페이지 선택')} value={mode} onChange={(e) => setMode(e.target.value)}>
        <option value="one">{tr('한 쪽')}</option><option value="range">{tr('여러 쪽')}</option><option value="all">{tr('모두')}</option>
      </select></label>
      {mode !== 'all' && <label>{tr('PDF 쪽 번호')}<input autoFocus aria-label={tr('PDF 쪽 번호')} value={range} onChange={(e) => setRange(e.target.value)} /></label>}
      {mode === 'range' && <p>{tr('예: 1, 3-5. 놓을 때마다 다음 쪽으로 넘어갑니다.')}</p>}
      {!valid && <p role="alert">{tr('1부터 {count} 사이의 쪽 번호를 입력하세요.', { count })}</p>}
      <label>{tr('자르기 기준')}<select aria-label={tr('자르기 기준')} value={crop} onChange={(e) => setCrop(e.target.value as PdfOptions['crop'])}>
        {crops().map(([value, label]) => <option key={value} value={value}>{label}</option>)}
      </select></label>
      {crop.startsWith('CropContent') && <p>{tr('레이어가 있는 PDF의 내용 경계는 레이어 설정을 따로 반영하지 않습니다.')}</p>}
      {loose.length > 0 && <p role="note">{tr('PDF에 들어 있지 않은 글꼴({list})은 이 컴퓨터의 글꼴로 대신 보입니다.', { list: loose.join(', ') })}</p>}
      <label style={{ justifyContent: 'flex-start', gap: 8 }}><input type="checkbox" checked={transparent} onChange={(e) => setTransparent(e.target.checked)} />{tr('투명 배경')}</label>
      <footer style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}><button type="button" onClick={() => request.done(null)}>{tr('취소')}</button><button type="submit" disabled={!valid}>{tr('확인')}</button></footer>
    </form>
  </div>, document.body);
}

export function usePdfOptions(document: string | undefined) {
  const [request, setRequest] = useState<Request | null>(null);
  const resolver = useRef<((choice: Choice | null) => void) | null>(null);
  const done = useCallback((choice: Choice | null) => {
    const resolve = resolver.current; resolver.current = null; setRequest(null); resolve?.(choice);
  }, []);
  useEffect(() => () => { resolver.current?.(null); resolver.current = null; }, []);
  useEffect(() => { done(null); }, [document, done]);
  const choose = useCallback((file: string, info: PlaceInfo) => new Promise<Choice | null>((resolve) => {
    resolver.current?.(null); resolver.current = resolve; setRequest({ file, info, done });
  }), [done]);
  return { choose, cancel: () => done(null), dialog: request ? <PdfImportDialog key={request.file} request={request} /> : null };
}
